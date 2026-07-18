//! HTTP/TLS resource loading with redirects, limits, timeouts, and cancellation.

use std::{
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use http::{
    HeaderMap, Method, StatusCode, Version,
    header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, LOCATION, USER_AGENT},
};
use http_body_util::{BodyExt, Full};
use hyper::{Request as HyperRequest, body::Incoming};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, Error as ClientError, connect::HttpConnector},
    rt::TokioExecutor,
};
use meow_url_policy::{BrowserUrl, UrlPolicyError};
use tokio::{sync::Notify, time::timeout};

/// Default upper bound for a single response body.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Default redirect hop limit.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// An owned request model that does not expose Hyper types at engine boundaries.
#[derive(Clone, Debug)]
pub struct Request {
    /// HTTP method.
    pub method: Method,
    /// Canonical request URL.
    pub url: BrowserUrl,
    /// Request headers.
    pub headers: HeaderMap,
    /// Buffered request body.
    pub body: Bytes,
}

impl Request {
    /// Creates a GET request with browser-oriented document defaults.
    #[must_use]
    pub fn get(url: BrowserUrl) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            "text/html,application/xhtml+xml,*/*;q=0.8".parse().unwrap(),
        );
        Self {
            method: Method::GET,
            url,
            headers,
            body: Bytes::new(),
        }
    }

    /// Creates a GET request for an external CSS stylesheet.
    #[must_use]
    pub fn stylesheet(url: BrowserUrl) -> Self {
        let mut request = Self::get(url);
        request
            .headers
            .insert(ACCEPT, "text/css,*/*;q=0.1".parse().unwrap());
        request
    }
}

/// A completed response and its body bytes.
#[derive(Clone, Debug)]
pub struct Response {
    /// Final response status.
    pub status: StatusCode,
    /// Final response headers.
    pub headers: HeaderMap,
    /// Buffered response body, bounded by loader policy.
    pub body: Bytes,
    /// Timing, protocol, redirect, and URL metadata.
    pub metadata: ResponseMetadata,
}

/// Metadata retained for navigation and diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseMetadata {
    /// URL before following redirects.
    pub requested_url: BrowserUrl,
    /// URL that produced the final response.
    pub final_url: BrowserUrl,
    /// Redirect hops in traversal order.
    pub redirects: Vec<RedirectHop>,
    /// Negotiated HTTP protocol version.
    pub http_version: HttpVersion,
    /// Parsed Content-Type header value, if valid UTF-8.
    pub content_type: Option<String>,
    /// Declared Content-Length header value, if valid.
    pub declared_content_length: Option<u64>,
    /// Number of body bytes retained.
    pub received_bytes: usize,
    /// Total elapsed load time in milliseconds.
    pub elapsed_millis: u128,
}

/// One redirect followed by the loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedirectHop {
    /// Source URL.
    pub from: BrowserUrl,
    /// Redirect status code.
    pub status: StatusCode,
    /// Resolved destination URL.
    pub to: BrowserUrl,
}

/// Stable HTTP version representation for engine metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpVersion {
    /// HTTP/0.9.
    Http09,
    /// HTTP/1.0.
    Http10,
    /// HTTP/1.1.
    Http11,
    /// HTTP/2.
    Http2,
    /// HTTP/3, when provided by a future connector.
    Http3,
    /// A version unknown to this engine build.
    Other,
}

impl From<Version> for HttpVersion {
    fn from(version: Version) -> Self {
        match version {
            Version::HTTP_09 => Self::Http09,
            Version::HTTP_10 => Self::Http10,
            Version::HTTP_11 => Self::Http11,
            Version::HTTP_2 => Self::Http2,
            Version::HTTP_3 => Self::Http3,
            _ => Self::Other,
        }
    }
}

/// Loader limits and timeout policy.
#[derive(Clone, Debug)]
pub struct LoadConfig {
    /// TCP/TLS connection timeout.
    pub connect_timeout: Duration,
    /// Timeout for each request and response-body phase.
    pub request_timeout: Duration,
    /// Maximum retained response body size.
    pub max_response_bytes: usize,
    /// Maximum number of redirects.
    pub max_redirects: usize,
    /// User-Agent sent when the request does not provide one.
    pub user_agent: String,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            user_agent: format!("MeowEngine/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// Cloneable cooperative cancellation primitive for network and navigation work.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancellationToken {
    /// Creates a token in the active state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels all current and future waiters.
    pub fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    /// Returns true after cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Resolves when cancellation is requested.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Tokio/Hyper/Rustls loader. DNS resolution is provided by Hyper's Tokio connector.
#[derive(Clone)]
pub struct Loader {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    config: LoadConfig,
}

impl fmt::Debug for Loader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Loader")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Loader {
    /// Creates a loader using secure defaults and Mozilla WebPKI roots.
    #[must_use]
    pub fn new(config: LoadConfig) -> Self {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        http.set_connect_timeout(Some(config.connect_timeout));
        http.set_nodelay(true);

        let https = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(http);
        let client = Client::builder(TokioExecutor::new()).build(https);
        Self { client, config }
    }

    /// Returns the active loader policy.
    #[must_use]
    pub const fn config(&self) -> &LoadConfig {
        &self.config
    }

    /// Loads one HTTP(S) request and retains response metadata.
    pub async fn load(
        &self,
        mut request: Request,
        cancellation: &CancellationToken,
    ) -> Result<Response, NetError> {
        if !request.url.is_http_family() {
            return Err(NetError::UnsupportedScheme(request.url.scheme().to_owned()));
        }
        if cancellation.is_cancelled() {
            return Err(NetError::Cancelled);
        }

        let started = Instant::now();
        let requested_url = request.url.clone();
        let mut redirects = Vec::new();

        loop {
            let response = self.send_once(&request, cancellation).await?;
            let status = response.status();

            if is_redirect(status) {
                let Some(location) = response.headers().get(LOCATION) else {
                    return self
                        .finish_response(
                            requested_url,
                            request.url,
                            redirects,
                            response,
                            started,
                            cancellation,
                        )
                        .await;
                };
                let location = location
                    .to_str()
                    .map_err(|_| NetError::InvalidRedirectLocation)?;
                if redirects.len() >= self.config.max_redirects {
                    return Err(NetError::TooManyRedirects {
                        limit: self.config.max_redirects,
                    });
                }
                let destination = request.url.resolve(location)?;
                if !destination.is_http_family() {
                    return Err(NetError::UnsupportedScheme(destination.scheme().to_owned()));
                }

                redirects.push(RedirectHop {
                    from: request.url.clone(),
                    status,
                    to: destination.clone(),
                });
                rewrite_request_for_redirect(&mut request, status, destination);
                continue;
            }

            return self
                .finish_response(
                    requested_url,
                    request.url,
                    redirects,
                    response,
                    started,
                    cancellation,
                )
                .await;
        }
    }

    async fn send_once(
        &self,
        request: &Request,
        cancellation: &CancellationToken,
    ) -> Result<hyper::Response<Incoming>, NetError> {
        let uri = request.url.as_str().parse::<http::Uri>()?;
        let mut builder = HyperRequest::builder()
            .method(request.method.clone())
            .uri(uri);
        let headers = builder.headers_mut().expect("request builder has headers");
        *headers = request.headers.clone();
        if !headers.contains_key(USER_AGENT) {
            headers.insert(
                USER_AGENT,
                self.config
                    .user_agent
                    .parse()
                    .expect("configured user agent must be a valid header value"),
            );
        }
        let request = builder.body(Full::new(request.body.clone()))?;
        let send = self.client.request(request);

        tokio::select! {
            _ = cancellation.cancelled() => Err(NetError::Cancelled),
            result = timeout(self.config.request_timeout, send) => {
                result.map_err(|_| NetError::Timeout)?.map_err(NetError::Client)
            }
        }
    }

    async fn finish_response(
        &self,
        requested_url: BrowserUrl,
        final_url: BrowserUrl,
        redirects: Vec<RedirectHop>,
        response: hyper::Response<Incoming>,
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Response, NetError> {
        let status = response.status();
        let version = response.version();
        let headers = response.headers().clone();
        let body = self.read_body(response.into_body(), cancellation).await?;
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let declared_content_length = headers
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());

        Ok(Response {
            status,
            headers,
            metadata: ResponseMetadata {
                requested_url,
                final_url,
                redirects,
                http_version: version.into(),
                content_type,
                declared_content_length,
                received_bytes: body.len(),
                elapsed_millis: started.elapsed().as_millis(),
            },
            body,
        })
    }

    async fn read_body(
        &self,
        mut body: Incoming,
        cancellation: &CancellationToken,
    ) -> Result<Bytes, NetError> {
        let max_bytes = self.config.max_response_bytes;
        let read = async move {
            let mut output = BytesMut::new();
            while let Some(frame) = body.frame().await {
                let frame = frame.map_err(NetError::Body)?;
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                let next_len = output
                    .len()
                    .checked_add(data.len())
                    .ok_or(NetError::ResponseTooLarge { limit: max_bytes })?;
                if next_len > max_bytes {
                    return Err(NetError::ResponseTooLarge { limit: max_bytes });
                }
                output.extend_from_slice(&data);
            }
            Ok(output.freeze())
        };

        tokio::select! {
            _ = cancellation.cancelled() => Err(NetError::Cancelled),
            result = timeout(self.config.request_timeout, read) => {
                result.map_err(|_| NetError::Timeout)?
            }
        }
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new(LoadConfig::default())
    }
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn rewrite_request_for_redirect(
    request: &mut Request,
    status: StatusCode,
    destination: BrowserUrl,
) {
    let switch_to_get = status == StatusCode::SEE_OTHER
        || ((status == StatusCode::MOVED_PERMANENTLY || status == StatusCode::FOUND)
            && request.method == Method::POST);
    if switch_to_get && request.method != Method::HEAD {
        request.method = Method::GET;
        request.body = Bytes::new();
        request.headers.remove(CONTENT_LENGTH);
        request.headers.remove(http::header::CONTENT_TYPE);
    }
    request.url = destination;
}

/// Network loader failure.
#[derive(Debug)]
pub enum NetError {
    /// URL scheme is not loadable by this HTTP loader.
    UnsupportedScheme(String),
    /// URL serialization could not be converted to an HTTP URI.
    InvalidUri(http::uri::InvalidUri),
    /// Hyper request construction failed.
    BuildRequest(http::Error),
    /// Hyper client request failed.
    Client(ClientError),
    /// Hyper response body failed.
    Body(hyper::Error),
    /// The configured request phase timeout elapsed.
    Timeout,
    /// The operation was cancelled.
    Cancelled,
    /// The body exceeded its configured byte limit.
    ResponseTooLarge { limit: usize },
    /// Redirect count exceeded policy.
    TooManyRedirects { limit: usize },
    /// Location header was not valid text.
    InvalidRedirectLocation,
    /// Redirect URL reference was invalid.
    Url(UrlPolicyError),
}

impl fmt::Display for NetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme(scheme) => {
                write!(formatter, "unsupported URL scheme: {scheme}")
            }
            Self::InvalidUri(error) => write!(formatter, "invalid HTTP URI: {error}"),
            Self::BuildRequest(error) => write!(formatter, "could not build HTTP request: {error}"),
            Self::Client(error) => write!(formatter, "HTTP request failed: {error}"),
            Self::Body(error) => write!(formatter, "HTTP response body failed: {error}"),
            Self::Timeout => formatter.write_str("network operation timed out"),
            Self::Cancelled => formatter.write_str("network operation was cancelled"),
            Self::ResponseTooLarge { limit } => {
                write!(formatter, "response body exceeded {limit} byte limit")
            }
            Self::TooManyRedirects { limit } => {
                write!(formatter, "redirect count exceeded limit of {limit}")
            }
            Self::InvalidRedirectLocation => {
                formatter.write_str("redirect Location header was not valid text")
            }
            Self::Url(error) => error.fmt(formatter),
        }
    }
}

impl Error for NetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidUri(error) => Some(error),
            Self::BuildRequest(error) => Some(error),
            Self::Client(error) => Some(error),
            Self::Body(error) => Some(error),
            Self::Url(error) => Some(error),
            _ => None,
        }
    }
}

impl From<http::uri::InvalidUri> for NetError {
    fn from(error: http::uri::InvalidUri) -> Self {
        Self::InvalidUri(error)
    }
}

impl From<http::Error> for NetError {
    fn from(error: http::Error) -> Self {
        Self::BuildRequest(error)
    }
}

impl From<UrlPolicyError> for NetError {
    fn from(error: UrlPolicyError) -> Self {
        Self::Url(error)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        task::JoinHandle,
    };

    use super::*;

    #[tokio::test]
    async fn loads_response_and_retains_metadata() {
        let server = TestServer::spawn().await;
        let response = Loader::default()
            .load(Request::get(server.url("/ok")), &CancellationToken::new())
            .await
            .expect("response should load");

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body, Bytes::from_static(b"hello meow"));
        assert_eq!(response.metadata.received_bytes, 10);
        assert_eq!(
            response.metadata.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(response.metadata.declared_content_length, Some(10));
        assert_eq!(response.metadata.http_version, HttpVersion::Http11);
        assert!(response.metadata.redirects.is_empty());
    }

    #[tokio::test]
    async fn follows_relative_redirects() {
        let server = TestServer::spawn().await;
        let response = Loader::default()
            .load(
                Request::get(server.url("/redirect")),
                &CancellationToken::new(),
            )
            .await
            .expect("redirect should load");

        assert_eq!(response.body, Bytes::from_static(b"hello meow"));
        assert_eq!(response.metadata.redirects.len(), 1);
        assert_eq!(response.metadata.redirects[0].status, StatusCode::FOUND);
        assert_eq!(response.metadata.final_url, server.url("/ok"));
    }

    #[tokio::test]
    async fn enforces_response_byte_limit() {
        let server = TestServer::spawn().await;
        let config = LoadConfig {
            max_response_bytes: 8,
            ..LoadConfig::default()
        };
        let error = Loader::new(config)
            .load(
                Request::get(server.url("/large")),
                &CancellationToken::new(),
            )
            .await
            .expect_err("large body should fail");

        assert!(matches!(error, NetError::ResponseTooLarge { limit: 8 }));
    }

    #[tokio::test]
    async fn enforces_timeout_and_cancellation() {
        let server = TestServer::spawn().await;
        let config = LoadConfig {
            request_timeout: Duration::from_millis(30),
            ..LoadConfig::default()
        };
        let timeout_error = Loader::new(config)
            .load(Request::get(server.url("/slow")), &CancellationToken::new())
            .await
            .expect_err("slow response should time out");
        assert!(matches!(timeout_error, NetError::Timeout));

        let token = CancellationToken::new();
        let cancellation = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation.cancel();
        });
        let cancelled_error = Loader::default()
            .load(Request::get(server.url("/slow")), &token)
            .await
            .expect_err("cancelled response should fail");
        assert!(matches!(cancelled_error, NetError::Cancelled));
    }

    struct TestServer {
        address: SocketAddr,
        task: JoinHandle<()>,
    }

    impl TestServer {
        async fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("test listener should bind");
            let address = listener.local_addr().unwrap();
            let task = tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
                    tokio::spawn(handle_connection(stream));
                }
            });
            Self { address, task }
        }

        fn url(&self, path: &str) -> BrowserUrl {
            BrowserUrl::parse(&format!("http://{}{path}", self.address)).unwrap()
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn handle_connection(mut stream: TcpStream) {
        let mut request = vec![0_u8; 4096];
        let Ok(read) = stream.read(&mut request).await else {
            return;
        };
        let request = String::from_utf8_lossy(&request[..read]);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");

        let response = match path {
            "/ok" => "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 10\r\nConnection: close\r\n\r\nhello meow".to_owned(),
            "/redirect" => "HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            "/large" => format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 64\r\nConnection: close\r\n\r\n{}",
                "x".repeat(64)
            ),
            "/slow" => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nslow".to_owned()
            }
            _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
        };
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }
}
