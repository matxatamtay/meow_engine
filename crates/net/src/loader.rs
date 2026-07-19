//! Tokio/Hyper/Rustls direct loading and brokered loading boundary.

use std::{
    fmt,
    sync::{Arc, Mutex},
    time::Instant,
};

use bytes::{Bytes, BytesMut};
use http::{
    Method, StatusCode,
    header::{CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION, USER_AGENT},
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
    broker::{RequestBroker, RequestContext},
    cache::{NetworkCacheMetrics, ResponseCache},
    cancellation::CancellationToken,
    cookie::CookieJar,
    error::NetError,
    model::{LoadConfig, NetworkDiagnostic, RedirectHop, Request, Response, ResponseMetadata},
};

/// Cloneable loader that either owns the network stack or delegates to a broker.
#[derive(Clone)]
pub struct Loader {
    backend: LoaderBackend,
    config: LoadConfig,
    diagnostics: Arc<Mutex<Vec<NetworkDiagnostic>>>,
}

#[derive(Clone)]
enum LoaderBackend {
    Direct(Arc<DirectLoader>),
    Brokered(Arc<dyn RequestBroker>),
}

struct DirectLoader {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    config: LoadConfig,
    cookies: Mutex<CookieJar>,
    cache: Mutex<ResponseCache>,
}

impl fmt::Debug for Loader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Loader")
            .field("brokered", &self.is_brokered())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Loader {
    /// Creates a direct loader using secure defaults and Mozilla WebPKI roots.
    #[must_use]
    pub fn new(config: LoadConfig) -> Self {
        let direct = DirectLoader::new(config.clone());
        Self {
            backend: LoaderBackend::Direct(Arc::new(direct)),
            config,
            diagnostics: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a loader whose requests cross a permission-mediated boundary.
    #[must_use]
    pub fn brokered(broker: Arc<dyn RequestBroker>, config: LoadConfig) -> Self {
        Self {
            backend: LoaderBackend::Brokered(broker),
            config,
            diagnostics: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the active loader policy.
    #[must_use]
    pub const fn config(&self) -> &LoadConfig {
        &self.config
    }

    /// Returns whether this instance cannot open HTTP sockets itself.
    #[must_use]
    pub const fn is_brokered(&self) -> bool {
        matches!(self.backend, LoaderBackend::Brokered(_))
    }

    /// Returns direct-loader cache metrics. Brokered cache metrics live in the broker process.
    #[must_use]
    pub fn cache_metrics(&self) -> Option<NetworkCacheMetrics> {
        match &self.backend {
            LoaderBackend::Direct(direct) => Some(direct.cache().metrics()),
            LoaderBackend::Brokered(_) => None,
        }
    }

    /// Returns a stable copy of the bounded request waterfall.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<NetworkDiagnostic> {
        self.diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Clears retained request diagnostics.
    pub fn clear_diagnostics(&self) {
        self.diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// Loads one request without ambient credentials.
    pub async fn load(
        &self,
        request: Request,
        cancellation: &CancellationToken,
    ) -> Result<Response, NetError> {
        self.load_with_context(request, RequestContext::anonymous(), cancellation)
            .await
    }

    /// Loads one request with explicit document and credential context.
    pub async fn load_with_context(
        &self,
        request: Request,
        context: RequestContext,
        cancellation: &CancellationToken,
    ) -> Result<Response, NetError> {
        if cancellation.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        let started = Instant::now();
        let method = request.method.to_string();
        let requested_url = request.url.to_string();
        let backend = if self.is_brokered() {
            "brokered"
        } else {
            "direct"
        }
        .to_owned();
        let result = match &self.backend {
            LoaderBackend::Direct(direct) => direct.load(request, context, cancellation).await,
            LoaderBackend::Brokered(broker) => broker.load(request, context, cancellation).await,
        };
        let mut diagnostics = self
            .diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let sequence = diagnostics
            .last()
            .map_or(1, |entry| entry.sequence.saturating_add(1));
        let diagnostic = match &result {
            Ok(response) => NetworkDiagnostic {
                sequence,
                method,
                requested_url,
                final_url: Some(response.metadata.final_url.to_string()),
                status: Some(response.status.as_u16()),
                transferred_bytes: response.body.len(),
                elapsed_ms: elapsed_millis(started),
                backend,
                error: None,
            },
            Err(error) => NetworkDiagnostic {
                sequence,
                method,
                requested_url,
                final_url: None,
                status: None,
                transferred_bytes: 0,
                elapsed_ms: elapsed_millis(started),
                backend,
                error: Some(error.to_string()),
            },
        };
        diagnostics.push(diagnostic);
        if diagnostics.len() > 512 {
            let excess = diagnostics.len() - 512;
            diagnostics.drain(..excess);
        }
        drop(diagnostics);
        result
    }
}

impl DirectLoader {
    fn new(config: LoadConfig) -> Self {
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
        Self {
            client,
            config,
            cookies: Mutex::new(CookieJar::default()),
            cache: Mutex::new(ResponseCache::default()),
        }
    }

    async fn load(
        &self,
        mut request: Request,
        context: RequestContext,
        cancellation: &CancellationToken,
    ) -> Result<Response, NetError> {
        if !request.url.is_http_family() {
            return Err(NetError::UnsupportedScheme(request.url.scheme().to_owned()));
        }
        if cancellation.is_cancelled() {
            return Err(NetError::Cancelled);
        }

        self.apply_cookie_header(&mut request, &context);
        let cache_request = request.clone();
        if let Some(response) = self.cache().get(&cache_request) {
            return Ok(response);
        }

        let started = Instant::now();
        let requested_url = request.url.clone();
        let mut redirects = Vec::new();

        loop {
            let response = self.send_once(&request, cancellation).await?;
            let status = response.status();

            if is_redirect(status) {
                let Some(location) = response.headers().get(LOCATION) else {
                    let response = self
                        .finish_response(
                            requested_url,
                            request.url,
                            redirects,
                            response,
                            started,
                            cancellation,
                        )
                        .await?;
                    return Ok(self.finish_policy(cache_request, context, response));
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
                self.apply_cookie_header(&mut request, &context);
                continue;
            }

            let response = self
                .finish_response(
                    requested_url,
                    request.url,
                    redirects,
                    response,
                    started,
                    cancellation,
                )
                .await?;
            return Ok(self.finish_policy(cache_request, context, response));
        }
    }

    fn finish_policy(
        &self,
        cache_request: Request,
        context: RequestContext,
        response: Response,
    ) -> Response {
        let final_cross_origin = context
            .document_url
            .as_ref()
            .is_some_and(|document| document.origin() != response.metadata.final_url.origin());
        if context.credentials.allows(final_cross_origin) {
            self.cookies()
                .store_response(&response.metadata.final_url, &response.headers);
        }
        self.cache().store(&cache_request, &response);
        response
    }

    fn apply_cookie_header(&self, request: &mut Request, context: &RequestContext) {
        request.headers.remove(COOKIE);
        let Some(document_url) = context.document_url.as_ref() else {
            return;
        };
        let cross_origin = document_url.origin() != request.url.origin();
        if !context.credentials.allows(cross_origin) {
            return;
        }
        if let Some(cookie) = self.cookies().header_for(&request.url, document_url)
            && let Ok(value) = cookie.parse()
        {
            request.headers.insert(COOKIE, value);
        }
    }

    fn cookies(&self) -> std::sync::MutexGuard<'_, CookieJar> {
        self.cookies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, ResponseCache> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}
