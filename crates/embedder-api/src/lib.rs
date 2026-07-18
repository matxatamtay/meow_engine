//! Stable boundary between browser embedders and the internal engine coordinator.

use std::{error::Error, fmt};

pub use meow_display_list::{
    DisplayCommand, DisplayList, DisplayListError, MAX_VIEWPORT_DIMENSION, MIN_REFERENCE_DIMENSION,
    REFERENCE_HEIGHT, REFERENCE_WIDTH, Rectangle, Rgba8, Viewport,
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

impl Frame {
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

pub use meow_engine::{
    BrowserUrl, CancellationToken, CharsetSource, DocumentState, HistoryEntry, NavigationError,
};

/// Browser-facing owner of frame and navigation coordinators.
#[derive(Debug)]
pub struct BrowserEngine {
    engine: meow_engine::Engine,
    navigator: meow_engine::Navigator,
}

impl BrowserEngine {
    /// Creates a browser-engine boundary object.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: meow_engine::Engine::new(),
            navigator: meow_engine::Navigator::default(),
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

    /// Performs and commits a top-level navigation.
    pub async fn navigate(
        &mut self,
        input: &str,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, EmbedderError> {
        self.navigator
            .navigate(input, cancellation)
            .await
            .map_err(EmbedderError::from)
    }

    /// Requests a resolved frame for physical pixel dimensions.
    pub fn render_frame(&mut self, width: u32, height: u32) -> Result<Frame, EmbedderError> {
        tracing::trace!(width, height, "requesting resolved engine frame");
        let viewport = Viewport::new(width, height).map_err(EmbedderError::from)?;
        let display_list = self
            .engine
            .build_display_list(viewport)
            .map_err(EmbedderError::from)?;
        tracing::trace!(
            commands = display_list.commands().len(),
            "resolved engine frame"
        );
        Ok(Frame {
            viewport,
            display_list,
        })
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
    fn embedder_receives_a_resolved_display_list() {
        let frame = BrowserEngine::new()
            .render_frame(320, 200)
            .expect("frame should build");

        assert_eq!(frame.viewport(), Viewport::new(320, 200).unwrap());
        assert_eq!(frame.display_list().commands().len(), 5);
    }
}
