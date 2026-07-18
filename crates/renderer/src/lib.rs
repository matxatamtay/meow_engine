//! CPU and GPU rasterizers for backend-neutral MeowEngine display lists.

use std::{error::Error, fmt};

use meow_display_list::{DisplayCommand, DisplayList, Rectangle, Rgba8, Viewport};
use tiny_skia::{BlendMode, Color as TinyColor, Paint, Pixmap, Rect as TinyRect, Transform};
use vello::{
    AaConfig, Renderer as VelloRenderer, RendererOptions, Scene,
    kurbo::{Affine, Rect as VelloRect},
    peniko::{Color as VelloColor, Fill},
    util::{RenderContext, RenderSurface},
    wgpu::{self, CurrentSurfaceTexture},
};

/// Result of presenting an interactive frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStatus {
    /// The frame reached its target.
    Presented,
    /// Presentation was temporarily skipped and should be retried.
    Skipped,
}

/// Common renderer contract: consume only a resolved display list and viewport.
pub trait Renderer {
    /// Backend-specific frame output.
    type Output;

    /// Rasterizes one display list for the supplied viewport.
    fn render(
        &mut self,
        viewport: Viewport,
        display_list: &DisplayList,
    ) -> Result<Self::Output, RenderError>;
}

/// Owned premultiplied-RGBA framebuffer used by the CPU renderer.
pub struct Framebuffer {
    pixmap: Pixmap,
}

impl Framebuffer {
    fn new(viewport: Viewport) -> Result<Self, RenderError> {
        let pixmap =
            Pixmap::new(viewport.width, viewport.height).ok_or(RenderError::AllocationFailed {
                width: viewport.width,
                height: viewport.height,
            })?;
        Ok(Self { pixmap })
    }

    /// Returns the framebuffer width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// Returns the framebuffer height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Returns the raw premultiplied RGBA bytes in row-major order.
    #[must_use]
    pub fn premultiplied_rgba(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Encodes the framebuffer as a PNG without timestamps or runtime metadata.
    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        self.pixmap
            .encode_png()
            .map_err(|error| RenderError::PngEncoding(error.to_string()))
    }

    /// Converts pixels to the `0x00RRGGBB` words expected by `softbuffer`.
    #[must_use]
    pub fn softbuffer_pixels(&self) -> Vec<u32> {
        self.premultiplied_rgba()
            .chunks_exact(4)
            .map(|pixel| {
                (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2])
            })
            .collect()
    }

    #[cfg(test)]
    fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width() || y >= self.height() {
            return None;
        }
        let offset = ((y as usize * self.width() as usize) + x as usize) * 4;
        self.premultiplied_rgba()
            .get(offset..offset + 4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
    }
}

/// Deterministic `tiny-skia` implementation of [`Renderer`].
#[derive(Debug, Default)]
pub struct ReferenceRenderer;

impl ReferenceRenderer {
    /// Creates the stateless CPU renderer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Renderer for ReferenceRenderer {
    type Output = Framebuffer;

    fn render(
        &mut self,
        viewport: Viewport,
        display_list: &DisplayList,
    ) -> Result<Self::Output, RenderError> {
        let mut framebuffer = Framebuffer::new(viewport)?;
        for command in display_list.commands() {
            match *command {
                DisplayCommand::Clear(color) => framebuffer.pixmap.fill(to_tiny_color(color)),
                DisplayCommand::FillRectangle { rectangle, color } => {
                    fill_tiny_rectangle(&mut framebuffer.pixmap, rectangle, color)?;
                }
            }
        }
        Ok(framebuffer)
    }
}

/// Interactive Vello renderer backed by a wgpu surface.
pub struct GpuRenderer {
    context: RenderContext,
    surface: RenderSurface<'static>,
    renderer: VelloRenderer,
    scene: Scene,
}

impl GpuRenderer {
    /// Creates a GPU renderer and a surface for a `'static` window handle such as `Arc<Window>`.
    pub fn new(
        window: impl Into<wgpu::SurfaceTarget<'static>>,
        viewport: Viewport,
    ) -> Result<Self, RenderError> {
        let mut context = RenderContext::new();
        let surface = pollster::block_on(context.create_surface(
            window,
            viewport.width,
            viewport.height,
            wgpu::PresentMode::AutoVsync,
        ))
        .map_err(|error| RenderError::Gpu(error.to_string()))?;
        let renderer = {
            let device = &context.devices[surface.dev_id].device;
            VelloRenderer::new(device, RendererOptions::default())
                .map_err(|error| RenderError::Gpu(error.to_string()))?
        };
        Ok(Self {
            context,
            surface,
            renderer,
            scene: Scene::new(),
        })
    }

    /// Reconfigures the wgpu surface and Vello intermediate target.
    pub fn resize(&mut self, viewport: Viewport) {
        if self.surface.config.width != viewport.width
            || self.surface.config.height != viewport.height
        {
            self.context
                .resize_surface(&mut self.surface, viewport.width, viewport.height);
        }
    }
}

impl Renderer for GpuRenderer {
    type Output = RenderStatus;

    fn render(
        &mut self,
        viewport: Viewport,
        display_list: &DisplayList,
    ) -> Result<Self::Output, RenderError> {
        self.resize(viewport);
        let base_color = lower_display_list(&mut self.scene, viewport, display_list);

        let device_handle = &self.context.devices[self.surface.dev_id];
        self.renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                &self.scene,
                &self.surface.target_view,
                &vello::RenderParams {
                    base_color,
                    width: viewport.width,
                    height: viewport.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|error| RenderError::Gpu(error.to_string()))?;

        let surface_texture = match self.surface.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Suboptimal(_) => {
                self.context.configure_surface(&self.surface);
                return Ok(RenderStatus::Skipped);
            }
            CurrentSurfaceTexture::Occluded | CurrentSurfaceTexture::Timeout => {
                return Ok(RenderStatus::Skipped);
            }
            CurrentSurfaceTexture::Lost => {
                return Err(RenderError::Gpu("wgpu surface was lost".to_owned()));
            }
            CurrentSurfaceTexture::Validation => {
                return Err(RenderError::Gpu(
                    "wgpu validation error while acquiring the surface".to_owned(),
                ));
            }
        };

        let mut encoder =
            device_handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("MeowEngine surface blit"),
                });
        self.surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &self.surface.target_view,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device_handle.queue.submit([encoder.finish()]);
        surface_texture.present();
        device_handle
            .device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| RenderError::Gpu(error.to_string()))?;
        Ok(RenderStatus::Presented)
    }
}

fn lower_display_list(
    scene: &mut Scene,
    viewport: Viewport,
    display_list: &DisplayList,
) -> VelloColor {
    scene.reset();
    let mut base_color = VelloColor::from_rgb8(0, 0, 0);
    let mut has_base_color = false;

    for command in display_list.commands() {
        match *command {
            DisplayCommand::Clear(color) if !has_base_color => {
                base_color = to_vello_color(color);
                has_base_color = true;
            }
            DisplayCommand::Clear(color) => {
                fill_vello_rectangle(
                    scene,
                    Rectangle::new(0, 0, viewport.width, viewport.height),
                    color,
                );
            }
            DisplayCommand::FillRectangle { rectangle, color } => {
                fill_vello_rectangle(scene, rectangle, color);
            }
        }
    }
    base_color
}

fn fill_vello_rectangle(scene: &mut Scene, rectangle: Rectangle, color: Rgba8) {
    let x0 = f64::from(rectangle.x);
    let y0 = f64::from(rectangle.y);
    let rect = VelloRect::new(
        x0,
        y0,
        x0 + f64::from(rectangle.width),
        y0 + f64::from(rectangle.height),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        to_vello_color(color),
        None,
        &rect,
    );
}

fn fill_tiny_rectangle(
    pixmap: &mut Pixmap,
    rectangle: Rectangle,
    color: Rgba8,
) -> Result<(), RenderError> {
    let rect = TinyRect::from_xywh(
        rectangle.x as f32,
        rectangle.y as f32,
        rectangle.width as f32,
        rectangle.height as f32,
    )
    .ok_or(RenderError::InvalidRectangle(rectangle))?;
    let mut paint = Paint::default();
    paint.set_color(to_tiny_color(color));
    paint.blend_mode = BlendMode::Source;
    paint.anti_alias = false;
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    Ok(())
}

fn to_tiny_color(color: Rgba8) -> TinyColor {
    TinyColor::from_rgba8(color.red(), color.green(), color.blue(), color.alpha())
}

fn to_vello_color(color: Rgba8) -> VelloColor {
    VelloColor::from_rgba8(color.red(), color.green(), color.blue(), color.alpha())
}

/// Errors produced while rasterizing or presenting a display list.
#[derive(Debug)]
pub enum RenderError {
    /// `tiny-skia` rejected the framebuffer allocation.
    AllocationFailed { width: u32, height: u32 },
    /// A rectangle cannot be represented by the CPU rasterizer.
    InvalidRectangle(Rectangle),
    /// PNG encoding failed.
    PngEncoding(String),
    /// GPU setup, rendering, or presentation failed.
    Gpu(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllocationFailed { width, height } => {
                write!(formatter, "failed to allocate {width}x{height} framebuffer")
            }
            Self::InvalidRectangle(rectangle) => write!(
                formatter,
                "invalid rectangle at ({}, {}) with size {}x{}",
                rectangle.x, rectangle.y, rectangle.width, rectangle.height
            ),
            Self::PngEncoding(message) => write!(formatter, "PNG encoding failed: {message}"),
            Self::Gpu(message) => write!(formatter, "GPU renderer failed: {message}"),
        }
    }
}

impl Error for RenderError {}

#[cfg(test)]
mod tests {
    use meow_display_list::{Rgba8, reference_scene};

    use super::*;

    #[test]
    fn cpu_renderer_consumes_display_list_commands() {
        let viewport = Viewport::new(4, 4).expect("viewport should be valid");
        let mut list = DisplayList::new();
        list.clear(Rgba8::rgb(10, 20, 30));
        list.fill_rectangle(Rectangle::new(1, 1, 2, 2), Rgba8::rgb(200, 100, 50))
            .expect("rectangle should be valid");

        let framebuffer = ReferenceRenderer::new()
            .render(viewport, &list)
            .expect("frame should render");
        assert_eq!(framebuffer.pixel(0, 0), Some([10, 20, 30, 255]));
        assert_eq!(framebuffer.pixel(1, 1), Some([200, 100, 50, 255]));
        assert_eq!(framebuffer.pixel(3, 3), Some([10, 20, 30, 255]));
    }

    #[test]
    fn reference_png_is_byte_deterministic_and_round_trips() {
        let viewport = Viewport::new(320, 200).expect("viewport should be valid");
        let list = reference_scene(viewport).expect("scene should build");
        let first = ReferenceRenderer::new()
            .render(viewport, &list)
            .expect("scene should render");
        let second = ReferenceRenderer::new()
            .render(viewport, &list)
            .expect("scene should render");
        let first_png = first.encode_png().expect("scene should encode");
        let second_png = second.encode_png().expect("scene should encode");

        assert_eq!(first_png, second_png);
        let decoded = Pixmap::decode_png(&first_png).expect("PNG should decode");
        assert_eq!(decoded.width(), 320);
        assert_eq!(decoded.height(), 200);
        assert_eq!(decoded.data(), first.premultiplied_rgba());
    }
}
