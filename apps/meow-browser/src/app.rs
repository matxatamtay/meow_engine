use std::{error::Error, fmt, num::NonZeroU32, sync::Arc};

use meow_embedder_api::{BrowserEngine, EmbedderError, Viewport};
use meow_renderer::{GpuRenderer, ReferenceRenderer, RenderError, RenderStatus, Renderer};
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
type DisplayContext = Context<OwnedDisplayHandle>;
type WindowSurface = Surface<OwnedDisplayHandle, Arc<Window>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationBackend {
    Cpu,
    Gpu,
}

pub struct BrowserApp {
    cpu_context: Option<DisplayContext>,
    requested_renderer: PresentationBackend,
    engine: BrowserEngine,
    window: Option<Arc<Window>>,
    presenter: Option<Presenter>,
    metrics: Option<WindowMetrics>,
    lifecycle: Lifecycle,
    presented_frames: u64,
    exit_after_first_frame: bool,
}

impl BrowserApp {
    pub fn new(
        cpu_context: Option<DisplayContext>,
        requested_renderer: PresentationBackend,
        exit_after_first_frame: bool,
    ) -> Self {
        Self {
            cpu_context,
            requested_renderer,
            engine: BrowserEngine::new(),
            window: None,
            presenter: None,
            metrics: None,
            lifecycle: Lifecycle::Starting,
            presented_frames: 0,
            exit_after_first_frame,
        }
    }

    fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            self.ensure_presenter(event_loop);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(format!(
                "{} {}",
                meow_embedder_api::ENGINE_NAME,
                meow_embedder_api::engine_version()
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

        let metrics = WindowMetrics::new(window.inner_size(), window.scale_factor());
        self.lifecycle = Lifecycle::Running;
        self.metrics = Some(metrics);
        self.window = Some(Arc::clone(&window));
        self.ensure_presenter(event_loop);
        if self.presenter.is_none() {
            return;
        }

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

    fn ensure_presenter(&mut self, event_loop: &ActiveEventLoop) {
        if self.presenter.is_some() {
            return;
        }

        let Some(window) = self.window.as_ref().cloned() else {
            return;
        };
        let size = window.inner_size();
        let viewport = match Viewport::new(size.width.max(1), size.height.max(1)) {
            Ok(viewport) => viewport,
            Err(error) => {
                tracing::error!(%error, "invalid initial render viewport");
                event_loop.exit();
                return;
            }
        };

        let presenter = match self.requested_renderer {
            PresentationBackend::Cpu => {
                let Some(context) = self.cpu_context.as_ref() else {
                    tracing::error!("CPU renderer selected without a software display context");
                    event_loop.exit();
                    return;
                };
                match Surface::new(context, window) {
                    Ok(surface) => Presenter::Cpu {
                        surface,
                        renderer: ReferenceRenderer::new(),
                    },
                    Err(error) => {
                        tracing::error!(%error, "failed to create software presentation surface");
                        event_loop.exit();
                        return;
                    }
                }
            }
            PresentationBackend::Gpu => match GpuRenderer::new(window, viewport) {
                Ok(renderer) => Presenter::Gpu(Box::new(renderer)),
                Err(error) => {
                    tracing::error!(%error, "failed to create Vello/wgpu renderer");
                    event_loop.exit();
                    return;
                }
            },
        };
        self.presenter = Some(presenter);
        tracing::info!(renderer = ?self.requested_renderer, "presentation backend ready");
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

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), FrameError> {
        let Some(size) = self.window.as_ref().map(|window| window.inner_size()) else {
            return Ok(());
        };
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let frame = self.engine.render_frame(width.get(), height.get())?;
        let Some(presenter) = self.presenter.as_mut() else {
            return Ok(());
        };

        let status = match presenter {
            Presenter::Cpu { surface, renderer } => {
                surface.resize(width, height)?;
                let framebuffer = renderer.render(frame.viewport(), frame.display_list())?;
                let pixels = framebuffer.softbuffer_pixels();
                let mut buffer = surface.buffer_mut()?;
                buffer.copy_from_slice(&pixels);
                buffer.present()?;
                RenderStatus::Presented
            }
            Presenter::Gpu(renderer) => renderer.render(frame.viewport(), frame.display_list())?,
        };
        if status == RenderStatus::Skipped {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return Ok(());
        }

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
        self.presenter = None;
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
                self.presenter = None;
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
                    tracing::error!(%error, renderer = ?self.requested_renderer, "frame presentation failed");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.lifecycle = Lifecycle::Exiting;
        self.presenter = None;
        self.window = None;
        tracing::info!(
            presented_frames = self.presented_frames,
            "event loop exiting"
        );
    }
}

enum Presenter {
    Cpu {
        surface: WindowSurface,
        renderer: ReferenceRenderer,
    },
    Gpu(Box<GpuRenderer>),
}

#[derive(Debug)]
enum FrameError {
    Embedder(EmbedderError),
    Renderer(RenderError),
    SoftBuffer(SoftBufferError),
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Embedder(error) => error.fmt(formatter),
            Self::Renderer(error) => error.fmt(formatter),
            Self::SoftBuffer(error) => error.fmt(formatter),
        }
    }
}

impl Error for FrameError {}

impl From<EmbedderError> for FrameError {
    fn from(error: EmbedderError) -> Self {
        Self::Embedder(error)
    }
}

impl From<RenderError> for FrameError {
    fn from(error: RenderError) -> Self {
        Self::Renderer(error)
    }
}

impl From<SoftBufferError> for FrameError {
    fn from(error: SoftBufferError) -> Self {
        Self::SoftBuffer(error)
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
