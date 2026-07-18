/// Longhand properties with W11 cascade and inheritance semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyId {
    /// Element principal box generation.
    Display,
    /// Foreground text color.
    Color,
    /// Background fill color.
    BackgroundColor,
    /// Font family list.
    FontFamily,
    /// Font size.
    FontSize,
    /// Font style.
    FontStyle,
    /// Font weight.
    FontWeight,
    /// Line box height.
    LineHeight,
    /// Inline content alignment.
    TextAlign,
    /// Element visibility.
    Visibility,
    /// Element opacity.
    Opacity,
    /// Preferred width.
    Width,
    /// Preferred height.
    Height,
}

/// Every W11 property in deterministic snapshot order.
pub const ALL_PROPERTIES: [PropertyId; 13] = [
    PropertyId::Display,
    PropertyId::Color,
    PropertyId::BackgroundColor,
    PropertyId::FontFamily,
    PropertyId::FontSize,
    PropertyId::FontStyle,
    PropertyId::FontWeight,
    PropertyId::LineHeight,
    PropertyId::TextAlign,
    PropertyId::Visibility,
    PropertyId::Opacity,
    PropertyId::Width,
    PropertyId::Height,
];

impl PropertyId {
    /// Resolves a standard ASCII-insensitive property name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "display" => Some(Self::Display),
            "color" => Some(Self::Color),
            "background-color" => Some(Self::BackgroundColor),
            "font-family" => Some(Self::FontFamily),
            "font-size" => Some(Self::FontSize),
            "font-style" => Some(Self::FontStyle),
            "font-weight" => Some(Self::FontWeight),
            "line-height" => Some(Self::LineHeight),
            "text-align" => Some(Self::TextAlign),
            "visibility" => Some(Self::Visibility),
            "opacity" => Some(Self::Opacity),
            "width" => Some(Self::Width),
            "height" => Some(Self::Height),
            _ => None,
        }
    }

    /// Canonical serialized property name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Color => "color",
            Self::BackgroundColor => "background-color",
            Self::FontFamily => "font-family",
            Self::FontSize => "font-size",
            Self::FontStyle => "font-style",
            Self::FontWeight => "font-weight",
            Self::LineHeight => "line-height",
            Self::TextAlign => "text-align",
            Self::Visibility => "visibility",
            Self::Opacity => "opacity",
            Self::Width => "width",
            Self::Height => "height",
        }
    }

    /// Whether the property's computed value inherits when no declaration wins.
    #[must_use]
    pub const fn inherited(self) -> bool {
        matches!(
            self,
            Self::Color
                | Self::FontFamily
                | Self::FontSize
                | Self::FontStyle
                | Self::FontWeight
                | Self::LineHeight
                | Self::TextAlign
                | Self::Visibility
        )
    }

    /// Engine-defined initial value for the W11 property subset.
    #[must_use]
    pub const fn initial_value(self) -> &'static str {
        match self {
            Self::Display => "inline",
            Self::Color => "black",
            Self::BackgroundColor => "transparent",
            Self::FontFamily => "serif",
            Self::FontSize => "medium",
            Self::FontStyle => "normal",
            Self::FontWeight => "normal",
            Self::LineHeight => "normal",
            Self::TextAlign => "start",
            Self::Visibility => "visible",
            Self::Opacity => "1",
            Self::Width | Self::Height => "auto",
        }
    }
}

/// CSS-wide keyword that affects specified-to-computed value resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssWideKeyword {
    /// Use the parent computed value, or the initial value at the root.
    Inherit,
    /// Use the property's initial value.
    Initial,
    /// Inherit inherited properties and initialize non-inherited properties.
    Unset,
}

/// W11 semantic value retained after declaration filtering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecifiedValue {
    /// Property-specific component value retained as normalized source text.
    Value(String),
    /// CSS-wide keyword.
    CssWide(CssWideKeyword),
}

/// One supported property declaration ready for the cascade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDeclaration {
    /// Supported longhand property.
    pub property: PropertyId,
    /// Property value or CSS-wide keyword.
    pub value: SpecifiedValue,
}
