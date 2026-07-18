//! Committed document, history, and stylesheet state models.

use std::fmt::Write as _;

use meow_css::Stylesheet;
use meow_html::{Document, NodeId};
use meow_net::ResponseMetadata;
use meow_url_policy::BrowserUrl;

use crate::{CascadeOrigin, CascadeStylesheet, ComputedStyleSnapshot, compute_styles};

/// Source that selected the committed document encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharsetSource {
    /// The synthetic `about:blank` document always uses UTF-8.
    AboutBlank,
    /// A Unicode byte-order mark.
    Bom,
    /// An HTTP `Content-Type` charset parameter.
    HttpHeader,
    /// A `<meta charset>` or equivalent declaration in the first 1024 bytes.
    Meta,
    /// The HTML fallback encoding.
    Default,
}

fn media_is_active(media: Option<&str>) -> bool {
    let Some(media) = media else {
        return true;
    };
    let media = media.trim();
    media.is_empty()
        || media
            .split(',')
            .map(str::trim)
            .any(|query| query.eq_ignore_ascii_case("all") || query.eq_ignore_ascii_case("screen"))
}

/// One committed session-history entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Monotonic entry sequence within this navigator.
    pub sequence: u64,
    /// Committed document URL.
    pub url: BrowserUrl,
}

/// Origin of one parsed document stylesheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StylesheetSource {
    /// CSS text from an HTML `<style>` node.
    Inline { node: NodeId },
    /// CSS bytes loaded from an HTML `<link rel="stylesheet">` node.
    External {
        node: NodeId,
        requested_url: BrowserUrl,
        final_url: BrowserUrl,
    },
}

/// One parsed stylesheet attached to the committed document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentStylesheet {
    /// Inline or external source metadata.
    pub source: StylesheetSource,
    /// Raw media query text, if present.
    pub media: Option<String>,
    /// Parsed CSS syntax tree and recoverable diagnostics.
    pub stylesheet: Stylesheet,
}

/// Non-fatal failure while resolving or loading a linked stylesheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StylesheetLoadError {
    /// `<link>` node that produced the failure.
    pub node: NodeId,
    /// Original href when the failure came from a linked stylesheet.
    pub href: Option<String>,
    /// Human-readable resolution, HTTP, or network error.
    pub message: String,
}

/// Fully parsed and committed top-level document state.
#[derive(Clone, Debug)]
pub struct DocumentState {
    /// URL that produced the document.
    pub url: BrowserUrl,
    /// URL used for relative-reference resolution after applying `<base>`.
    pub base_url: BrowserUrl,
    /// Parsed DOM document.
    pub document: Document,
    /// Canonical Encoding Standard name.
    pub encoding: &'static str,
    /// Why the encoding was selected.
    pub charset_source: CharsetSource,
    /// HTTP response metadata. Synthetic documents have none.
    pub response: Option<ResponseMetadata>,
    /// Parsed inline and successfully loaded external stylesheets in document order.
    pub stylesheets: Vec<DocumentStylesheet>,
    /// Non-fatal linked stylesheet failures.
    pub stylesheet_errors: Vec<StylesheetLoadError>,
    /// Index of this document in the current history list.
    pub history_index: usize,
}

impl DocumentState {
    /// Produces a deterministic dump of every discovered stylesheet and load error.
    #[must_use]
    pub fn dump_stylesheets(&self) -> String {
        let mut output = String::new();
        for (index, entry) in self.stylesheets.iter().enumerate() {
            match &entry.source {
                StylesheetSource::Inline { node } => {
                    writeln!(
                        output,
                        "stylesheet[{index}] inline node={} media={:?}",
                        node.slot, entry.media
                    )
                    .expect("writing to String cannot fail");
                }
                StylesheetSource::External {
                    node,
                    requested_url,
                    final_url,
                } => {
                    writeln!(
                        output,
                        "stylesheet[{index}] external node={} requested={:?} final={:?} media={:?}",
                        node.slot,
                        requested_url.as_str(),
                        final_url.as_str(),
                        entry.media
                    )
                    .expect("writing to String cannot fail");
                }
            }
            output.push_str(&entry.stylesheet.dump());
        }
        for (index, error) in self.stylesheet_errors.iter().enumerate() {
            writeln!(
                output,
                "stylesheet-error[{index}] node={} href={:?} message={:?}",
                error.node.slot, error.href, error.message
            )
            .expect("writing to String cannot fail");
        }
        output
    }

    /// Computes author stylesheets against the committed document.
    #[must_use]
    pub fn computed_styles(&self) -> ComputedStyleSnapshot {
        let stylesheets = self
            .stylesheets
            .iter()
            .filter(|entry| media_is_active(entry.media.as_deref()))
            .map(|entry| CascadeStylesheet::new(CascadeOrigin::Author, &entry.stylesheet))
            .collect::<Vec<_>>();
        compute_styles(&self.document, &stylesheets)
    }

    /// Produces the deterministic W11 computed-style snapshot.
    #[must_use]
    pub fn dump_computed_styles(&self) -> String {
        self.computed_styles().dump()
    }
}
