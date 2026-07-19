//! HTTP/TLS resource loading with redirects, limits, timeouts, and cancellation.

mod broker;
mod cache;
mod cancellation;
mod cookie;
mod error;
mod loader;
mod model;

pub use broker::{CredentialsMode, RequestBroker, RequestContext};
pub use cache::NetworkCacheMetrics;
pub use cancellation::CancellationToken;
pub use error::NetError;
pub use loader::Loader;
pub use model::{
    DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_RESPONSE_BYTES, HttpVersion, LoadConfig, NetworkDiagnostic,
    RedirectHop, Request, Response, ResponseMetadata,
};

#[cfg(test)]
mod tests;
