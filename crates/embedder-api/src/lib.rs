//! Stable boundary between browser embedders and the internal engine coordinator.

use std::{error::Error, fmt, path::PathBuf};

pub use meow_display_list::{
    Affine2D, DisplayCommand, DisplayList, DisplayListError, ImageId, MAX_VIEWPORT_DIMENSION,
    MIN_REFERENCE_DIMENSION, REFERENCE_HEIGHT, REFERENCE_WIDTH, RasterImage, Rectangle, Rgba8,
    StackingContextMetadata, Viewport,
};
pub use meow_engine::{
    BoaRuntime, BrowserUrl, CancellationToken, CharsetSource, ConsoleLevel, ConsoleMessage,
    DocumentState, DocumentStylesheet, DocumentViewMetrics, DomMutation, DomMutationKind,
    FormControlState, GlyphCacheMetrics, HistoryEntry, HitTestEntry, HitTestKind, HitTestList,
    ImageCacheMetrics, ImageKind, ImageLoadError, ImageResource, InteractionPoint,
    InteractionResult, JsRuntime, KeyboardCommand, NavigationError, NodeId, ScriptError,
    ScriptErrorKind, ScriptExecution, ScriptExecutionPhase, ScriptLimits, ScriptSource,
    ScriptValue, ScrollNode, ScrollOffset, ScrollTree, StorageManager, StylesheetLoadError,
    StylesheetSource, TimerRunReport, WebPlatform, WebTaskReport,
};

/// Human-readable engine name exposed to embedders.
pub const ENGINE_NAME: &str = meow_engine::ENGINE_NAME;

/// Returns the engine package version through the embedder boundary.
#[must_use]
pub const fn engine_version() -> &'static str {
    meow_engine::version()
}

/// One resolved frame returned to an embedder.
#[derive(Debug, Clone)]
pub struct Frame {
    viewport: Viewport,
    display_list: DisplayList,
}

/// Snapshot of coalesced DOM work waiting for the next document frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutationPipelineReport {
    pub pending_records: usize,
    pub frame_scheduled: bool,
    pub view_rebuilds: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerformanceMetrics {
    pub document_view_builds: u64,
    pub document_view_cache_hits: u64,
    pub pending_mutations: usize,
    pub last_view: Option<DocumentViewMetrics>,
    pub glyph_cache: GlyphCacheMetrics,
    pub image_cache: ImageCacheMetrics,
}

impl Frame {
    /// Reconstructs a frame received through a validated process boundary.
    #[must_use]
    pub const fn from_parts(viewport: Viewport, display_list: DisplayList) -> Self {
        Self {
            viewport,
            display_list,
        }
    }

    /// Returns the frame viewport.
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Returns the backend-neutral paint commands.
    #[must_use]
    pub const fn display_list(&self) -> &DisplayList {
        &self.display_list
    }
}

/// Browser-facing owner of frame, interaction, and navigation coordinators.
#[derive(Debug)]
pub struct BrowserEngine {
    engine: meow_engine::Engine,
    navigator: meow_engine::Navigator,
    web: WebPlatform,
    document_view: Option<meow_engine::DocumentView>,
    interaction: meow_engine::InteractionState,
    pending_mutations: Vec<DomMutation>,
    frame_scheduled: bool,
    view_rebuilds: u64,
    view_cache_hits: u64,
}

impl BrowserEngine {
    /// Creates a browser-engine boundary object.
    #[must_use]
    pub fn new() -> Self {
        Self::with_storage(StorageManager::ephemeral())
    }

    #[must_use]
    pub fn new_with_profile(profile_dir: impl Into<PathBuf>) -> Self {
        Self::with_loader_and_storage(
            meow_engine::Loader::default(),
            StorageManager::persistent(profile_dir),
        )
    }

    #[must_use]
    pub fn new_with_loader(loader: meow_engine::Loader) -> Self {
        Self::with_loader_and_storage(loader, StorageManager::ephemeral())
    }

    #[must_use]
    pub fn new_with_loader_and_profile(
        loader: meow_engine::Loader,
        profile_dir: impl Into<PathBuf>,
    ) -> Self {
        Self::with_loader_and_storage(loader, StorageManager::persistent(profile_dir))
    }

    fn with_storage(storage: StorageManager) -> Self {
        Self::with_loader_and_storage(meow_engine::Loader::default(), storage)
    }

    fn with_loader_and_storage(loader: meow_engine::Loader, storage: StorageManager) -> Self {
        Self {
            engine: meow_engine::Engine::new(),
            navigator: meow_engine::Navigator::new_with_storage(loader.clone(), storage),
            web: WebPlatform::new(loader),
            document_view: None,
            interaction: meow_engine::InteractionState::default(),
            pending_mutations: Vec::new(),
            frame_scheduled: false,
            view_rebuilds: 0,
            view_cache_hits: 0,
        }
    }

    /// Returns the current committed top-level document.
    #[must_use]
    pub fn current_document(&self) -> &DocumentState {
        self.navigator.current()
    }

    /// Returns the current session-history entries.
    #[must_use]
    pub fn history(&self) -> &[HistoryEntry] {
        self.navigator.history()
    }

    /// Returns whether history traversal can move backward.
    #[must_use]
    pub const fn can_go_back(&self) -> bool {
        self.navigator.can_go_back()
    }

    /// Returns whether history traversal can move forward.
    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        self.navigator.can_go_forward()
    }

    /// Performs and commits a top-level navigation.
    pub async fn navigate(
        &mut self,
        input: &str,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, EmbedderError> {
        self.navigator
            .navigate(input, cancellation)
            .await
            .map_err(EmbedderError::from)?;
        self.document_committed();
        Ok(self.navigator.current())
    }

    /// Traverses to the previous history entry and returns whether it moved.
    pub async fn back(&mut self, cancellation: &CancellationToken) -> Result<bool, EmbedderError> {
        let moved = self
            .navigator
            .back(cancellation)
            .await
            .map_err(EmbedderError::from)?
            .is_some();
        if moved {
            self.document_committed();
        }
        Ok(moved)
    }

    /// Traverses to the next history entry and returns whether it moved.
    pub async fn forward(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<bool, EmbedderError> {
        let moved = self
            .navigator
            .forward(cancellation)
            .await
            .map_err(EmbedderError::from)?
            .is_some();
        if moved {
            self.document_committed();
        }
        Ok(moved)
    }

    /// Reloads the current history entry.
    pub async fn reload(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, EmbedderError> {
        self.navigator
            .reload(cancellation)
            .await
            .map_err(EmbedderError::from)?;
        self.document_committed();
        Ok(self.navigator.current())
    }

    /// Requests the deterministic reference scene retained for renderer smoke tests.
    pub fn render_frame(&mut self, width: u32, height: u32) -> Result<Frame, EmbedderError> {
        tracing::trace!(width, height, "requesting reference engine frame");
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        let display_list = self
            .engine
            .build_display_list(viewport)
            .map_err(EmbedderError::from)?;
        Ok(Frame {
            viewport,
            display_list,
        })
    }

    /// Lays out and paints the current committed document with live interaction state.
    pub fn render_document_frame(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<Frame, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        let display_list = self
            .document_view
            .as_ref()
            .expect("document view is initialized")
            .display_list(&self.interaction)
            .map_err(EmbedderError::from)?;
        tracing::trace!(
            width,
            height,
            commands = display_list.commands().len(),
            scroll_x = self.interaction.scroll_offset().x,
            scroll_y = self.interaction.scroll_offset().y,
            "resolved document frame"
        );
        Ok(Frame {
            viewport,
            display_list,
        })
    }

    /// Returns a browser-window title for the current document.
    pub fn document_title(&mut self, width: u32, height: u32) -> Result<String, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        let title = self
            .document_view
            .as_ref()
            .and_then(meow_engine::DocumentView::title)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| self.navigator.current().url.as_str());
        Ok(format!("{title} · {ENGINE_NAME}"))
    }

    /// Returns current scroll metadata, building the viewport cache as needed.
    pub fn scroll_tree(&mut self, width: u32, height: u32) -> Result<&ScrollTree, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        Ok(self
            .document_view
            .as_ref()
            .expect("document view is initialized")
            .scroll_tree())
    }

    /// Returns the current hit-test list, building the viewport cache as needed.
    pub fn hit_tests(&mut self, width: u32, height: u32) -> Result<&HitTestList, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        Ok(self
            .document_view
            .as_ref()
            .expect("document view is initialized")
            .hit_tests())
    }

    /// Scrolls the root viewport and returns whether the offset changed.
    pub fn scroll_by(
        &mut self,
        width: u32,
        height: u32,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<bool, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        let view = self
            .document_view
            .as_ref()
            .expect("document view is initialized");
        Ok(self.interaction.scroll_by(view, delta_x, delta_y))
    }

    /// Dispatches pointer-down in viewport coordinates.
    pub fn pointer_down(
        &mut self,
        width: u32,
        height: u32,
        point: InteractionPoint,
    ) -> Result<InteractionResult, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        let view = self
            .document_view
            .as_ref()
            .expect("document view is initialized");
        Ok(self.interaction.pointer_down(view, point))
    }

    /// Dispatches pointer-up and click default actions in viewport coordinates.
    pub fn pointer_up(
        &mut self,
        width: u32,
        height: u32,
        point: InteractionPoint,
    ) -> Result<InteractionResult, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        let click_target = {
            let view = self
                .document_view
                .as_ref()
                .expect("document view is initialized");
            self.interaction.click_target(view, point)
        };
        let allow_default = if let Some(target) = click_target {
            !self
                .navigator
                .dispatch_event(target, "click", true, true)
                .map_err(NavigationError::Script)
                .map_err(EmbedderError::from)?
                .default_prevented
        } else {
            true
        };
        let result = {
            let view = self
                .document_view
                .as_ref()
                .expect("document view is initialized");
            self.interaction
                .pointer_up_with_default(view, point, allow_default)
        };
        self.finish_interaction(result)
    }

    /// Dispatches one backend-neutral keyboard command.
    pub fn keyboard(
        &mut self,
        width: u32,
        height: u32,
        command: KeyboardCommand,
    ) -> Result<InteractionResult, EmbedderError> {
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        self.ensure_document_view(viewport);
        let click_target = {
            let view = self
                .document_view
                .as_ref()
                .expect("document view is initialized");
            self.interaction.keyboard_click_target(view, &command)
        };
        let allow_default = if let Some(target) = click_target {
            !self
                .navigator
                .dispatch_event(target, "click", true, true)
                .map_err(NavigationError::Script)
                .map_err(EmbedderError::from)?
                .default_prevented
        } else {
            true
        };
        let result = {
            let view = self
                .document_view
                .as_ref()
                .expect("document view is initialized");
            self.interaction
                .keyboard_with_default(view, command, allow_default)
        };
        self.finish_interaction(result)
    }

    /// Processes pending fetches and WebSocket events for the live document realm.
    pub async fn pump_web_tasks(&mut self) -> WebTaskReport {
        let report = self.navigator.pump_web_tasks(&mut self.web).await;
        self.collect_runtime_mutations();
        report
    }

    #[must_use]
    pub fn has_pending_web_tasks(&self) -> bool {
        self.navigator.has_pending_web_tasks() || self.web.has_pending_work()
    }

    /// Advances timers and schedules one frame for any number of resulting DOM mutations.
    pub fn advance_time(&mut self, advance_ms: u64, max_tasks: usize) -> TimerRunReport {
        let report = self.navigator.advance_time(advance_ms, max_tasks);
        self.collect_runtime_mutations();
        report
    }

    #[must_use]
    pub fn has_pending_timers(&self) -> bool {
        self.navigator.has_pending_timers()
    }

    pub fn take_console_messages(&mut self) -> Vec<ConsoleMessage> {
        self.navigator.take_console_messages()
    }

    #[must_use]
    pub fn mutation_pipeline_report(&self) -> MutationPipelineReport {
        MutationPipelineReport {
            pending_records: self.pending_mutations.len(),
            frame_scheduled: self.frame_scheduled,
            view_rebuilds: self.view_rebuilds,
        }
    }

    #[must_use]
    pub fn performance_metrics(&self) -> PerformanceMetrics {
        PerformanceMetrics {
            document_view_builds: self.view_rebuilds,
            document_view_cache_hits: self.view_cache_hits,
            pending_mutations: self.pending_mutations.len(),
            last_view: self
                .document_view
                .as_ref()
                .map(meow_engine::DocumentView::metrics),
            glyph_cache: meow_engine::glyph_cache_metrics(),
            image_cache: self.navigator.current().image_cache_metrics,
        }
    }

    /// Returns the current root scroll offset.
    #[must_use]
    pub const fn scroll_offset(&self) -> ScrollOffset {
        self.interaction.scroll_offset()
    }

    fn finish_interaction(
        &mut self,
        mut result: InteractionResult,
    ) -> Result<InteractionResult, EmbedderError> {
        let control_mutations = self
            .navigator
            .sync_form_controls(&self.interaction.control_states());
        self.schedule_mutations(control_mutations);
        self.collect_runtime_mutations();

        if let Some(form) = result.submitted_form {
            let invalid = self.navigator.invalid_form_controls(form);
            for control in &invalid {
                self.navigator
                    .dispatch_event(*control, "invalid", false, true)
                    .map_err(NavigationError::Script)
                    .map_err(EmbedderError::from)?;
            }
            self.collect_runtime_mutations();
            if !invalid.is_empty() {
                result.navigation = None;
                result.redraw = true;
                return Ok(result);
            }
            let prevented = self
                .navigator
                .dispatch_event(form, "submit", true, true)
                .map_err(NavigationError::Script)
                .map_err(EmbedderError::from)?
                .default_prevented;
            self.collect_runtime_mutations();
            if prevented {
                result.navigation = None;
            }
        }
        Ok(result)
    }

    fn collect_runtime_mutations(&mut self) {
        let mutations = self.navigator.take_runtime_mutations();
        self.schedule_mutations(mutations);
    }

    fn schedule_mutations(&mut self, mutations: Vec<DomMutation>) {
        if mutations.is_empty() {
            return;
        }
        self.pending_mutations.extend(mutations);
        if !self.frame_scheduled {
            self.frame_scheduled = true;
            self.document_view = None;
        }
    }

    fn document_committed(&mut self) {
        self.web.document_committed();
        self.document_view = None;
        self.interaction.reset();
        self.pending_mutations.clear();
        self.frame_scheduled = false;
    }

    fn ensure_document_view(&mut self, viewport: Viewport) {
        let rebuild = self
            .document_view
            .as_ref()
            .is_none_or(|view| view.viewport() != viewport);
        if rebuild {
            self.document_view = Some(meow_engine::DocumentView::new(
                self.navigator.current(),
                viewport,
            ));
            self.view_rebuilds = self.view_rebuilds.saturating_add(1);
            self.pending_mutations.clear();
            self.frame_scheduled = false;
        } else {
            self.view_cache_hits = self.view_cache_hits.saturating_add(1);
        }
        self.interaction.reconcile(
            self.document_view
                .as_ref()
                .expect("document view is initialized"),
        );
    }
}

impl Default for BrowserEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Error exposed across the browser-shell/engine boundary.
#[derive(Debug)]
pub enum EmbedderError {
    /// Display-list construction failed.
    DisplayList(DisplayListError),
    /// Top-level navigation failed before commit.
    Navigation(NavigationError),
}

impl fmt::Display for EmbedderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisplayList(error) => error.fmt(formatter),
            Self::Navigation(error) => error.fmt(formatter),
        }
    }
}

impl Error for EmbedderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DisplayList(error) => Some(error),
            Self::Navigation(error) => Some(error),
        }
    }
}

impl From<DisplayListError> for EmbedderError {
    fn from(error: DisplayListError) -> Self {
        Self::DisplayList(error)
    }
}

impl From<NavigationError> for EmbedderError {
    fn from(error: NavigationError) -> Self {
        Self::Navigation(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedder_receives_a_resolved_reference_display_list() {
        let frame = BrowserEngine::new()
            .render_frame(320, 200)
            .expect("frame should build");

        assert_eq!(frame.viewport(), Viewport::new(320, 200).unwrap());
        assert_eq!(frame.display_list().commands().len(), 5);
    }

    #[test]
    fn about_blank_has_a_document_frame_and_interaction_metadata() {
        let mut engine = BrowserEngine::new();
        let frame = engine
            .render_document_frame(320, 200)
            .expect("document frame should build");

        assert_eq!(frame.viewport(), Viewport::new(320, 200).unwrap());
        assert!(!frame.display_list().is_empty());
        assert_eq!(engine.scroll_offset(), ScrollOffset::default());
        assert_eq!(engine.hit_tests(320, 200).unwrap().entries().len(), 0);
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn repeated_same_viewport_render_reuses_document_view() {
        let mut engine = BrowserEngine::new();
        engine.render_document_frame(320, 200).unwrap();
        let first = engine.performance_metrics();
        engine.render_document_frame(320, 200).unwrap();
        let second = engine.performance_metrics();
        assert_eq!(first.document_view_builds, 1);
        assert_eq!(second.document_view_builds, 1);
        assert!(second.document_view_cache_hits > first.document_view_cache_hits);
        assert!(second.last_view.is_some());
    }
}
