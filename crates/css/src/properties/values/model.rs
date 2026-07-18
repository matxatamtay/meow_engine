pub const CSS_NUMBER_SCALE: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CssNumber(i64);

impl CssNumber {
    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn parse(source: &str) -> Option<Self> {
        let source = source.trim();
        if source.is_empty() {
            return None;
        }
        let (negative, digits) = match source.as_bytes()[0] {
            b'-' => (true, &source[1..]),
            b'+' => (false, &source[1..]),
            _ => (false, source),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > 6
        {
            return None;
        }
        let whole = if whole.is_empty() {
            0
        } else {
            whole.parse::<i64>().ok()?
        };
        let fraction_value = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<i64>().ok()? * 10_i64.pow(u32::try_from(6 - fraction.len()).ok()?)
        };
        let scaled = whole
            .checked_mul(CSS_NUMBER_SCALE)?
            .checked_add(fraction_value)?;
        Some(Self(if negative { -scaled } else { scaled }))
    }

    #[must_use]
    pub const fn scaled(self) -> i64 {
        self.0
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    #[must_use]
    pub fn clamp_unit(self) -> Self {
        Self(self.0.clamp(0, CSS_NUMBER_SCALE))
    }

    #[must_use]
    pub fn to_css(self) -> String {
        let negative = self.0 < 0;
        let absolute = self.0.unsigned_abs();
        let whole = absolute / CSS_NUMBER_SCALE as u64;
        let fraction = absolute % CSS_NUMBER_SCALE as u64;
        let sign = if negative { "-" } else { "" };
        if fraction == 0 {
            return format!("{sign}{whole}");
        }
        let fraction = format!("{fraction:06}").trim_end_matches('0').to_owned();
        if whole == 0 {
            format!("{sign}.{fraction}")
        } else {
            format!("{sign}{whole}.{fraction}")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    Px,
    Em,
    Rem,
    Percent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Length {
    pub number: CssNumber,
    pub unit: LengthUnit,
}

impl Length {
    #[must_use]
    pub fn to_css(self) -> String {
        let suffix = match self.unit {
            LengthUnit::Px => "px",
            LengthUnit::Em => "em",
            LengthUnit::Rem => "rem",
            LengthUnit::Percent => "%",
        };
        format!("{}{suffix}", self.number.to_css())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayValue {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSizingValue {
    ContentBox,
    BorderBox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedColor {
    Black,
    Silver,
    Gray,
    White,
    Maroon,
    Red,
    Purple,
    Fuchsia,
    Green,
    Lime,
    Olive,
    Yellow,
    Navy,
    Blue,
    Teal,
    Aqua,
    Orange,
    Gold,
    Crimson,
    RebeccaPurple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorValue {
    Transparent,
    CurrentColor,
    Named(NamedColor),
    Rgba {
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthOrAuto {
    Auto,
    Length(Length),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthOrNone {
    None,
    Length(Length),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderWidthValue {
    Thin,
    Medium,
    Thick,
    Length(Length),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputedValue {
    Display(DisplayValue),
    Color(ColorValue),
    Length(Length),
    LengthOrAuto(LengthOrAuto),
    LengthOrNone(LengthOrNone),
    BorderWidth(BorderWidthValue),
    Number(CssNumber),
    BoxSizing(BoxSizingValue),
    FontSizeLength(Length),
    LineHeightLength(Length),
    LineHeightNumber(CssNumber),
    Keyword(String),
}

impl ComputedValue {
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Display(_) => "display",
            Self::Color(_) => "color",
            Self::Length(_) => "length",
            Self::LengthOrAuto(_) => "length-or-auto",
            Self::LengthOrNone(_) => "length-or-none",
            Self::BorderWidth(_) => "border-width",
            Self::Number(_) => "number",
            Self::BoxSizing(_) => "box-sizing",
            Self::FontSizeLength(_) => "font-size-length",
            Self::LineHeightLength(_) => "line-height-length",
            Self::LineHeightNumber(_) => "line-height-number",
            Self::Keyword(_) => "keyword",
        }
    }

    #[must_use]
    pub fn to_css(&self) -> String {
        match self {
            Self::Display(value) => match value {
                DisplayValue::None => "none",
                DisplayValue::Block => "block",
                DisplayValue::Inline => "inline",
                DisplayValue::InlineBlock => "inline-block",
                DisplayValue::Flex => "flex",
                DisplayValue::Grid => "grid",
            }
            .to_owned(),
            Self::Color(value) => color_to_css(*value),
            Self::Length(value) | Self::FontSizeLength(value) | Self::LineHeightLength(value) => {
                value.to_css()
            }
            Self::LengthOrAuto(LengthOrAuto::Auto) => "auto".to_owned(),
            Self::LengthOrAuto(LengthOrAuto::Length(value)) => value.to_css(),
            Self::LengthOrNone(LengthOrNone::None) => "none".to_owned(),
            Self::LengthOrNone(LengthOrNone::Length(value)) => value.to_css(),
            Self::BorderWidth(BorderWidthValue::Thin) => "thin".to_owned(),
            Self::BorderWidth(BorderWidthValue::Medium) => "medium".to_owned(),
            Self::BorderWidth(BorderWidthValue::Thick) => "thick".to_owned(),
            Self::BorderWidth(BorderWidthValue::Length(value)) => value.to_css(),
            Self::Number(value) | Self::LineHeightNumber(value) => value.to_css(),
            Self::BoxSizing(BoxSizingValue::ContentBox) => "content-box".to_owned(),
            Self::BoxSizing(BoxSizingValue::BorderBox) => "border-box".to_owned(),
            Self::Keyword(value) => value.clone(),
        }
    }
}

fn color_to_css(value: ColorValue) -> String {
    match value {
        ColorValue::Transparent => "transparent".to_owned(),
        ColorValue::CurrentColor => "currentcolor".to_owned(),
        ColorValue::Named(named) => match named {
            NamedColor::Black => "black",
            NamedColor::Silver => "silver",
            NamedColor::Gray => "gray",
            NamedColor::White => "white",
            NamedColor::Maroon => "maroon",
            NamedColor::Red => "red",
            NamedColor::Purple => "purple",
            NamedColor::Fuchsia => "fuchsia",
            NamedColor::Green => "green",
            NamedColor::Lime => "lime",
            NamedColor::Olive => "olive",
            NamedColor::Yellow => "yellow",
            NamedColor::Navy => "navy",
            NamedColor::Blue => "blue",
            NamedColor::Teal => "teal",
            NamedColor::Aqua => "aqua",
            NamedColor::Orange => "orange",
            NamedColor::Gold => "gold",
            NamedColor::Crimson => "crimson",
            NamedColor::RebeccaPurple => "rebeccapurple",
        }
        .to_owned(),
        ColorValue::Rgba {
            red,
            green,
            blue,
            alpha: 255,
        } => format!("#{red:02x}{green:02x}{blue:02x}"),
        ColorValue::Rgba {
            red,
            green,
            blue,
            alpha,
        } => format!("#{red:02x}{green:02x}{blue:02x}{alpha:02x}"),
    }
}
