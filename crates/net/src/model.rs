//! Owned request, response, metadata, and loader policy models.

use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Version, header::ACCEPT};
use meow_url_policy::BrowserUrl;
use serde::{Deserialize, Serialize};

/// Default upper bound for a single response body.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Default redirect hop limit.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// One bounded loader diagnostic used by the built-in network waterfall.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDiagnostic {
    pub sequence: u64,
    pub method: String,
    pub requested_url: String,
    pub final_url: Option<String>,
    pub status: Option<u16>,
    pub transferred_bytes: usize,
    pub elapsed_ms: u64,
    pub backend: String,
    pub error: Option<String>,
}

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

    /// Creates a GET request for an image resource.
    #[must_use]
    pub fn image(url: BrowserUrl) -> Self {
        let mut request = Self::get(url);
        request.headers.insert(
            ACCEPT,
            "image/avif,image/webp,image/png,image/svg+xml,image/*,*/*;q=0.8"
                .parse()
                .unwrap(),
        );
        request
    }

    /// Creates a GET request for an external classic JavaScript resource.
    #[must_use]
    pub fn script(url: BrowserUrl) -> Self {
        let mut request = Self::get(url);
        request.headers.insert(
            ACCEPT,
            "text/javascript,application/javascript,*/*;q=0.1"
                .parse()
                .unwrap(),
        );
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
