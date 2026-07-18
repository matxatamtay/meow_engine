/// Longhand properties supported by the computed-style stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyId {
    Display,
    Color,
    BackgroundColor,
    FontFamily,
    FontSize,
    FontStyle,
    FontWeight,
    LineHeight,
    TextAlign,
    TextDecorationLine,
    Visibility,
    Opacity,
    Width,
    Height,
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    BorderTopWidth,
    BorderRightWidth,
    BorderBottomWidth,
    BorderLeftWidth,
    BoxSizing,
}

/// Every supported property in deterministic registry order.
pub const ALL_PROPERTIES: [PropertyId; 31] = [
    PropertyId::Display,
    PropertyId::Color,
    PropertyId::BackgroundColor,
    PropertyId::FontFamily,
    PropertyId::FontSize,
    PropertyId::FontStyle,
    PropertyId::FontWeight,
    PropertyId::LineHeight,
    PropertyId::TextAlign,
    PropertyId::TextDecorationLine,
    PropertyId::Visibility,
    PropertyId::Opacity,
    PropertyId::Width,
    PropertyId::Height,
    PropertyId::MinWidth,
    PropertyId::MaxWidth,
    PropertyId::MinHeight,
    PropertyId::MaxHeight,
    PropertyId::MarginTop,
    PropertyId::MarginRight,
    PropertyId::MarginBottom,
    PropertyId::MarginLeft,
    PropertyId::PaddingTop,
    PropertyId::PaddingRight,
    PropertyId::PaddingBottom,
    PropertyId::PaddingLeft,
    PropertyId::BorderTopWidth,
    PropertyId::BorderRightWidth,
    PropertyId::BorderBottomWidth,
    PropertyId::BorderLeftWidth,
    PropertyId::BoxSizing,
];

/// W11 properties retained by the legacy byte-stable snapshot format.
pub const W11_SNAPSHOT_PROPERTIES: [PropertyId; 13] = [
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

/// W12 properties retained by the typed snapshot introduced in W12.
pub const W12_SNAPSHOT_PROPERTIES: [PropertyId; 26] = [
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
    PropertyId::MarginTop,
    PropertyId::MarginRight,
    PropertyId::MarginBottom,
    PropertyId::MarginLeft,
    PropertyId::PaddingTop,
    PropertyId::PaddingRight,
    PropertyId::PaddingBottom,
    PropertyId::PaddingLeft,
    PropertyId::BorderTopWidth,
    PropertyId::BorderRightWidth,
    PropertyId::BorderBottomWidth,
    PropertyId::BorderLeftWidth,
    PropertyId::BoxSizing,
];

impl PropertyId {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "display" => Self::Display,
            "color" => Self::Color,
            "background-color" => Self::BackgroundColor,
            "font-family" => Self::FontFamily,
            "font-size" => Self::FontSize,
            "font-style" => Self::FontStyle,
            "font-weight" => Self::FontWeight,
            "line-height" => Self::LineHeight,
            "text-align" => Self::TextAlign,
            "text-decoration-line" => Self::TextDecorationLine,
            "visibility" => Self::Visibility,
            "opacity" => Self::Opacity,
            "width" => Self::Width,
            "height" => Self::Height,
            "min-width" => Self::MinWidth,
            "max-width" => Self::MaxWidth,
            "min-height" => Self::MinHeight,
            "max-height" => Self::MaxHeight,
            "margin-top" => Self::MarginTop,
            "margin-right" => Self::MarginRight,
            "margin-bottom" => Self::MarginBottom,
            "margin-left" => Self::MarginLeft,
            "padding-top" => Self::PaddingTop,
            "padding-right" => Self::PaddingRight,
            "padding-bottom" => Self::PaddingBottom,
            "padding-left" => Self::PaddingLeft,
            "border-top-width" => Self::BorderTopWidth,
            "border-right-width" => Self::BorderRightWidth,
            "border-bottom-width" => Self::BorderBottomWidth,
            "border-left-width" => Self::BorderLeftWidth,
            "box-sizing" => Self::BoxSizing,
            _ => return None,
        })
    }

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
            Self::TextDecorationLine => "text-decoration-line",
            Self::Visibility => "visibility",
            Self::Opacity => "opacity",
            Self::Width => "width",
            Self::Height => "height",
            Self::MinWidth => "min-width",
            Self::MaxWidth => "max-width",
            Self::MinHeight => "min-height",
            Self::MaxHeight => "max-height",
            Self::MarginTop => "margin-top",
            Self::MarginRight => "margin-right",
            Self::MarginBottom => "margin-bottom",
            Self::MarginLeft => "margin-left",
            Self::PaddingTop => "padding-top",
            Self::PaddingRight => "padding-right",
            Self::PaddingBottom => "padding-bottom",
            Self::PaddingLeft => "padding-left",
            Self::BorderTopWidth => "border-top-width",
            Self::BorderRightWidth => "border-right-width",
            Self::BorderBottomWidth => "border-bottom-width",
            Self::BorderLeftWidth => "border-left-width",
            Self::BoxSizing => "box-sizing",
        }
    }

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
            Self::TextDecorationLine => "none",
            Self::Visibility => "visible",
            Self::Opacity => "1",
            Self::Width | Self::Height => "auto",
            Self::MinWidth | Self::MinHeight => "0px",
            Self::MaxWidth | Self::MaxHeight => "none",
            Self::MarginTop | Self::MarginRight | Self::MarginBottom | Self::MarginLeft => "0px",
            Self::PaddingTop | Self::PaddingRight | Self::PaddingBottom | Self::PaddingLeft => {
                "0px"
            }
            Self::BorderTopWidth
            | Self::BorderRightWidth
            | Self::BorderBottomWidth
            | Self::BorderLeftWidth => "0px",
            Self::BoxSizing => "content-box",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssWideKeyword {
    Inherit,
    Initial,
    Unset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecifiedValue {
    Value(String),
    CssWide(CssWideKeyword),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDeclaration {
    pub property: PropertyId,
    pub value: SpecifiedValue,
}
