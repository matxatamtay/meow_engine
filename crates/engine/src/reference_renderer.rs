//! Deterministic software reference rendering built on `tiny-skia`.

use std::{error::Error, fmt};

use tiny_skia::{BlendMode, Color, Paint, Pixmap, Rect, Transform};

/// Default width used by the headless reference render.
pub const REFERENCE_WIDTH: u32 = 800;
/// Default height used by the headless reference render.
pub const REFERENCE_HEIGHT: u32 = 600;
/// Smallest supported side for the built-in reference scene.
pub const MIN_REFERENCE_DIMENSION: u32 = 64;
/// Upper bound that prevents accidental multi-gigabyte framebuffer allocations.
pub const MAX_FRAMEBUFFER_DIMENSION: u32 = 16_384;

const BACKGROUND: Rgba8 = Rgba8::rgb(18, 23, 33);
const HEADER: Rgba8 = Rgba8::rgb(57, 189, 159);
const MAIN_SURFACE: Rgba8 = Rgba8::rgb(34, 44, 59);
const SIDEBAR_SURFACE: Rgba8 = Rgba8::rgb(45, 57, 75);
const HIGHLIGHT: Rgba8 = Rgba8::rgb(239, 179, 74);

/// An unpremultiplied 8-bit RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba8 {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl Rgba8 {
    /// Creates a color from red, green, blue, and alpha channels.
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Creates an opaque RGB color.
    #[must_use]
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::new(red, green, blue, u8::MAX)
    }

    fn to_tiny_skia(self) -> Color {
        Color::from_rgba8(self.red, self.green, self.blue, self.alpha)
    }
}

/// An axis-aligned rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rectangle {
    /// Horizontal origin in pixels.
    pub x: u32,
    /// Vertical origin in pixels.
    pub y: u32,
    /// Rectangle width in pixels.
    pub width: u32,
    /// Rectangle height in pixels.
    pub height: u32,
}

impl Rectangle {
    /// Creates a pixel-aligned rectangle.
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Owned premultiplied-RGBA framebuffer used by the reference renderer.
pub struct Framebuffer {
    pixmap: Pixmap,
}

impl Framebuffer {
    /// Allocates a transparent framebuffer.
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        validate_framebuffer_dimensions(width, height)?;
        let pixmap =
            Pixmap::new(width, height).ok_or(RenderError::AllocationFailed { width, height })?;
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

    /// Replaces every pixel with one background color.
    pub fn clear(&mut self, color: Rgba8) {
        self.pixmap.fill(color.to_tiny_skia());
    }

    /// Draws a filled, pixel-aligned rectangle using source replacement.
    pub fn fill_rectangle(
        &mut self,
        rectangle: Rectangle,
        color: Rgba8,
    ) -> Result<(), RenderError> {
        if rectangle.width == 0 || rectangle.height == 0 {
            return Err(RenderError::InvalidRectangle(rectangle));
        }

        let rect = Rect::from_xywh(
            rectangle.x as f32,
            rectangle.y as f32,
            rectangle.width as f32,
            rectangle.height as f32,
        )
        .ok_or(RenderError::InvalidRectangle(rectangle))?;

        let mut paint = Paint::default();
        paint.set_color(color.to_tiny_skia());
        paint.blend_mode = BlendMode::Source;
        paint.anti_alias = false;
        self.pixmap
            .fill_rect(rect, &paint, Transform::identity(), None);
        Ok(())
    }

    /// Encodes the framebuffer as a PNG without timestamps or runtime metadata.
    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        self.pixmap
            .encode_png()
            .map_err(|error| RenderError::PngEncoding(error.to_string()))
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

/// Renders the built-in W3 scene into an owned framebuffer.
pub fn render_reference_frame(width: u32, height: u32) -> Result<Framebuffer, RenderError> {
    validate_reference_dimensions(width, height)?;

    let mut framebuffer = Framebuffer::new(width, height)?;
    framebuffer.clear(BACKGROUND);

    let short_side = width.min(height);
    let margin = (short_side / 12).max(4);
    let gap = (margin / 3).max(2);
    let header_height = (height / 8).max(8);
    let content_y = margin + header_height + gap;
    let content_height = height - content_y - margin;
    let sidebar_width = (width / 4).max(8);
    let main_width = width - (margin * 2) - gap - sidebar_width;

    framebuffer.fill_rectangle(
        Rectangle::new(margin, margin, width - (margin * 2), header_height),
        HEADER,
    )?;
    framebuffer.fill_rectangle(
        Rectangle::new(margin, content_y, main_width, content_height),
        MAIN_SURFACE,
    )?;
    framebuffer.fill_rectangle(
        Rectangle::new(
            margin + main_width + gap,
            content_y,
            sidebar_width,
            content_height,
        ),
        SIDEBAR_SURFACE,
    )?;

    let inset = gap * 2;
    framebuffer.fill_rectangle(
        Rectangle::new(
            margin + inset,
            content_y + inset,
            (main_width - inset * 2) * 2 / 3,
            (content_height - inset * 2).max(1) / 5,
        ),
        HIGHLIGHT,
    )?;

    Ok(framebuffer)
}

/// Renders and encodes the built-in W3 scene as deterministic PNG bytes.
pub fn render_reference_png(width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    render_reference_frame(width, height)?.encode_png()
}

fn validate_framebuffer_dimensions(width: u32, height: u32) -> Result<(), RenderError> {
    if width == 0
        || height == 0
        || width > MAX_FRAMEBUFFER_DIMENSION
        || height > MAX_FRAMEBUFFER_DIMENSION
    {
        return Err(RenderError::InvalidDimensions { width, height });
    }

    Ok(())
}

fn validate_reference_dimensions(width: u32, height: u32) -> Result<(), RenderError> {
    validate_framebuffer_dimensions(width, height)?;
    if width < MIN_REFERENCE_DIMENSION || height < MIN_REFERENCE_DIMENSION {
        return Err(RenderError::ReferenceSceneTooSmall { width, height });
    }

    Ok(())
}

/// Errors produced while allocating, drawing, or encoding a reference frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The framebuffer dimensions are zero or exceed the allocation guard.
    InvalidDimensions { width: u32, height: u32 },
    /// The built-in scene cannot fit inside the requested dimensions.
    ReferenceSceneTooSmall { width: u32, height: u32 },
    /// `tiny-skia` rejected the framebuffer allocation.
    AllocationFailed { width: u32, height: u32 },
    /// The rectangle has zero width or height or cannot be represented.
    InvalidRectangle(Rectangle),
    /// PNG encoding failed.
    PngEncoding(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => write!(
                formatter,
                "invalid framebuffer dimensions {width}x{height}; each side must be between 1 and {MAX_FRAMEBUFFER_DIMENSION}"
            ),
            Self::ReferenceSceneTooSmall { width, height } => write!(
                formatter,
                "reference scene dimensions {width}x{height} are too small; each side must be at least {MIN_REFERENCE_DIMENSION}"
            ),
            Self::AllocationFailed { width, height } => {
                write!(formatter, "failed to allocate {width}x{height} framebuffer")
            }
            Self::InvalidRectangle(rectangle) => write!(
                formatter,
                "invalid rectangle at ({}, {}) with size {}x{}",
                rectangle.x, rectangle.y, rectangle.width, rectangle.height
            ),
            Self::PngEncoding(message) => write!(formatter, "PNG encoding failed: {message}"),
        }
    }
}

impl Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_and_rectangle_update_expected_pixels() {
        let mut framebuffer = Framebuffer::new(4, 4).expect("framebuffer should allocate");
        framebuffer.clear(Rgba8::rgb(10, 20, 30));
        framebuffer
            .fill_rectangle(Rectangle::new(1, 1, 2, 2), Rgba8::rgb(200, 100, 50))
            .expect("rectangle should draw");

        assert_eq!(framebuffer.pixel(0, 0), Some([10, 20, 30, 255]));
        assert_eq!(framebuffer.pixel(1, 1), Some([200, 100, 50, 255]));
        assert_eq!(framebuffer.pixel(2, 2), Some([200, 100, 50, 255]));
        assert_eq!(framebuffer.pixel(3, 3), Some([10, 20, 30, 255]));
    }

    #[test]
    fn png_encoding_is_byte_deterministic_and_round_trips() {
        let first = render_reference_frame(320, 200).expect("scene should render");
        let second_png = render_reference_png(320, 200).expect("scene should encode");
        let first_png = first.encode_png().expect("scene should encode");

        assert_eq!(first_png, second_png);

        let decoded = Pixmap::decode_png(&first_png).expect("PNG should decode");
        assert_eq!(decoded.width(), 320);
        assert_eq!(decoded.height(), 200);
        assert_eq!(decoded.data(), first.premultiplied_rgba());
    }

    #[test]
    fn invalid_dimensions_and_rectangles_are_rejected() {
        assert!(matches!(
            Framebuffer::new(0, 10),
            Err(RenderError::InvalidDimensions { .. })
        ));
        assert!(matches!(
            render_reference_frame(32, 64),
            Err(RenderError::ReferenceSceneTooSmall { .. })
        ));

        let mut framebuffer = Framebuffer::new(10, 10).expect("framebuffer should allocate");
        assert!(matches!(
            framebuffer.fill_rectangle(Rectangle::new(0, 0, 0, 5), Rgba8::rgb(0, 0, 0)),
            Err(RenderError::InvalidRectangle(_))
        ));
    }
}
