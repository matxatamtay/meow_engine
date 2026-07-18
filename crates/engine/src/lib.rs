//! Top-level frame and navigation orchestration for MeowEngine.

mod box_tree;
mod encoding;
mod error;
mod layout;
mod model;
mod navigator;
mod style;

use meow_display_list::{DisplayList, DisplayListError, Viewport, reference_scene};

pub use box_tree::{BoxId, BoxKind, BoxNode, BoxTree, build_box_tree};
pub use error::NavigationError;
pub use layout::{
    CssPx, EdgeSizes, LayoutBox, LayoutRect, LayoutTree, LayoutViewport, OverflowMetadata,
    collapse_margins, layout_box_tree, layout_normal_flow,
};
pub use meow_css::{PropertyId, parse_selector_list};
pub use meow_net::CancellationToken;
pub use meow_url_policy::BrowserUrl;
pub use model::{
    CharsetSource, DocumentState, DocumentStylesheet, HistoryEntry, StylesheetLoadError,
    StylesheetSource,
};
pub use navigator::Navigator;
pub use style::{
    CascadeOrigin, CascadeStylesheet, ComputedElementStyle, ComputedStyle, ComputedStyleSnapshot,
    DirtyFlag, InvalidationReport, RestyleReport, StyleDiagnostic, StyleEngine, ValueDiagnostic,
    compute_styles,
};

/// Human-readable engine name used by first-party applications.
pub const ENGINE_NAME: &str = "MeowEngine";

/// Engine coordinator that produces resolved, backend-neutral frames.
#[derive(Debug, Default)]
pub struct Engine;

impl Engine {
    /// Creates an engine coordinator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the display list for one viewport.
    pub fn build_display_list(
        &mut self,
        viewport: Viewport,
    ) -> Result<DisplayList, DisplayListError> {
        reference_scene(viewport)
    }
}

/// Returns the workspace package version embedded at compile time.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
