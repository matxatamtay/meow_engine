use std::{
    fmt,
    future::Future,
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use meow_ipc::{Connection, Envelope, MessageKind, RemoteError, RequestId, StreamTransport};
use meow_net::{
    CancellationToken, CredentialsMode, HttpVersion, LoadConfig, Loader, NetError, RedirectHop,
    Request, RequestBroker, RequestContext, Response, ResponseMetadata,
};
use meow_url_policy::BrowserUrl;
use serde::{Deserialize, Serialize};

use crate::ProcessError;

const MAX_BROKER_REQUEST_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct NetworkBrokerClient {
    inner: Arc<NetworkClientInner>,
}

struct NetworkClientInner {
    connection: Mutex<Connection<StreamTransport<UnixStream>>>,
    next_request_id: AtomicU64,
}

impl fmt::Debug for NetworkBrokerClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkBrokerClient")
            .field(
                "next_request_id",
                &self.inner.next_request_id.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl NetworkBrokerClient {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, ProcessError> {
        let stream = UnixStream::connect(path)?;
        Ok(Self {
            inner: Arc::new(NetworkClientInner {
                connection: Mutex::new(Connection::new(StreamTransport::new(stream))),
                next_request_id: AtomicU64::new(1),
            }),
        })
    }

    fn round_trip(&self, request: WireNetworkRequest) -> Result<WireNetworkResponse, ProcessError> {
        let request_id = RequestId(self.inner.next_request_id.fetch_add(1, Ordering::Relaxed));
        let mut connection = self
            .inner
            .connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        connection.send(&Envelope::request(request_id, request))?;
        let response: Envelope<WireNetworkResponse> = connection.receive()?;
        if response.kind != MessageKind::Response {
            return Err(ProcessError::Protocol(format!(
                "network broker returned {:?} instead of response",
                response.kind
            )));
        }
        if response.request_id != request_id {
            return Err(ProcessError::Protocol(format!(
                "network request ID mismatch: expected {}, got {}",
                request_id.0, response.request_id.0
            )));
        }
        if let Some(error) = response.error {
            return Err(ProcessError::Remote(error));
        }
        response
            .payload
            .ok_or_else(|| ProcessError::Protocol("network response omitted payload".to_owned()))
    }
}

impl RequestBroker for NetworkBrokerClient {
    fn load<'a>(
        &'a self,
        request: Request,
        context: RequestContext,
        cancellation: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Response, NetError>> + Send + 'a>> {
        let client = self.clone();
        let cancellation = cancellation.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(NetError::Cancelled);
            }
            let wire = WireNetworkRequest::Load {
                request: WireHttpRequest::from_request(request),
                context: WireRequestContext::from_context(context),
            };
            let result = tokio::task::spawn_blocking(move || client.round_trip(wire))
                .await
                .map_err(|error| NetError::Broker(error.to_string()))?;
            if cancellation.is_cancelled() {
                return Err(NetError::Cancelled);
            }
            match result {
                Ok(WireNetworkResponse::Loaded { response }) => response
                    .into_response()
                    .map_err(|error| NetError::Broker(error.to_string())),
                Ok(WireNetworkResponse::Ack) => Err(NetError::Broker(
                    "network broker returned ack for load".to_owned(),
                )),
                Err(ProcessError::Remote(error)) if error.code == "permission_denied" => {
                    Err(NetError::PermissionDenied(error.message))
                }
                Err(error) => Err(NetError::Broker(error.to_string())),
            }
        })
    }
}

pub fn run_network_process(socket_path: impl AsRef<Path>) -> Result<(), ProcessError> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let loader = Loader::new(LoadConfig::default());
    loop {
        let (stream, _) = listener.accept()?;
        if serve_network_connection(stream, &runtime, &loader)? {
            break;
        }
    }
    let _ = std::fs::remove_file(socket_path);
    Ok(())
}

fn serve_network_connection(
    stream: UnixStream,
    runtime: &tokio::runtime::Runtime,
    loader: &Loader,
) -> Result<bool, ProcessError> {
    let mut connection = Connection::new(StreamTransport::new(stream));
    loop {
        let request: Envelope<WireNetworkRequest> = match connection.receive() {
            Ok(request) => request,
            Err(meow_ipc::IpcError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error.into()),
        };
        if request.kind != MessageKind::Request {
            connection.send(&Envelope::<WireNetworkResponse>::failure(
                request.request_id,
                RemoteError::new("unexpected_message", "network broker accepts requests only"),
            ))?;
            continue;
        }
        let Some(payload) = request.payload else {
            connection.send(&Envelope::<WireNetworkResponse>::failure(
                request.request_id,
                RemoteError::new("invalid_request", "network request omitted payload"),
            ))?;
            continue;
        };
        match payload {
            WireNetworkRequest::Stop => {
                connection.send(&Envelope::response(
                    request.request_id,
                    WireNetworkResponse::Ack,
                ))?;
                return Ok(true);
            }
            WireNetworkRequest::Load {
                request: wire,
                context,
            } => {
                let result = wire
                    .into_request()
                    .and_then(|request| Ok((request, context.into_context()?)))
                    .and_then(|(request, context)| {
                        runtime
                            .block_on(loader.load_with_context(
                                request,
                                context,
                                &CancellationToken::new(),
                            ))
                            .map_err(|error| ProcessError::Protocol(error.to_string()))
                    });
                match result {
                    Ok(response) => connection.send(&Envelope::response(
                        request.request_id,
                        WireNetworkResponse::Loaded {
                            response: Box::new(WireHttpResponse::from_response(response)),
                        },
                    ))?,
                    Err(ProcessError::Protocol(message))
                        if message.starts_with("permission denied:") =>
                    {
                        connection.send(&Envelope::<WireNetworkResponse>::failure(
                            request.request_id,
                            RemoteError::new("permission_denied", message),
                        ))?;
                    }
                    Err(error) => connection.send(&Envelope::<WireNetworkResponse>::failure(
                        request.request_id,
                        RemoteError::new("load_failed", error.to_string()).retryable(true),
                    ))?,
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum WireNetworkRequest {
    Load {
        request: WireHttpRequest,
        context: WireRequestContext,
    },
    Stop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
enum WireNetworkResponse {
    Loaded { response: Box<WireHttpResponse> },
    Ack,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WireHttpRequest {
    method: String,
    url: String,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
}

impl WireHttpRequest {
    fn from_request(request: Request) -> Self {
        Self {
            method: request.method.to_string(),
            url: request.url.to_string(),
            headers: request
                .headers
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
                .collect(),
            body: request.body.to_vec(),
        }
    }

    fn into_request(self) -> Result<Request, ProcessError> {
        if self.body.len() > MAX_BROKER_REQUEST_BYTES {
            return permission_denied(format!(
                "request body is {} bytes, limit is {MAX_BROKER_REQUEST_BYTES}",
                self.body.len()
            ));
        }
        let method = Method::from_bytes(self.method.as_bytes())
            .map_err(|error| ProcessError::Protocol(error.to_string()))?;
        if matches!(method, Method::CONNECT | Method::TRACE) {
            return permission_denied(format!("HTTP method {method} is forbidden"));
        }
        let url = BrowserUrl::parse(&self.url)
            .map_err(|error| ProcessError::Protocol(error.to_string()))?;
        if !url.is_http_family() {
            return permission_denied(format!("URL scheme {} is not brokerable", url.scheme()));
        }
        let mut headers = HeaderMap::new();
        for (name, value) in self.headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|error| ProcessError::Protocol(error.to_string()))?;
            if forbidden_broker_header(&name) {
                return permission_denied(format!("header {name} is forbidden"));
            }
            let value = HeaderValue::from_bytes(&value)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?;
            headers.append(name, value);
        }
        Ok(Request {
            method,
            url,
            headers,
            body: Bytes::from(self.body),
        })
    }
}

fn forbidden_broker_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "host"
            | "cookie"
            | "cookie2"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    ) || name.as_str().starts_with("proxy-")
}

fn permission_denied<T>(message: String) -> Result<T, ProcessError> {
    Err(ProcessError::Protocol(format!(
        "permission denied: {message}"
    )))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WireRequestContext {
    document_url: Option<String>,
    credentials: WireCredentialsMode,
}

impl WireRequestContext {
    fn from_context(context: RequestContext) -> Self {
        Self {
            document_url: context.document_url.map(|url| url.to_string()),
            credentials: context.credentials.into(),
        }
    }

    fn into_context(self) -> Result<RequestContext, ProcessError> {
        Ok(RequestContext {
            document_url: self
                .document_url
                .map(|url| BrowserUrl::parse(&url))
                .transpose()
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            credentials: self.credentials.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireCredentialsMode {
    Omit,
    SameOrigin,
    Include,
}

impl From<CredentialsMode> for WireCredentialsMode {
    fn from(value: CredentialsMode) -> Self {
        match value {
            CredentialsMode::Omit => Self::Omit,
            CredentialsMode::SameOrigin => Self::SameOrigin,
            CredentialsMode::Include => Self::Include,
        }
    }
}

impl From<WireCredentialsMode> for CredentialsMode {
    fn from(value: WireCredentialsMode) -> Self {
        match value {
            WireCredentialsMode::Omit => Self::Omit,
            WireCredentialsMode::SameOrigin => Self::SameOrigin,
            WireCredentialsMode::Include => Self::Include,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WireHttpResponse {
    status: u16,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
    metadata: WireResponseMetadata,
}

impl WireHttpResponse {
    fn from_response(response: Response) -> Self {
        Self {
            status: response.status.as_u16(),
            headers: response
                .headers
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
                .collect(),
            body: response.body.to_vec(),
            metadata: WireResponseMetadata::from_metadata(response.metadata),
        }
    }

    fn into_response(self) -> Result<Response, ProcessError> {
        let status = StatusCode::from_u16(self.status)
            .map_err(|error| ProcessError::Protocol(error.to_string()))?;
        let mut headers = HeaderMap::new();
        for (name, value) in self.headers {
            headers.append(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|error| ProcessError::Protocol(error.to_string()))?,
                HeaderValue::from_bytes(&value)
                    .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            );
        }
        Ok(Response {
            status,
            headers,
            body: Bytes::from(self.body),
            metadata: self.metadata.into_metadata()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WireResponseMetadata {
    requested_url: String,
    final_url: String,
    redirects: Vec<WireRedirect>,
    http_version: WireHttpVersion,
    content_type: Option<String>,
    declared_content_length: Option<u64>,
    received_bytes: usize,
    elapsed_millis: u64,
}

impl WireResponseMetadata {
    fn from_metadata(metadata: ResponseMetadata) -> Self {
        Self {
            requested_url: metadata.requested_url.to_string(),
            final_url: metadata.final_url.to_string(),
            redirects: metadata
                .redirects
                .into_iter()
                .map(WireRedirect::from_redirect)
                .collect(),
            http_version: metadata.http_version.into(),
            content_type: metadata.content_type,
            declared_content_length: metadata.declared_content_length,
            received_bytes: metadata.received_bytes,
            elapsed_millis: u64::try_from(metadata.elapsed_millis).unwrap_or(u64::MAX),
        }
    }

    fn into_metadata(self) -> Result<ResponseMetadata, ProcessError> {
        Ok(ResponseMetadata {
            requested_url: BrowserUrl::parse(&self.requested_url)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            final_url: BrowserUrl::parse(&self.final_url)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            redirects: self
                .redirects
                .into_iter()
                .map(WireRedirect::into_redirect)
                .collect::<Result<Vec<_>, _>>()?,
            http_version: self.http_version.into(),
            content_type: self.content_type,
            declared_content_length: self.declared_content_length,
            received_bytes: self.received_bytes,
            elapsed_millis: u128::from(self.elapsed_millis),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WireRedirect {
    from: String,
    status: u16,
    to: String,
}

impl WireRedirect {
    fn from_redirect(redirect: RedirectHop) -> Self {
        Self {
            from: redirect.from.to_string(),
            status: redirect.status.as_u16(),
            to: redirect.to.to_string(),
        }
    }

    fn into_redirect(self) -> Result<RedirectHop, ProcessError> {
        Ok(RedirectHop {
            from: BrowserUrl::parse(&self.from)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            status: StatusCode::from_u16(self.status)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            to: BrowserUrl::parse(&self.to)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireHttpVersion {
    Http09,
    Http10,
    Http11,
    Http2,
    Http3,
    Other,
}

impl From<HttpVersion> for WireHttpVersion {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => Self::Http09,
            HttpVersion::Http10 => Self::Http10,
            HttpVersion::Http11 => Self::Http11,
            HttpVersion::Http2 => Self::Http2,
            HttpVersion::Http3 => Self::Http3,
            HttpVersion::Other => Self::Other,
        }
    }
}

impl From<WireHttpVersion> for HttpVersion {
    fn from(value: WireHttpVersion) -> Self {
        match value {
            WireHttpVersion::Http09 => Self::Http09,
            WireHttpVersion::Http10 => Self::Http10,
            WireHttpVersion::Http11 => Self::Http11,
            WireHttpVersion::Http2 => Self::Http2,
            WireHttpVersion::Http3 => Self::Http3,
            WireHttpVersion::Other => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_policy_rejects_raw_cookie_and_connect() {
        let request = WireHttpRequest {
            method: "CONNECT".to_owned(),
            url: "https://example.test/".to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
        };
        assert!(request.into_request().is_err());

        let request = WireHttpRequest {
            method: "GET".to_owned(),
            url: "https://example.test/".to_owned(),
            headers: vec![("cookie".to_owned(), b"secret=1".to_vec())],
            body: Vec::new(),
        };
        assert!(request.into_request().is_err());
    }
}
