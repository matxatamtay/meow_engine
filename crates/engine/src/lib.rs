//! Top-level frame orchestration for MeowEngine.

use meow_display_list::{DisplayList, DisplayListError, Viewport, reference_scene};

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
mod tests {
    use super::*;

    #[test]
    fn engine_outputs_commands_without_selecting_a_renderer() {
        let viewport = Viewport::new(320, 200).expect("viewport should be valid");
        let list = Engine::new()
            .build_display_list(viewport)
            .expect("frame should build");

        assert!(!list.is_empty());
    }
}
