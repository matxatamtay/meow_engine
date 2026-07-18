//! Navigation error surface.

use std::{error::Error, fmt};

use meow_net::NetError;
use meow_url_policy::UrlPolicyError;

/// Navigation failure before document commit.
#[derive(Debug)]
pub enum NavigationError {
    /// Target URL or relative reference was invalid.
    Url(UrlPolicyError),
    /// Network loading failed.
    Network(NetError),
}

impl fmt::Display for NavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(error) => error.fmt(formatter),
            Self::Network(error) => error.fmt(formatter),
        }
    }
}

impl Error for NavigationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Url(error) => Some(error),
            Self::Network(error) => Some(error),
        }
    }
}
