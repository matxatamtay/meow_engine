//! Top-level navigation, document commit, and stylesheet loading.

use encoding_rs::{Encoding, UTF_8};
use meow_css::parse_stylesheet;
use meow_html::{Document, StylesheetCandidateKind, parse_bytes, parse_utf8};
use meow_net::{CancellationToken, Loader, NetError, Request};
use meow_url_policy::BrowserUrl;

use super::{
    encoding::{charset_parameter, sniff_encoding},
    error::NavigationError,
    model::{
        CharsetSource, DocumentState, DocumentStylesheet, HistoryEntry, StylesheetLoadError,
        StylesheetSource,
    },
};

/// Top-level navigation lifecycle owner.
#[derive(Debug)]
pub struct Navigator {
    loader: Loader,
    current: DocumentState,
    history: Vec<HistoryEntry>,
    next_sequence: u64,
}

impl Navigator {
    /// Creates a navigator with a committed `about:blank` document and history entry.
    #[must_use]
    pub fn new(loader: Loader) -> Self {
        let url = BrowserUrl::about_blank();
        let current = DocumentState {
            url: url.clone(),
            base_url: url.clone(),
            document: parse_utf8(b"").document,
            encoding: UTF_8.name(),
            charset_source: CharsetSource::AboutBlank,
            response: None,
            stylesheets: Vec::new(),
            stylesheet_errors: Vec::new(),
            history_index: 0,
        };
        Self {
            loader,
            current,
            history: vec![HistoryEntry { sequence: 0, url }],
            next_sequence: 1,
        }
    }

    /// Returns the current committed document.
    #[must_use]
    pub const fn current(&self) -> &DocumentState {
        &self.current
    }

    /// Returns committed history entries.
    #[must_use]
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// Resolves, loads, parses, and atomically commits a top-level navigation.
    pub async fn navigate(
        &mut self,
        input: &str,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        let target = BrowserUrl::parse(input)
            .or_else(|_| self.current.base_url.resolve(input))
            .map_err(NavigationError::Url)?;
        self.navigate_to(target, cancellation).await
    }

    /// Loads a canonical target URL and atomically commits it.
    pub async fn navigate_to(
        &mut self,
        target: BrowserUrl,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        tracing::debug!(url = %target, "starting top-level navigation");
        let pending = if target.as_str() == "about:blank" {
            DocumentState {
                url: target.clone(),
                base_url: target.clone(),
                document: parse_utf8(b"").document,
                encoding: UTF_8.name(),
                charset_source: CharsetSource::AboutBlank,
                response: None,
                stylesheets: Vec::new(),
                stylesheet_errors: Vec::new(),
                history_index: self.history.len(),
            }
        } else {
            let response = self
                .loader
                .load(Request::get(target), cancellation)
                .await
                .map_err(NavigationError::Network)?;
            let (encoding, charset_source) =
                sniff_encoding(&response.body, response.metadata.content_type.as_deref());
            let parsed = parse_bytes(&response.body, encoding);
            let final_url = response.metadata.final_url.clone();
            let base_url = parsed
                .document
                .first_base_href()
                .and_then(|reference| final_url.resolve(&reference).ok())
                .unwrap_or_else(|| final_url.clone());
            let (stylesheets, stylesheet_errors) =
                load_stylesheets(&self.loader, &parsed.document, &base_url, cancellation)
                    .await
                    .map_err(NavigationError::Network)?;

            DocumentState {
                url: final_url,
                base_url,
                document: parsed.document,
                encoding: encoding.name(),
                charset_source,
                response: Some(response.metadata),
                stylesheets,
                stylesheet_errors,
                history_index: self.history.len(),
            }
        };

        self.history.push(HistoryEntry {
            sequence: self.next_sequence,
            url: pending.url.clone(),
        });
        self.next_sequence += 1;
        self.current = pending;
        tracing::debug!(url = %self.current.url, history_index = self.current.history_index, "committed top-level navigation");
        Ok(&self.current)
    }
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new(Loader::default())
    }
}

async fn load_stylesheets(
    loader: &Loader,
    document: &Document,
    base_url: &BrowserUrl,
    cancellation: &CancellationToken,
) -> Result<(Vec<DocumentStylesheet>, Vec<StylesheetLoadError>), NetError> {
    let mut stylesheets = Vec::new();
    let mut errors = Vec::new();

    for candidate in document.stylesheet_candidates() {
        match candidate.kind {
            StylesheetCandidateKind::Inline(css) => stylesheets.push(DocumentStylesheet {
                source: StylesheetSource::Inline {
                    node: candidate.node,
                },
                media: candidate.media,
                stylesheet: parse_stylesheet(&css),
            }),
            StylesheetCandidateKind::Linked(href) => {
                let requested_url = match base_url.resolve(&href) {
                    Ok(url) => url,
                    Err(error) => {
                        errors.push(StylesheetLoadError {
                            node: candidate.node,
                            href: Some(href),
                            message: error.to_string(),
                        });
                        continue;
                    }
                };
                match loader
                    .load(Request::stylesheet(requested_url.clone()), cancellation)
                    .await
                {
                    Ok(response) if response.status.is_success() => {
                        let css = decode_stylesheet(
                            &response.body,
                            response.metadata.content_type.as_deref(),
                        );
                        stylesheets.push(DocumentStylesheet {
                            source: StylesheetSource::External {
                                node: candidate.node,
                                requested_url,
                                final_url: response.metadata.final_url,
                            },
                            media: candidate.media,
                            stylesheet: parse_stylesheet(&css),
                        });
                    }
                    Ok(response) => errors.push(StylesheetLoadError {
                        node: candidate.node,
                        href: Some(href),
                        message: format!("stylesheet HTTP status {}", response.status),
                    }),
                    Err(NetError::Cancelled) => return Err(NetError::Cancelled),
                    Err(error) => errors.push(StylesheetLoadError {
                        node: candidate.node,
                        href: Some(href),
                        message: error.to_string(),
                    }),
                }
            }
        }
    }

    Ok((stylesheets, errors))
}

fn decode_stylesheet(bytes: &[u8], content_type: Option<&str>) -> String {
    let encoding = content_type
        .and_then(charset_parameter)
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(UTF_8);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}
