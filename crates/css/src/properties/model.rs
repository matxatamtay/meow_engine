/// Longhand properties supported by the W12 computed-style stage.
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
    Visibility,
    Opacity,
    Width,
    Height,
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

/// Every W12 property in deterministic registry order.
pub const ALL_PROPERTIES: [PropertyId; 26] = [
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

impl PropertyId {
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
            "margin-top" => Some(Self::MarginTop),
            "margin-right" => Some(Self::MarginRight),
            "margin-bottom" => Some(Self::MarginBottom),
            "margin-left" => Some(Self::MarginLeft),
            "padding-top" => Some(Self::PaddingTop),
            "padding-right" => Some(Self::PaddingRight),
            "padding-bottom" => Some(Self::PaddingBottom),
            "padding-left" => Some(Self::PaddingLeft),
            "border-top-width" => Some(Self::BorderTopWidth),
            "border-right-width" => Some(Self::BorderRightWidth),
            "border-bottom-width" => Some(Self::BorderBottomWidth),
            "border-left-width" => Some(Self::BorderLeftWidth),
            "box-sizing" => Some(Self::BoxSizing),
            _ => None,
        }
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
            Self::Visibility => "visibility",
            Self::Opacity => "opacity",
            Self::Width => "width",
            Self::Height => "height",
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
            Self::Visibility => "visible",
            Self::Opacity => "1",
            Self::Width | Self::Height => "auto",
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
