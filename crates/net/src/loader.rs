//! Tokio, Hyper, and Rustls loading pipeline.

use std::{fmt, time::Instant};

use bytes::{Bytes, BytesMut};
use http::{
    Method, StatusCode,
    header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION, USER_AGENT},
};
use http_body_util::{BodyExt, Full};
use hyper::{Request as HyperRequest, body::Incoming};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use meow_url_policy::BrowserUrl;
use tokio::time::timeout;

use super::{
    cancellation::CancellationToken,
    error::NetError,
    model::{LoadConfig, RedirectHop, Request, Response, ResponseMetadata},
};

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
