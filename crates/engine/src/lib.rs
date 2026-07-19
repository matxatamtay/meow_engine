//! Top-level frame and navigation orchestration for MeowEngine.

mod box_tree;
mod encoding;
mod error;
mod fonts;
mod fragments;
mod images;
mod interaction;
mod layout;
mod model;
mod navigator;
mod paint;
mod profile;
mod script;
mod storage;
mod style;
mod text;
mod web;

use meow_display_list::{DisplayList, DisplayListError, Viewport, reference_scene};

pub use box_tree::{BoxId, BoxKind, BoxNode, BoxTree, build_box_tree, build_box_tree_with_images};
pub use error::NavigationError;
pub use fonts::{
    FontCoverage, FontDatabase, FontFace, FontId, FontRequest, FontSlant, FontSource, FontSpan,
    Script, script_for,
};
pub use fragments::{
    FragmentId, FragmentLayout, FragmentTree, GlyphCacheMetrics, GlyphFragment, InlinePaintStyle,
    LineFragment, ParagraphFragment, TextDecorations, build_fragment_display_list,
    build_fragment_display_list_with_images, build_fragment_display_list_with_images_and_offset,
    build_fragment_display_list_with_offset, build_fragment_tree, glyph_cache_metrics,
    layout_fragment_tree,
};
pub use images::{
    DEFAULT_IMAGE_CACHE_ENTRIES, ImageCache, ImageCacheMetrics, ImageKind, ImageLoadError,
    ImageResource, MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS,
};
pub use interaction::{
    DocumentView, DocumentViewMetrics, FormControlState, HitTestEntry, HitTestKind, HitTestList,
    InteractionPoint, InteractionResult, InteractionState, KeyboardCommand, ScrollNode,
    ScrollOffset, ScrollTree,
};
pub use layout::{
    CssPx, EdgeSizes, LayoutBox, LayoutRect, LayoutTree, LayoutViewport, OverflowMetadata,
    collapse_margins, layout_box_tree, layout_normal_flow, layout_normal_flow_with_inline_heights,
};
pub use meow_css::{PropertyId, parse_selector_list};
pub use meow_html::{DomMutation, DomMutationKind, NodeId};
pub use meow_net::{CancellationToken, LoadConfig, Loader};
pub use meow_url_policy::BrowserUrl;
pub use model::{
    CharsetSource, DocumentState, DocumentStylesheet, HistoryEntry, StylesheetLoadError,
    StylesheetSource,
};
pub use navigator::Navigator;
pub use paint::build_layout_display_list;
pub use script::{
    BoaRuntime, ConsoleLevel, ConsoleMessage, EventDispatchResult, FetchCompletion,
    FetchResponseInit, FetchTask, JsRuntime, ScriptError, ScriptErrorKind, ScriptExecution,
    ScriptExecutionPhase, ScriptLimits, ScriptSource, ScriptValue, TimerRunReport,
    WebSocketCommand, WebSocketEvent,
};
pub use style::{
    CascadeOrigin, CascadeStylesheet, ComputedElementStyle, ComputedStyle, ComputedStyleSnapshot,
    DirtyFlag, InvalidationReport, RestyleReport, StyleDiagnostic, StyleEngine,
    StyleSharingMetrics, ValueDiagnostic, compute_styles,
};
pub use text::{
    LineBox, LineRun, ParagraphLayout, PositionedGlyph, ShapedGlyph, ShapedRun, ShapedText,
    TextAlign, TextDirection, collapse_whitespace, is_combining_mark, layout_paragraph, shape_text,
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

pub use profile::{
    PROFILE_SCHEMA_VERSION, ProfileError, ProfileManifest, ProfileMigrationReport, prepare_profile,
};
pub use storage::{DEFAULT_STORAGE_QUOTA_BYTES, StorageManager};
pub use web::{WebPlatform, WebTaskReport};
