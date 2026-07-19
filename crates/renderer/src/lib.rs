//! CPU and GPU rasterizers for backend-neutral MeowEngine display lists.

use std::{error::Error, fmt};

use meow_display_list::{
    Affine2D, DisplayCommand, DisplayList, RasterImage, Rectangle, Rgba8, Viewport,
};
use tiny_skia::{
    BlendMode, Color as TinyColor, IntSize, Paint, Pixmap, PixmapPaint, Rect as TinyRect, Transform,
};
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
        tracing::trace!(
            width = viewport.width,
            height = viewport.height,
            commands = display_list.commands().len(),
            "rasterizing CPU display list"
        );
        let viewport_clip = Rectangle::new(0, 0, viewport.width, viewport.height);
        let root =
            Pixmap::new(viewport.width, viewport.height).ok_or(RenderError::AllocationFailed {
                width: viewport.width,
                height: viewport.height,
            })?;
        let mut layers = vec![CpuLayer {
            pixmap: root,
            transform: Affine2D::IDENTITY,
            opacity: u16::MAX,
            clips: vec![Some(viewport_clip)],
        }];
        for command in display_list.commands() {
            match *command {
                DisplayCommand::Clear(color) => {
                    layers
                        .last_mut()
                        .expect("root layer is retained")
                        .pixmap
                        .fill(to_tiny_color(color));
                }
                DisplayCommand::FillRectangle { rectangle, color } => {
                    let layer = layers.last_mut().expect("root layer is retained");
                    if let Some(rectangle) =
                        clipped_for_cpu(rectangle, layer.transform, &layer.clips)
                    {
                        fill_tiny_rectangle(&mut layer.pixmap, rectangle, color, layer.transform)?;
                    }
                }
                DisplayCommand::DrawImage { image, rectangle } => {
                    let resource = display_list
                        .image(image)
                        .ok_or(RenderError::MissingImage(image.0))?;
                    let layer = layers.last_mut().expect("root layer is retained");
                    if transformed_visible(rectangle, layer.transform, &layer.clips) {
                        draw_tiny_image(&mut layer.pixmap, resource, rectangle, layer.transform)?;
                    }
                }
                DisplayCommand::PushLayer {
                    transform, opacity, ..
                } => {
                    let parent = layers.last().expect("root layer is retained");
                    let pixmap = Pixmap::new(viewport.width, viewport.height).ok_or(
                        RenderError::AllocationFailed {
                            width: viewport.width,
                            height: viewport.height,
                        },
                    )?;
                    layers.push(CpuLayer {
                        pixmap,
                        transform: parent.transform.multiply(transform),
                        opacity,
                        clips: parent.clips.clone(),
                    });
                }
                DisplayCommand::PopLayer => {
                    if layers.len() <= 1 {
                        continue;
                    }
                    let child = layers.pop().expect("child layer exists");
                    let parent = layers.last_mut().expect("root layer is retained");
                    let paint = PixmapPaint {
                        opacity: f32::from(child.opacity) / f32::from(u16::MAX),
                        blend_mode: BlendMode::SourceOver,
                        ..PixmapPaint::default()
                    };
                    parent.pixmap.draw_pixmap(
                        0,
                        0,
                        child.pixmap.as_ref(),
                        &paint,
                        Transform::identity(),
                        None,
                    );
                }
                DisplayCommand::PushClip(rectangle) => {
                    let layer = layers.last_mut().expect("root layer is retained");
                    let transformed = layer.transform.transform_bounds(rectangle);
                    let clip = layer
                        .clips
                        .last()
                        .copied()
                        .flatten()
                        .and_then(|current| current.intersection(transformed));
                    layer.clips.push(clip);
                }
                DisplayCommand::PopClip => {
                    let layer = layers.last_mut().expect("root layer is retained");
                    if layer.clips.len() > 1 {
                        layer.clips.pop();
                    }
                }
            }
        }
        while layers.len() > 1 {
            let child = layers.pop().expect("child layer exists");
            let parent = layers.last_mut().expect("root layer is retained");
            let paint = PixmapPaint {
                opacity: f32::from(child.opacity) / f32::from(u16::MAX),
                blend_mode: BlendMode::SourceOver,
                ..PixmapPaint::default()
            };
            parent.pixmap.draw_pixmap(
                0,
                0,
                child.pixmap.as_ref(),
                &paint,
                Transform::identity(),
                None,
            );
        }
        Ok(Framebuffer {
            pixmap: layers.pop().expect("root layer is retained").pixmap,
        })
    }
}

struct CpuLayer {
    pixmap: Pixmap,
    transform: Affine2D,
    opacity: u16,
    clips: Vec<Option<Rectangle>>,
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
            tracing::debug!(
                previous_width = self.surface.config.width,
                previous_height = self.surface.config.height,
                width = viewport.width,
                height = viewport.height,
                "resizing GPU surface"
            );
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
        tracing::trace!(
            width = viewport.width,
            height = viewport.height,
            commands = display_list.commands().len(),
            "lowering display list for GPU presentation"
        );
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
                tracing::debug!("GPU surface requires reconfiguration");
                self.context.configure_surface(&self.surface);
                return Ok(RenderStatus::Skipped);
            }
            CurrentSurfaceTexture::Occluded | CurrentSurfaceTexture::Timeout => {
                tracing::trace!("GPU presentation temporarily skipped");
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
    let viewport_clip = Rectangle::new(0, 0, viewport.width, viewport.height);
    let mut states = vec![GpuState {
        transform: Affine2D::IDENTITY,
        opacity: u16::MAX,
        clips: vec![Some(viewport_clip)],
    }];

    for command in display_list.commands() {
        match *command {
            DisplayCommand::Clear(color) if !has_base_color && states.len() == 1 => {
                base_color = to_vello_color(color);
                has_base_color = true;
            }
            DisplayCommand::Clear(color) => {
                let state = states.last().expect("root GPU state is retained");
                fill_vello_rectangle(
                    scene,
                    Rectangle::new(0, 0, viewport.width, viewport.height),
                    color_with_opacity(color, state.opacity),
                    state.transform,
                );
            }
            DisplayCommand::FillRectangle { rectangle, color } => {
                let state = states.last().expect("root GPU state is retained");
                if transformed_visible(rectangle, state.transform, &state.clips) {
                    fill_vello_rectangle(
                        scene,
                        rectangle,
                        color_with_opacity(color, state.opacity),
                        state.transform,
                    );
                }
            }
            DisplayCommand::DrawImage { image, rectangle } => {
                let state = states.last().expect("root GPU state is retained");
                if let Some(resource) = display_list.image(image)
                    && transformed_visible(rectangle, state.transform, &state.clips)
                {
                    draw_vello_image(scene, resource, rectangle, state);
                }
            }
            DisplayCommand::PushLayer {
                transform, opacity, ..
            } => {
                let parent = states.last().expect("root GPU state is retained");
                states.push(GpuState {
                    transform: parent.transform.multiply(transform),
                    opacity: multiply_opacity(parent.opacity, opacity),
                    clips: parent.clips.clone(),
                });
            }
            DisplayCommand::PopLayer => {
                if states.len() > 1 {
                    states.pop();
                }
            }
            DisplayCommand::PushClip(rectangle) => {
                let state = states.last_mut().expect("root GPU state is retained");
                let transformed = state.transform.transform_bounds(rectangle);
                let clip = state
                    .clips
                    .last()
                    .copied()
                    .flatten()
                    .and_then(|current| current.intersection(transformed));
                state.clips.push(clip);
            }
            DisplayCommand::PopClip => {
                let state = states.last_mut().expect("root GPU state is retained");
                if state.clips.len() > 1 {
                    state.clips.pop();
                }
            }
        }
    }
    base_color
}

struct GpuState {
    transform: Affine2D,
    opacity: u16,
    clips: Vec<Option<Rectangle>>,
}

fn fill_vello_rectangle(
    scene: &mut Scene,
    rectangle: Rectangle,
    color: Rgba8,
    transform: Affine2D,
) {
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
        to_vello_affine(transform),
        to_vello_color(color),
        None,
        &rect,
    );
}

fn fill_tiny_rectangle(
    pixmap: &mut Pixmap,
    rectangle: Rectangle,
    color: Rgba8,
    transform: Affine2D,
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
    paint.blend_mode = BlendMode::SourceOver;
    paint.anti_alias = false;
    pixmap.fill_rect(rect, &paint, to_tiny_transform(transform), None);
    Ok(())
}

fn draw_tiny_image(
    target: &mut Pixmap,
    image: &RasterImage,
    rectangle: Rectangle,
    transform: Affine2D,
) -> Result<(), RenderError> {
    let size = IntSize::from_wh(image.width, image.height).ok_or(RenderError::InvalidImage {
        width: image.width,
        height: image.height,
    })?;
    let source = Pixmap::from_vec(image.pixels.clone(), size).ok_or(RenderError::InvalidImage {
        width: image.width,
        height: image.height,
    })?;
    let local = Affine2D::translation(
        rectangle.x.min(i32::MAX as u32) as i32,
        rectangle.y.min(i32::MAX as u32) as i32,
    )
    .multiply(Affine2D::scale(
        rectangle.width as f64 / image.width as f64,
        rectangle.height as f64 / image.height as f64,
    ));
    let paint = PixmapPaint {
        blend_mode: BlendMode::SourceOver,
        ..PixmapPaint::default()
    };
    target.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &paint,
        to_tiny_transform(transform.multiply(local)),
        None,
    );
    Ok(())
}

fn draw_vello_image(
    scene: &mut Scene,
    image: &RasterImage,
    rectangle: Rectangle,
    state: &GpuState,
) {
    let sample_width = image.width.min(64);
    let sample_height = image.height.min(64);
    for sample_y in 0..sample_height {
        for sample_x in 0..sample_width {
            let source_x = sample_x.saturating_mul(image.width) / sample_width;
            let source_y = sample_y.saturating_mul(image.height) / sample_height;
            let offset = ((source_y as usize * image.width as usize) + source_x as usize) * 4;
            let Some(pixel) = image.pixels.get(offset..offset + 4) else {
                continue;
            };
            if pixel[3] == 0 {
                continue;
            }
            let x0 = rectangle.x + sample_x.saturating_mul(rectangle.width) / sample_width;
            let x1 = rectangle.x + (sample_x + 1).saturating_mul(rectangle.width) / sample_width;
            let y0 = rectangle.y + sample_y.saturating_mul(rectangle.height) / sample_height;
            let y1 = rectangle.y + (sample_y + 1).saturating_mul(rectangle.height) / sample_height;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let alpha = pixel[3];
            let unpremultiply = |channel: u8| {
                if alpha == 0 {
                    0
                } else {
                    (u16::from(channel) * 255 / u16::from(alpha)).min(255) as u8
                }
            };
            let color = color_with_opacity(
                Rgba8::new(
                    unpremultiply(pixel[0]),
                    unpremultiply(pixel[1]),
                    unpremultiply(pixel[2]),
                    alpha,
                ),
                state.opacity,
            );
            fill_vello_rectangle(
                scene,
                Rectangle::new(x0, y0, x1 - x0, y1 - y0),
                color,
                state.transform,
            );
        }
    }
}

fn clipped_for_cpu(
    rectangle: Rectangle,
    transform: Affine2D,
    clips: &[Option<Rectangle>],
) -> Option<Rectangle> {
    let clip = clips.last().copied().flatten()?;
    if transform == Affine2D::IDENTITY {
        rectangle.intersection(clip)
    } else {
        transform
            .transform_bounds(rectangle)
            .intersection(clip)
            .map(|_| rectangle)
    }
}

fn transformed_visible(
    rectangle: Rectangle,
    transform: Affine2D,
    clips: &[Option<Rectangle>],
) -> bool {
    clips.last().copied().flatten().is_some_and(|clip| {
        transform
            .transform_bounds(rectangle)
            .intersection(clip)
            .is_some()
    })
}

fn multiply_opacity(left: u16, right: u16) -> u16 {
    ((u32::from(left) * u32::from(right)) / u32::from(u16::MAX)) as u16
}

fn color_with_opacity(color: Rgba8, opacity: u16) -> Rgba8 {
    let alpha = ((u32::from(color.alpha()) * u32::from(opacity)) / u32::from(u16::MAX)) as u8;
    Rgba8::new(color.red(), color.green(), color.blue(), alpha)
}

fn to_tiny_transform(transform: Affine2D) -> Transform {
    let [a, b, c, d, e, f] = transform.values();
    Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
}

fn to_vello_affine(transform: Affine2D) -> Affine {
    Affine::new(transform.values())
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
    /// A display-list image resource was missing.
    MissingImage(u32),
    /// Raster image resource was invalid.
    InvalidImage { width: u32, height: u32 },
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
            Self::MissingImage(image) => write!(formatter, "missing image resource {image}"),
            Self::InvalidImage { width, height } => {
                write!(
                    formatter,
                    "invalid raster image dimensions {width}x{height}"
                )
            }
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
    fn clip_stack_intersects_nested_fills() {
        let viewport = Viewport::new(5, 5).unwrap();
        let mut list = DisplayList::new();
        list.clear(Rgba8::rgb(0, 0, 0));
        list.push_clip(Rectangle::new(1, 1, 2, 2)).unwrap();
        list.fill_rectangle(Rectangle::new(0, 0, 5, 5), Rgba8::rgb(255, 0, 0))
            .unwrap();
        list.pop_clip().unwrap();
        let framebuffer = ReferenceRenderer::new().render(viewport, &list).unwrap();
        assert_eq!(framebuffer.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(framebuffer.pixel(1, 1), Some([255, 0, 0, 255]));
        assert_eq!(framebuffer.pixel(3, 3), Some([0, 0, 0, 255]));
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

    #[test]
    fn opacity_group_composites_once_and_transform_moves_geometry() {
        let viewport = Viewport::new(8, 8).unwrap();
        let mut list = DisplayList::new();
        list.clear(Rgba8::rgb(255, 255, 255));
        list.push_layer(Affine2D::translation(2, 1), 32_768);
        list.fill_rectangle(Rectangle::new(0, 0, 3, 3), Rgba8::rgb(255, 0, 0))
            .unwrap();
        list.fill_rectangle(Rectangle::new(1, 0, 3, 3), Rgba8::rgb(255, 0, 0))
            .unwrap();
        list.pop_layer().unwrap();
        let framebuffer = ReferenceRenderer::new().render(viewport, &list).unwrap();
        assert_eq!(framebuffer.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(framebuffer.pixel(2, 1), Some([255, 127, 127, 255]));
        assert_eq!(framebuffer.pixel(3, 1), Some([255, 127, 127, 255]));
    }

    #[test]
    fn raster_image_scales_through_cpu_display_list() {
        let viewport = Viewport::new(4, 2).unwrap();
        let mut list = DisplayList::new();
        list.clear(Rgba8::rgb(0, 0, 0));
        let image = list
            .add_image(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255])
            .unwrap();
        list.draw_image(image, Rectangle::new(0, 0, 4, 2)).unwrap();
        let framebuffer = ReferenceRenderer::new().render(viewport, &list).unwrap();
        assert_eq!(framebuffer.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(framebuffer.pixel(3, 1), Some([0, 255, 0, 255]));
    }
}
