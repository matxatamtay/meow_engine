//! Backend-neutral paint commands consumed by MeowEngine renderers.

use std::{error::Error, fmt};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// One backend-neutral paint operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// Ordered paint commands for one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisplayList {
    commands: Vec<DisplayCommand>,
}

impl DisplayList {
    /// Creates an empty display list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
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

    /// Returns the paint commands in submission order.
    #[must_use]
    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }

    /// Returns whether no commands have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
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
    }
}
