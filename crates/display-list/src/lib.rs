//! Backend-neutral paint commands consumed by MeowEngine renderers.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

/// Default width used by the built-in reference scene.
pub const REFERENCE_WIDTH: u32 = 800;
/// Default height used by the built-in reference scene.
pub const REFERENCE_HEIGHT: u32 = 600;
/// Smallest supported side for the built-in reference scene.
pub const MIN_REFERENCE_DIMENSION: u32 = 64;
/// Upper bound that prevents accidental multi-gigabyte render targets.
pub const MAX_VIEWPORT_DIMENSION: u32 = 16_384;

const BACKGROUND: Rgba8 = Rgba8::rgb(18, 23, 33);
const HEADER: Rgba8 = Rgba8::rgb(57, 189, 159);
const MAIN_SURFACE: Rgba8 = Rgba8::rgb(34, 44, 59);
const SIDEBAR_SURFACE: Rgba8 = Rgba8::rgb(45, 57, 75);
const HIGHLIGHT: Rgba8 = Rgba8::rgb(239, 179, 74);

/// Physical-pixel dimensions for one render target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

impl Viewport {
    /// Validates and creates a viewport.
    pub fn new(width: u32, height: u32) -> Result<Self, DisplayListError> {
        if width == 0
            || height == 0
            || width > MAX_VIEWPORT_DIMENSION
            || height > MAX_VIEWPORT_DIMENSION
        {
            return Err(DisplayListError::InvalidViewport { width, height });
        }
        Ok(Self { width, height })
    }
}

/// An unpremultiplied 8-bit RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Returns the red channel.
    #[must_use]
    pub const fn red(self) -> u8 {
        self.red
    }

    /// Returns the green channel.
    #[must_use]
    pub const fn green(self) -> u8 {
        self.green
    }

    /// Returns the blue channel.
    #[must_use]
    pub const fn blue(self) -> u8 {
        self.blue
    }

    /// Returns the alpha channel.
    #[must_use]
    pub const fn alpha(self) -> u8 {
        self.alpha
    }
}

/// An axis-aligned rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Returns the non-empty intersection of two rectangles.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let y1 = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));
        (x1 > x0 && y1 > y0).then(|| Self::new(x0, y0, x1 - x0, y1 - y0))
    }
}

/// Fixed-point affine transform using one-millionth precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Affine2D {
    pub a: i64,
    pub b: i64,
    pub c: i64,
    pub d: i64,
    pub e: i64,
    pub f: i64,
}

impl Affine2D {
    pub const SCALE: i64 = 1_000_000;
    pub const IDENTITY: Self = Self {
        a: Self::SCALE,
        b: 0,
        c: 0,
        d: Self::SCALE,
        e: 0,
        f: 0,
    };

    #[must_use]
    pub const fn translation(x: i32, y: i32) -> Self {
        Self {
            e: x as i64 * Self::SCALE,
            f: y as i64 * Self::SCALE,
            ..Self::IDENTITY
        }
    }

    #[must_use]
    pub fn scale(x: f64, y: f64) -> Self {
        Self::from_f64(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    #[must_use]
    pub fn rotation_degrees(degrees: f64) -> Self {
        let radians = degrees.to_radians();
        let cosine = radians.cos();
        let sine = radians.sin();
        Self::from_f64(cosine, sine, -sine, cosine, 0.0, 0.0)
    }

    #[must_use]
    pub fn from_f64(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        fn fixed(value: f64) -> i64 {
            if !value.is_finite() {
                return 0;
            }
            (value * Affine2D::SCALE as f64)
                .round()
                .clamp(i64::MIN as f64, i64::MAX as f64) as i64
        }
        Self {
            a: fixed(a),
            b: fixed(b),
            c: fixed(c),
            d: fixed(d),
            e: fixed(e),
            f: fixed(f),
        }
    }

    /// Returns `self * next`, applying `next` first and then `self`.
    #[must_use]
    pub fn multiply(self, next: Self) -> Self {
        let scale = i128::from(Self::SCALE);
        let mul = |left: i64, right: i64| i128::from(left) * i128::from(right);
        let fixed =
            |value: i128| (value / scale).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
        Self {
            a: fixed(mul(self.a, next.a) + mul(self.c, next.b)),
            b: fixed(mul(self.b, next.a) + mul(self.d, next.b)),
            c: fixed(mul(self.a, next.c) + mul(self.c, next.d)),
            d: fixed(mul(self.b, next.c) + mul(self.d, next.d)),
            e: fixed(mul(self.a, next.e) + mul(self.c, next.f)) + self.e,
            f: fixed(mul(self.b, next.e) + mul(self.d, next.f)) + self.f,
        }
    }

    #[must_use]
    pub fn values(self) -> [f64; 6] {
        let scale = Self::SCALE as f64;
        [
            self.a as f64 / scale,
            self.b as f64 / scale,
            self.c as f64 / scale,
            self.d as f64 / scale,
            self.e as f64 / scale,
            self.f as f64 / scale,
        ]
    }

    #[must_use]
    pub fn transform_point(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.values();
        (a * x + c * y + e, b * x + d * y + f)
    }

    #[must_use]
    pub fn transform_bounds(self, rectangle: Rectangle) -> Rectangle {
        let x0 = rectangle.x as f64;
        let y0 = rectangle.y as f64;
        let x1 = rectangle.x.saturating_add(rectangle.width) as f64;
        let y1 = rectangle.y.saturating_add(rectangle.height) as f64;
        let points = [
            self.transform_point(x0, y0),
            self.transform_point(x1, y0),
            self.transform_point(x0, y1),
            self.transform_point(x1, y1),
        ];
        let min_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let min_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let max_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let x = min_x.floor().max(0.0).min(u32::MAX as f64) as u32;
        let y = min_y.floor().max(0.0).min(u32::MAX as f64) as u32;
        let right = max_x.ceil().max(x as f64).min(u32::MAX as f64) as u32;
        let bottom = max_y.ceil().max(y as f64).min(u32::MAX as f64) as u32;
        Rectangle::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }
}

/// Stable index into the raster-image resource table of one display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageId(pub u32);

/// Premultiplied RGBA image resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Metadata retained for compositor inspection and profiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackingContextMetadata {
    pub id: u32,
    pub parent: Option<u32>,
    pub transform: Affine2D,
    pub opacity: u16,
    pub start_command: usize,
    pub end_command: usize,
}

/// One backend-neutral paint operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayCommand {
    /// Replaces the full target with one color.
    Clear(Rgba8),
    /// Fills one axis-aligned rectangle.
    FillRectangle {
        /// Rectangle geometry in physical pixels.
        rectangle: Rectangle,
        /// Fill color.
        color: Rgba8,
    },
    /// Draws one raster image into a destination rectangle.
    DrawImage {
        image: ImageId,
        rectangle: Rectangle,
    },
    /// Begins an isolated stacking-context layer.
    PushLayer {
        id: u32,
        transform: Affine2D,
        opacity: u16,
    },
    /// Composites the latest isolated layer.
    PopLayer,
    /// Intersects subsequent paint with one rectangle.
    PushClip(Rectangle),
    /// Restores the previous clip.
    PopClip,
}

/// Ordered paint commands for one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisplayList {
    commands: Vec<DisplayCommand>,
    images: Vec<RasterImage>,
    stacking_contexts: Vec<StackingContextMetadata>,
    layer_stack: Vec<usize>,
    clip_depth: usize,
}

impl DisplayList {
    /// Creates an empty display list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            images: Vec::new(),
            stacking_contexts: Vec::new(),
            layer_stack: Vec::new(),
            clip_depth: 0,
        }
    }

    /// Appends a full-target clear.
    pub fn clear(&mut self, color: Rgba8) {
        self.commands.push(DisplayCommand::Clear(color));
    }

    /// Appends a validated rectangle fill.
    pub fn fill_rectangle(
        &mut self,
        rectangle: Rectangle,
        color: Rgba8,
    ) -> Result<(), DisplayListError> {
        if rectangle.width == 0 || rectangle.height == 0 {
            return Err(DisplayListError::InvalidRectangle(rectangle));
        }
        self.commands
            .push(DisplayCommand::FillRectangle { rectangle, color });
        Ok(())
    }

    /// Adds one validated premultiplied RGBA resource.
    pub fn add_image(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<ImageId, DisplayListError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(DisplayListError::InvalidImage {
                width,
                height,
                bytes: pixels.len(),
            })?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err(DisplayListError::InvalidImage {
                width,
                height,
                bytes: pixels.len(),
            });
        }
        let id =
            ImageId(u32::try_from(self.images.len()).map_err(|_| DisplayListError::TooManyImages)?);
        self.images.push(RasterImage {
            width,
            height,
            pixels,
        });
        Ok(id)
    }

    pub fn draw_image(
        &mut self,
        image: ImageId,
        rectangle: Rectangle,
    ) -> Result<(), DisplayListError> {
        if rectangle.width == 0 || rectangle.height == 0 {
            return Err(DisplayListError::InvalidRectangle(rectangle));
        }
        if self.images.get(image.0 as usize).is_none() {
            return Err(DisplayListError::UnknownImage(image));
        }
        self.commands
            .push(DisplayCommand::DrawImage { image, rectangle });
        Ok(())
    }

    /// Begins one transformed, isolated opacity group and returns its context ID.
    pub fn push_layer(&mut self, transform: Affine2D, opacity: u16) -> u32 {
        let id = self.stacking_contexts.len() as u32;
        let parent = self
            .layer_stack
            .last()
            .map(|index| self.stacking_contexts[*index].id);
        self.commands.push(DisplayCommand::PushLayer {
            id,
            transform,
            opacity,
        });
        let index = self.stacking_contexts.len();
        self.stacking_contexts.push(StackingContextMetadata {
            id,
            parent,
            transform,
            opacity,
            start_command: self.commands.len() - 1,
            end_command: self.commands.len() - 1,
        });
        self.layer_stack.push(index);
        id
    }

    pub fn pop_layer(&mut self) -> Result<(), DisplayListError> {
        let Some(index) = self.layer_stack.pop() else {
            return Err(DisplayListError::LayerStackUnderflow);
        };
        self.commands.push(DisplayCommand::PopLayer);
        self.stacking_contexts[index].end_command = self.commands.len() - 1;
        Ok(())
    }

    /// Pushes a validated rectangular clip.
    pub fn push_clip(&mut self, rectangle: Rectangle) -> Result<(), DisplayListError> {
        if rectangle.width == 0 || rectangle.height == 0 {
            return Err(DisplayListError::InvalidRectangle(rectangle));
        }
        self.commands.push(DisplayCommand::PushClip(rectangle));
        self.clip_depth += 1;
        Ok(())
    }

    /// Pops the latest clip.
    pub fn pop_clip(&mut self) -> Result<(), DisplayListError> {
        if self.clip_depth == 0 {
            return Err(DisplayListError::ClipStackUnderflow);
        }
        self.clip_depth -= 1;
        self.commands.push(DisplayCommand::PopClip);
        Ok(())
    }

    /// Returns the paint commands in submission order.
    #[must_use]
    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }

    #[must_use]
    pub fn image(&self, id: ImageId) -> Option<&RasterImage> {
        self.images.get(id.0 as usize)
    }

    #[must_use]
    pub fn images(&self) -> &[RasterImage] {
        &self.images
    }

    #[must_use]
    pub fn stacking_contexts(&self) -> &[StackingContextMetadata] {
        &self.stacking_contexts
    }

    /// Returns whether no commands have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Rebuilds a display list from untrusted wire data while restoring invariants.
    pub fn from_wire_parts(
        commands: Vec<DisplayCommand>,
        images: Vec<RasterImage>,
    ) -> Result<Self, DisplayListError> {
        let mut list = Self::new();
        for image in images {
            list.add_image(image.width, image.height, image.pixels)?;
        }
        for command in commands {
            match command {
                DisplayCommand::Clear(color) => list.clear(color),
                DisplayCommand::FillRectangle { rectangle, color } => {
                    list.fill_rectangle(rectangle, color)?;
                }
                DisplayCommand::DrawImage { image, rectangle } => {
                    list.draw_image(image, rectangle)?;
                }
                DisplayCommand::PushLayer {
                    id,
                    transform,
                    opacity,
                } => {
                    let actual = list.push_layer(transform, opacity);
                    if actual != id {
                        return Err(DisplayListError::UnexpectedLayerId {
                            expected: actual,
                            actual: id,
                        });
                    }
                }
                DisplayCommand::PopLayer => list.pop_layer()?,
                DisplayCommand::PushClip(rectangle) => list.push_clip(rectangle)?,
                DisplayCommand::PopClip => list.pop_clip()?,
            }
        }
        if !list.layer_stack.is_empty() {
            return Err(DisplayListError::UnclosedLayers(list.layer_stack.len()));
        }
        if list.clip_depth != 0 {
            return Err(DisplayListError::UnclosedClips(list.clip_depth));
        }
        Ok(list)
    }
}

/// Builds the W3/W4 reference scene as backend-neutral paint commands.
pub fn reference_scene(viewport: Viewport) -> Result<DisplayList, DisplayListError> {
    if viewport.width < MIN_REFERENCE_DIMENSION || viewport.height < MIN_REFERENCE_DIMENSION {
        return Err(DisplayListError::ReferenceSceneTooSmall {
            width: viewport.width,
            height: viewport.height,
        });
    }

    let width = viewport.width;
    let height = viewport.height;
    let short_side = width.min(height);
    let margin = (short_side / 12).max(4);
    let gap = (margin / 3).max(2);
    let header_height = (height / 8).max(8);
    let content_y = margin + header_height + gap;
    let content_height = height - content_y - margin;
    let sidebar_width = (width / 4).max(8);
    let main_width = width - (margin * 2) - gap - sidebar_width;

    let mut list = DisplayList::new();
    list.clear(BACKGROUND);
    list.fill_rectangle(
        Rectangle::new(margin, margin, width - (margin * 2), header_height),
        HEADER,
    )?;
    list.fill_rectangle(
        Rectangle::new(margin, content_y, main_width, content_height),
        MAIN_SURFACE,
    )?;
    list.fill_rectangle(
        Rectangle::new(
            margin + main_width + gap,
            content_y,
            sidebar_width,
            content_height,
        ),
        SIDEBAR_SURFACE,
    )?;

    let inset = gap * 2;
    list.fill_rectangle(
        Rectangle::new(
            margin + inset,
            content_y + inset,
            (main_width - inset * 2) * 2 / 3,
            (content_height - inset * 2).max(1) / 5,
        ),
        HIGHLIGHT,
    )?;

    Ok(list)
}

/// Errors produced while validating paint commands and reference scenes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayListError {
    /// Viewport dimensions are zero or exceed the allocation guard.
    InvalidViewport { width: u32, height: u32 },
    /// The built-in scene cannot fit inside the requested dimensions.
    ReferenceSceneTooSmall { width: u32, height: u32 },
    /// A rectangle has zero width or height.
    InvalidRectangle(Rectangle),
    /// A clip pop had no matching push.
    ClipStackUnderflow,
    /// A layer pop had no matching push.
    LayerStackUnderflow,
    /// Image dimensions and byte length disagree.
    InvalidImage {
        width: u32,
        height: u32,
        bytes: usize,
    },
    /// Image resource IDs exceeded u32.
    TooManyImages,
    /// A draw command referenced an absent resource.
    UnknownImage(ImageId),
    /// A wire layer ID did not match deterministic reconstruction order.
    UnexpectedLayerId { expected: u32, actual: u32 },
    /// Wire data ended with unclosed layers.
    UnclosedLayers(usize),
    /// Wire data ended with unclosed clips.
    UnclosedClips(usize),
}

impl fmt::Display for DisplayListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidViewport { width, height } => write!(
                formatter,
                "invalid viewport dimensions {width}x{height}; each side must be between 1 and {MAX_VIEWPORT_DIMENSION}"
            ),
            Self::ReferenceSceneTooSmall { width, height } => write!(
                formatter,
                "reference scene dimensions {width}x{height} are too small; each side must be at least {MIN_REFERENCE_DIMENSION}"
            ),
            Self::InvalidRectangle(rectangle) => write!(
                formatter,
                "invalid rectangle at ({}, {}) with size {}x{}",
                rectangle.x, rectangle.y, rectangle.width, rectangle.height
            ),
            Self::ClipStackUnderflow => formatter.write_str("display-list clip stack underflow"),
            Self::LayerStackUnderflow => formatter.write_str("display-list layer stack underflow"),
            Self::InvalidImage {
                width,
                height,
                bytes,
            } => write!(
                formatter,
                "invalid {width}x{height} image resource with {bytes} bytes"
            ),
            Self::TooManyImages => {
                formatter.write_str("display-list image resource limit exceeded")
            }
            Self::UnknownImage(image) => {
                write!(formatter, "unknown display-list image {}", image.0)
            }
            Self::UnexpectedLayerId { expected, actual } => write!(
                formatter,
                "unexpected display-list layer ID {actual}; expected {expected}"
            ),
            Self::UnclosedLayers(depth) => {
                write!(formatter, "display-list ended with {depth} unclosed layers")
            }
            Self::UnclosedClips(depth) => {
                write!(formatter, "display-list ended with {depth} unclosed clips")
            }
        }
    }
}

impl Error for DisplayListError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_scene_is_backend_neutral_and_stable() {
        let viewport = Viewport::new(320, 200).expect("viewport should be valid");
        let list = reference_scene(viewport).expect("scene should fit");

        assert_eq!(list.commands().len(), 5);
        assert!(matches!(list.commands()[0], DisplayCommand::Clear(_)));
        assert!(
            list.commands()[1..]
                .iter()
                .all(|command| matches!(command, DisplayCommand::FillRectangle { .. }))
        );
    }

    #[test]
    fn invalid_inputs_are_rejected_before_rendering() {
        assert!(matches!(
            Viewport::new(0, 10),
            Err(DisplayListError::InvalidViewport { .. })
        ));
        assert!(matches!(
            reference_scene(Viewport::new(32, 64).expect("viewport itself is valid")),
            Err(DisplayListError::ReferenceSceneTooSmall { .. })
        ));

        let mut list = DisplayList::new();
        assert!(matches!(
            list.fill_rectangle(Rectangle::new(0, 0, 0, 5), Rgba8::rgb(0, 0, 0)),
            Err(DisplayListError::InvalidRectangle(_))
        ));
        assert_eq!(list.pop_clip(), Err(DisplayListError::ClipStackUnderflow));
    }

    #[test]
    fn clip_commands_are_balanced_and_rectangles_intersect() {
        let mut list = DisplayList::new();
        list.push_clip(Rectangle::new(2, 2, 4, 4)).unwrap();
        list.pop_clip().unwrap();
        assert_eq!(
            Rectangle::new(0, 0, 4, 4).intersection(Rectangle::new(2, 1, 4, 2)),
            Some(Rectangle::new(2, 1, 2, 2))
        );
        assert!(matches!(
            list.commands(),
            [DisplayCommand::PushClip(_), DisplayCommand::PopClip]
        ));
    }

    #[test]
    fn layers_and_images_retain_compositor_metadata() {
        let mut list = DisplayList::new();
        let image = list
            .add_image(1, 1, vec![255, 0, 0, 255])
            .expect("image should be valid");
        let transform = Affine2D::translation(3, 4).multiply(Affine2D::scale(2.0, 2.0));
        let id = list.push_layer(transform, 32_768);
        list.draw_image(image, Rectangle::new(1, 2, 4, 4)).unwrap();
        list.pop_layer().unwrap();
        assert_eq!(id, 0);
        assert_eq!(list.images().len(), 1);
        assert_eq!(list.stacking_contexts().len(), 1);
        assert_eq!(list.stacking_contexts()[0].end_command, 2);
        assert_eq!(list.pop_layer(), Err(DisplayListError::LayerStackUnderflow));
    }
}
