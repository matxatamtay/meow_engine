//! Committed document, history, and stylesheet state models.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    sync::{Arc, LazyLock},
};

use meow_css::{Stylesheet, parse_stylesheet};
use meow_display_list::{DisplayList, DisplayListError, Viewport};
use meow_html::{Document, DomMutation, NodeId};
use meow_net::ResponseMetadata;
use meow_url_policy::BrowserUrl;

use crate::{
    BoxTree, CascadeOrigin, CascadeStylesheet, ComputedStyleSnapshot, FontDatabase, FragmentLayout,
    ImageCacheMetrics, ImageLoadError, ImageResource, LayoutTree, LayoutViewport, ScriptExecution,
    build_box_tree_with_images, build_fragment_display_list_with_images, build_layout_display_list,
    compute_styles, layout_box_tree, layout_fragment_tree, layout_normal_flow,
};

const HTML_USER_AGENT_CSS: &str = r#"
html, body, address, article, aside, blockquote, div, dl, fieldset, figcaption,
figure, footer, form, h1, h2, h3, h4, h5, h6, header, hr, main, nav, ol, p,
pre, section, ul { display: block; }
head, base, link, meta, title, style, script, template { display: none; }
body { margin: 8px; }
"#;

static HTML_USER_AGENT_STYLESHEET: LazyLock<Stylesheet> =
    LazyLock::new(|| parse_stylesheet(HTML_USER_AGENT_CSS));

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
    /// Classic-script tasks in deterministic execution order.
    pub script_executions: Vec<ScriptExecution>,
    /// DOM mutation records produced by completed classic scripts.
    pub script_mutations: Vec<DomMutation>,
    /// Successfully decoded image resources keyed by their `<img>` node.
    pub images: BTreeMap<NodeId, Arc<ImageResource>>,
    /// Non-fatal image resolution or decode failures.
    pub image_errors: Vec<ImageLoadError>,
    /// Shared image-cache counters after this document finished loading.
    pub image_cache_metrics: ImageCacheMetrics,
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

    /// Produces a deterministic classic-script execution dump.
    #[must_use]
    pub fn dump_scripts(&self) -> String {
        let mut output = String::new();
        for (index, execution) in self.script_executions.iter().enumerate() {
            writeln!(
                output,
                "script[{index}] phase={:?} node={:?} url={:?} status={}",
                execution.phase,
                execution.node.map(|node| node.slot),
                execution.source_url.as_str(),
                if execution.succeeded() { "ok" } else { "error" }
            )
            .expect("writing to String cannot fail");
            if let Some(error) = &execution.error {
                writeln!(
                    output,
                    "script-error[{index}] kind={:?} message={:?}",
                    error.kind, error.message
                )
                .expect("writing to String cannot fail");
            }
        }
        writeln!(output, "script-mutations={}", self.script_mutations.len())
            .expect("writing to String cannot fail");
        output
    }

    /// Computes author stylesheets against the committed document.
    #[must_use]
    pub fn computed_styles(&self) -> ComputedStyleSnapshot {
        let mut stylesheets = vec![CascadeStylesheet::new(
            CascadeOrigin::UserAgent,
            &HTML_USER_AGENT_STYLESHEET,
        )];
        stylesheets.extend(
            self.stylesheets
                .iter()
                .filter(|entry| media_is_active(entry.media.as_deref()))
                .map(|entry| CascadeStylesheet::new(CascadeOrigin::Author, &entry.stylesheet)),
        );
        compute_styles(&self.document, &stylesheets)
    }

    /// Produces the deterministic W11-compatible computed-style snapshot.
    #[must_use]
    pub fn dump_computed_styles(&self) -> String {
        self.computed_styles().dump()
    }

    /// Produces the deterministic W12 typed computed-style snapshot.
    #[must_use]
    pub fn dump_typed_computed_styles(&self) -> String {
        self.computed_styles().dump_typed()
    }

    /// Generates the W13 formatting box tree from the current DOM and styles.
    #[must_use]
    pub fn box_tree(&self) -> BoxTree {
        let styles = self.computed_styles();
        build_box_tree_with_images(&self.document, &styles, &self.images)
    }

    /// Produces a deterministic box-tree dump separate from the DOM dump.
    #[must_use]
    pub fn dump_box_tree(&self) -> String {
        self.box_tree().dump()
    }

    /// Resolves W14 layout geometry for one viewport.
    #[must_use]
    pub fn layout(&self, viewport: LayoutViewport) -> LayoutTree {
        let styles = self.computed_styles();
        let boxes = build_box_tree_with_images(&self.document, &styles, &self.images);
        layout_box_tree(&boxes, &styles, viewport)
    }

    /// Produces a deterministic layout-tree dump.
    #[must_use]
    pub fn dump_layout(&self, viewport: LayoutViewport) -> String {
        self.layout(viewport).dump()
    }

    /// Resolves W15 vertical normal flow for one viewport.
    #[must_use]
    pub fn flow_layout(&self, viewport: LayoutViewport) -> LayoutTree {
        let styles = self.computed_styles();
        let boxes = build_box_tree_with_images(&self.document, &styles, &self.images);
        layout_normal_flow(&boxes, &styles, viewport)
    }

    /// Produces a deterministic vertical-flow layout dump.
    #[must_use]
    pub fn dump_flow_layout(&self, viewport: LayoutViewport) -> String {
        self.flow_layout(viewport).dump()
    }

    /// Resolves W20 two-pass block layout and final inline fragments.
    #[must_use]
    pub fn fragment_layout(&self, viewport: LayoutViewport) -> FragmentLayout {
        let styles = self.computed_styles();
        let boxes = build_box_tree_with_images(&self.document, &styles, &self.images);
        let mut fonts = FontDatabase::deterministic();
        layout_fragment_tree(&boxes, &styles, viewport, &mut fonts)
    }

    /// Produces a deterministic final fragment-tree dump.
    #[must_use]
    pub fn dump_fragments(&self, viewport: LayoutViewport) -> String {
        self.fragment_layout(viewport).fragments.dump()
    }

    /// Builds W20 readable text, background, border, and decoration paint commands.
    pub fn readable_display_list(
        &self,
        viewport: Viewport,
    ) -> Result<DisplayList, DisplayListError> {
        let styles = self.computed_styles();
        let boxes = build_box_tree_with_images(&self.document, &styles, &self.images);
        let mut fonts = FontDatabase::deterministic();
        let output = layout_fragment_tree(
            &boxes,
            &styles,
            LayoutViewport::new(viewport.width, viewport.height),
            &mut fonts,
        );
        build_fragment_display_list_with_images(
            &output.layout,
            &styles,
            &output.fragments,
            viewport,
            &self.images,
        )
    }

    /// Builds W16 background and border paint commands for the committed document.
    pub fn display_list(&self, viewport: Viewport) -> Result<DisplayList, DisplayListError> {
        let styles = self.computed_styles();
        let boxes = build_box_tree_with_images(&self.document, &styles, &self.images);
        let layout = layout_normal_flow(
            &boxes,
            &styles,
            LayoutViewport::new(viewport.width, viewport.height),
        );
        build_layout_display_list(&layout, &styles, viewport)
    }
}
