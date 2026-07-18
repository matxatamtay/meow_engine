use std::{num::NonZeroU32, sync::Arc};

use softbuffer::{Context, SoftBufferError, Surface};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, OwnedDisplayHandle},
    window::{Window, WindowId},
};

const INITIAL_SIZE: LogicalSize<f64> = LogicalSize::new(1280.0, 800.0);
const MINIMUM_SIZE: LogicalSize<f64> = LogicalSize::new(640.0, 480.0);
const CLEAR_COLOR: u32 = 0x0014_1822;

type DisplayContext = Context<OwnedDisplayHandle>;
type WindowSurface = Surface<OwnedDisplayHandle, Arc<Window>>;

pub struct BrowserApp {
    context: DisplayContext,
    window: Option<Arc<Window>>,
    surface: Option<WindowSurface>,
    metrics: Option<WindowMetrics>,
    lifecycle: Lifecycle,
    presented_frames: u64,
    exit_after_first_frame: bool,
}

impl BrowserApp {
    pub fn new(context: DisplayContext, exit_after_first_frame: bool) -> Self {
        Self {
            context,
            window: None,
            surface: None,
            metrics: None,
            lifecycle: Lifecycle::Starting,
            presented_frames: 0,
            exit_after_first_frame,
        }
    }

    fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            self.ensure_surface(event_loop);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(format!(
                "{} {}",
                meow_engine::ENGINE_NAME,
                meow_engine::version()
            ))
            .with_inner_size(INITIAL_SIZE)
            .with_min_inner_size(MINIMUM_SIZE)
            .with_resizable(true);

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                tracing::error!(%error, "failed to create browser window");
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&self.context, Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(error) => {
                tracing::error!(%error, "failed to create software presentation surface");
                event_loop.exit();
                return;
            }
        };

        let metrics = WindowMetrics::new(window.inner_size(), window.scale_factor());
        self.lifecycle = Lifecycle::Running;
        self.metrics = Some(metrics);
        self.surface = Some(surface);
        self.window = Some(Arc::clone(&window));

        tracing::info!(
            backend = display_backend(event_loop),
            window_id = ?window.id(),
            physical_width = metrics.physical_size.width,
            physical_height = metrics.physical_size.height,
            logical_width = metrics.logical_size.width,
            logical_height = metrics.logical_size.height,
            scale_factor = metrics.scale_factor,
            "browser window created"
        );

        window.request_redraw();
    }

    fn ensure_surface(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }

        let Some(window) = self.window.as_ref().cloned() else {
            return;
        };

        match Surface::new(&self.context, window) {
            Ok(surface) => {
                self.surface = Some(surface);
                tracing::debug!("presentation surface recreated after resume");
            }
            Err(error) => {
                tracing::error!(%error, "failed to recreate presentation surface");
                event_loop.exit();
            }
        }
    }

    fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        let metrics = self
            .metrics
            .get_or_insert_with(|| WindowMetrics::new(size, 1.0));
        let was_minimized = metrics.is_minimized();
        metrics.set_physical_size(size);

        tracing::debug!(
            physical_width = metrics.physical_size.width,
            physical_height = metrics.physical_size.height,
            logical_width = metrics.logical_size.width,
            logical_height = metrics.logical_size.height,
            scale_factor = metrics.scale_factor,
            minimized = metrics.is_minimized(),
            "window resized"
        );

        match (was_minimized, metrics.is_minimized()) {
            (false, true) => tracing::info!("window minimized or zero-sized"),
            (true, false) => tracing::info!("window restored from zero-sized state"),
            _ => {}
        }
    }

    fn handle_scale_factor_change(&mut self, scale_factor: f64) {
        let physical_size = self
            .window
            .as_ref()
            .map_or(PhysicalSize::new(0, 0), |window| window.inner_size());
        let metrics = self
            .metrics
            .get_or_insert_with(|| WindowMetrics::new(physical_size, scale_factor));
        let previous_scale_factor = metrics.scale_factor;
        metrics.set_scale_factor(scale_factor, physical_size);

        tracing::info!(
            previous_scale_factor,
            scale_factor = metrics.scale_factor,
            physical_width = metrics.physical_size.width,
            physical_height = metrics.physical_size.height,
            logical_width = metrics.logical_size.width,
            logical_height = metrics.logical_size.height,
            "window DPI scale factor changed"
        );
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), SoftBufferError> {
        let Some(size) = self.window.as_ref().map(|window| window.inner_size()) else {
            return Ok(());
        };
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };

        surface.resize(width, height)?;
        let mut buffer = surface.buffer_mut()?;
        buffer.fill(CLEAR_COLOR);
        buffer.present()?;

        self.presented_frames += 1;
        if self.presented_frames == 1 {
            tracing::info!(
                physical_width = size.width,
                physical_height = size.height,
                "first frame presented"
            );

            if self.exit_after_first_frame {
                tracing::info!("smoke test frame completed; exiting event loop");
                event_loop.exit();
            }
        }

        Ok(())
    }
}

impl ApplicationHandler for BrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracing::debug!(previous_state = ?self.lifecycle, "application resumed");
        self.lifecycle = Lifecycle::Running;
        self.ensure_window(event_loop);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        tracing::info!(previous_state = ?self.lifecycle, "application suspended");
        self.lifecycle = Lifecycle::Suspended;
        self.surface = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if !self
            .window
            .as_ref()
            .is_some_and(|window| window.id() == window_id)
        {
            tracing::debug!(?window_id, "ignoring event for unknown window");
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!(?window_id, "close requested");
                self.lifecycle = Lifecycle::Exiting;
                event_loop.exit();
            }
            WindowEvent::Destroyed => {
                tracing::info!(?window_id, "window destroyed");
                self.surface = None;
                self.window = None;
                self.lifecycle = Lifecycle::Exiting;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size);
                if !size_is_zero(size)
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.handle_scale_factor_change(scale_factor);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::Occluded(occluded) => {
                tracing::debug!(occluded, "window occlusion changed");
            }
            WindowEvent::Focused(focused) => {
                tracing::debug!(focused, "window focus changed");
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(event_loop) {
                    tracing::error!(%error, "frame presentation failed");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.lifecycle = Lifecycle::Exiting;
        self.surface = None;
        self.window = None;
        tracing::info!(
            presented_frames = self.presented_frames,
            "event loop exiting"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Starting,
    Running,
    Suspended,
    Exiting,
}

#[derive(Debug, Clone, Copy)]
struct WindowMetrics {
    physical_size: PhysicalSize<u32>,
    logical_size: LogicalSize<f64>,
    scale_factor: f64,
}

impl WindowMetrics {
    fn new(physical_size: PhysicalSize<u32>, scale_factor: f64) -> Self {
        let scale_factor = valid_scale_factor(scale_factor);
        Self {
            physical_size,
            logical_size: physical_size.to_logical(scale_factor),
            scale_factor,
        }
    }

    fn set_physical_size(&mut self, physical_size: PhysicalSize<u32>) {
        self.physical_size = physical_size;
        self.logical_size = physical_size.to_logical(self.scale_factor);
    }

    fn set_scale_factor(&mut self, scale_factor: f64, physical_size: PhysicalSize<u32>) {
        self.scale_factor = valid_scale_factor(scale_factor);
        self.set_physical_size(physical_size);
    }

    const fn is_minimized(self) -> bool {
        size_is_zero(self.physical_size)
    }
}

const fn size_is_zero(size: PhysicalSize<u32>) -> bool {
    size.width == 0 || size.height == 0
}

fn valid_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

#[cfg(target_os = "linux")]
fn display_backend(event_loop: &ActiveEventLoop) -> &'static str {
    use winit::platform::{wayland::ActiveEventLoopExtWayland, x11::ActiveEventLoopExtX11};

    if event_loop.is_wayland() {
        "wayland"
    } else if event_loop.is_x11() {
        "x11"
    } else {
        "unknown"
    }
}

#[cfg(not(target_os = "linux"))]
const fn display_backend(_event_loop: &ActiveEventLoop) -> &'static str {
    "native"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_updates_logical_dimensions() {
        let mut metrics = WindowMetrics::new(PhysicalSize::new(800, 600), 1.0);

        metrics.set_scale_factor(2.0, PhysicalSize::new(1600, 1200));

        assert_eq!(metrics.physical_size, PhysicalSize::new(1600, 1200));
        assert_eq!(metrics.logical_size, LogicalSize::new(800.0, 600.0));
        assert_eq!(metrics.scale_factor, 2.0);
    }

    #[test]
    fn zero_sized_windows_are_treated_as_minimized() {
        let metrics = WindowMetrics::new(PhysicalSize::new(0, 800), 1.25);

        assert!(metrics.is_minimized());
    }

    #[test]
    fn invalid_scale_factor_falls_back_to_one() {
        let metrics = WindowMetrics::new(PhysicalSize::new(640, 480), f64::NAN);

        assert_eq!(metrics.scale_factor, 1.0);
        assert_eq!(metrics.logical_size, LogicalSize::new(640.0, 480.0));
    }
}
