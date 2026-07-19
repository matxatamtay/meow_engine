//! URL parsing, canonicalization, origin, and reference resolution policy.

use std::{error::Error, fmt, str::FromStr};

use url::{Origin as UrlOrigin, ParseError, Url};

/// A canonical WHATWG URL used at MeowEngine boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserUrl(Url);

impl BrowserUrl {
    /// Parses and canonicalizes an absolute URL.
    pub fn parse(input: &str) -> Result<Self, UrlPolicyError> {
        Url::parse(input).map(Self).map_err(UrlPolicyError)
    }

    /// Returns the initial empty document URL.
    #[must_use]
    pub fn about_blank() -> Self {
        Self(Url::parse("about:blank").expect("about:blank is a valid absolute URL"))
    }

    /// Resolves a URL reference against this URL.
    pub fn resolve(&self, reference: &str) -> Result<Self, UrlPolicyError> {
        self.0.join(reference).map(Self).map_err(UrlPolicyError)
    }

    /// Returns the canonical serialization.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns a clone with its query replaced by encoded name/value pairs.
    #[must_use]
    pub fn with_query_pairs(&self, pairs: &[(String, String)]) -> Self {
        let mut url = self.0.clone();
        {
            let mut query = url.query_pairs_mut();
            query.clear();
            query.extend_pairs(
                pairs
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            );
        }
        Self(url)
    }

    /// Returns the underlying URL for adapter code.
    #[must_use]
    pub const fn as_url(&self) -> &Url {
        &self.0
    }

    /// Returns the scheme without its trailing colon.
    #[must_use]
    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    /// Returns true for URLs the HTTP loader can fetch.
    #[must_use]
    pub fn is_http_family(&self) -> bool {
        matches!(self.scheme(), "http" | "https")
    }

    /// Computes the URL origin used by same-origin checks.
    #[must_use]
    pub fn origin(&self) -> Origin {
        match self.0.origin() {
            UrlOrigin::Tuple(scheme, host, port) => Origin::Tuple {
                scheme,
                host: host.to_string(),
                port,
            },
            UrlOrigin::Opaque(_) => Origin::Opaque,
        }
    }
}

impl fmt::Display for BrowserUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for BrowserUrl {
    type Err = UrlPolicyError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

/// A serialized origin model with explicit tuple components.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A network tuple origin. Default ports are normalized by the URL parser.
    Tuple {
        /// Lowercase URL scheme.
        scheme: String,
        /// Canonical host serialization.
        host: String,
        /// Effective network port.
        port: u16,
    },
    /// A unique opaque origin, used by schemes such as `about` and `data`.
    Opaque,
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tuple { scheme, host, port } => {
                let default_port =
                    matches!((scheme.as_str(), *port), ("http", 80) | ("https", 443));
                if default_port {
                    write!(formatter, "{scheme}://{host}")
                } else {
                    write!(formatter, "{scheme}://{host}:{port}")
                }
            }
            Self::Opaque => formatter.write_str("null"),
        }
    }
}

/// Error returned when an absolute URL or URL reference is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlPolicyError(ParseError);

impl fmt::Display for UrlPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid URL: {}", self.0)
    }
}

impl Error for UrlPolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_urls_and_default_ports() {
        let url = BrowserUrl::parse("HTTPS://Example.COM:443/a/../b?q=hello world#x")
            .expect("URL should parse");

        assert_eq!(url.as_str(), "https://example.com/b?q=hello%20world#x");
        assert_eq!(
            url.origin(),
            Origin::Tuple {
                scheme: "https".into(),
                host: "example.com".into(),
                port: 443,
            }
        );
        assert_eq!(url.origin().to_string(), "https://example.com");
    }

    #[test]
    fn distinguishes_tuple_and_opaque_origins() {
        assert_eq!(BrowserUrl::about_blank().origin(), Origin::Opaque);
        assert_eq!(
            BrowserUrl::parse("http://127.0.0.1:8080/")
                .unwrap()
                .origin()
                .to_string(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn resolves_256_url_reference_cases() {
        let base = BrowserUrl::parse("https://example.test/root/dir/index.html?old=1#old")
            .expect("base URL should parse");

        for case in 0..256_u16 {
            let reference = format!("../asset/{case}.html?q={case}#fragment-{case}");
            let resolved = base.resolve(&reference).expect("reference should resolve");
            let expected =
                format!("https://example.test/root/asset/{case}.html?q={case}#fragment-{case}");
            assert_eq!(resolved.as_str(), expected, "reference case {case}");
        }
    }

    #[test]
    fn resolves_query_fragment_and_authority_references() {
        let base = BrowserUrl::parse("https://example.test/a/b?old=1#old").unwrap();
        let cases = [
            ("?new=2", "https://example.test/a/b?new=2"),
            ("#new", "https://example.test/a/b?old=1#new"),
            ("/root", "https://example.test/root"),
            ("//cdn.example.test/x", "https://cdn.example.test/x"),
            (".", "https://example.test/a/"),
            ("..", "https://example.test/"),
        ];

        for (reference, expected) in cases {
            assert_eq!(base.resolve(reference).unwrap().as_str(), expected);
        }
    }
}
