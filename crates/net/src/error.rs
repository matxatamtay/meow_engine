//! Network loader error surface.

use std::{error::Error, fmt};

use hyper_util::client::legacy::Error as ClientError;
use meow_url_policy::UrlPolicyError;

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
