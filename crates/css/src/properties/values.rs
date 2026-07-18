use super::model::PropertyId;

const SCALE: i64 = 1_000_000;

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
        let scaled = whole.checked_mul(SCALE)?.checked_add(fraction_value)?;
        Some(Self(if negative { -scaled } else { scaled }))
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    #[must_use]
    pub fn clamp_unit(self) -> Self {
        Self(self.0.clamp(0, SCALE))
    }

    #[must_use]
    pub fn to_css(self) -> String {
        let negative = self.0 < 0;
        let absolute = self.0.unsigned_abs();
        let whole = absolute / SCALE as u64;
        let fraction = absolute % SCALE as u64;
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

#[must_use]
pub fn parse_computed_value(property: PropertyId, source: &str) -> Option<ComputedValue> {
    let source = source.trim();
    match property {
        PropertyId::Display => parse_display(source).map(ComputedValue::Display),
        PropertyId::Color | PropertyId::BackgroundColor => {
            parse_color(source).map(ComputedValue::Color)
        }
        PropertyId::Opacity => CssNumber::parse(source)
            .map(CssNumber::clamp_unit)
            .map(ComputedValue::Number),
        PropertyId::Width | PropertyId::Height => {
            parse_length_or_auto(source, false).map(ComputedValue::LengthOrAuto)
        }
        PropertyId::MarginTop
        | PropertyId::MarginRight
        | PropertyId::MarginBottom
        | PropertyId::MarginLeft => {
            parse_length_or_auto(source, true).map(ComputedValue::LengthOrAuto)
        }
        PropertyId::PaddingTop
        | PropertyId::PaddingRight
        | PropertyId::PaddingBottom
        | PropertyId::PaddingLeft => parse_length(source, false).map(ComputedValue::Length),
        PropertyId::BorderTopWidth
        | PropertyId::BorderRightWidth
        | PropertyId::BorderBottomWidth
        | PropertyId::BorderLeftWidth => parse_border_width(source).map(ComputedValue::BorderWidth),
        PropertyId::BoxSizing => parse_box_sizing(source).map(ComputedValue::BoxSizing),
        PropertyId::FontSize => parse_font_size(source),
        PropertyId::LineHeight => parse_line_height(source),
        PropertyId::FontStyle => parse_keyword(source, &["normal", "italic", "oblique"]),
        PropertyId::FontWeight => parse_font_weight(source),
        PropertyId::TextAlign => parse_keyword(
            source,
            &["start", "end", "left", "right", "center", "justify"],
        ),
        PropertyId::Visibility => parse_keyword(source, &["visible", "hidden", "collapse"]),
        PropertyId::FontFamily => {
            (!source.is_empty()).then(|| ComputedValue::Keyword(source.to_owned()))
        }
    }
}

fn parse_display(source: &str) -> Option<DisplayValue> {
    match source.to_ascii_lowercase().as_str() {
        "none" => Some(DisplayValue::None),
        "block" => Some(DisplayValue::Block),
        "inline" => Some(DisplayValue::Inline),
        "inline-block" => Some(DisplayValue::InlineBlock),
        "flex" => Some(DisplayValue::Flex),
        "grid" => Some(DisplayValue::Grid),
        _ => None,
    }
}

fn parse_box_sizing(source: &str) -> Option<BoxSizingValue> {
    match source.to_ascii_lowercase().as_str() {
        "content-box" => Some(BoxSizingValue::ContentBox),
        "border-box" => Some(BoxSizingValue::BorderBox),
        _ => None,
    }
}

fn parse_length_or_auto(source: &str, allow_negative: bool) -> Option<LengthOrAuto> {
    if source.eq_ignore_ascii_case("auto") {
        Some(LengthOrAuto::Auto)
    } else {
        parse_length(source, allow_negative).map(LengthOrAuto::Length)
    }
}

fn parse_length(source: &str, allow_negative: bool) -> Option<Length> {
    let lower = source.trim().to_ascii_lowercase();
    let (number, unit) = if lower == "0" {
        (CssNumber::zero(), LengthUnit::Px)
    } else if let Some(number) = lower.strip_suffix("rem") {
        (CssNumber::parse(number)?, LengthUnit::Rem)
    } else if let Some(number) = lower.strip_suffix("px") {
        (CssNumber::parse(number)?, LengthUnit::Px)
    } else if let Some(number) = lower.strip_suffix("em") {
        (CssNumber::parse(number)?, LengthUnit::Em)
    } else {
        let number = lower.strip_suffix('%')?;
        (CssNumber::parse(number)?, LengthUnit::Percent)
    };
    if !allow_negative && number.is_negative() {
        return None;
    }
    Some(Length { number, unit })
}

fn parse_border_width(source: &str) -> Option<BorderWidthValue> {
    match source.to_ascii_lowercase().as_str() {
        "thin" => Some(BorderWidthValue::Thin),
        "medium" => Some(BorderWidthValue::Medium),
        "thick" => Some(BorderWidthValue::Thick),
        _ => parse_length(source, false).map(BorderWidthValue::Length),
    }
}

fn parse_font_size(source: &str) -> Option<ComputedValue> {
    const KEYWORDS: &[&str] = &[
        "xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "smaller",
        "larger",
    ];
    parse_length(source, false)
        .map(ComputedValue::FontSizeLength)
        .or_else(|| parse_keyword(source, KEYWORDS))
}

fn parse_line_height(source: &str) -> Option<ComputedValue> {
    if source.eq_ignore_ascii_case("normal") {
        return Some(ComputedValue::Keyword("normal".to_owned()));
    }
    if let Some(length) = parse_length(source, false) {
        return Some(ComputedValue::LineHeightLength(length));
    }
    let number = CssNumber::parse(source)?;
    (!number.is_negative()).then_some(ComputedValue::LineHeightNumber(number))
}

fn parse_font_weight(source: &str) -> Option<ComputedValue> {
    if let Some(keyword) = parse_keyword(source, &["normal", "bold", "bolder", "lighter"]) {
        return Some(keyword);
    }
    matches!(
        source,
        "100" | "200" | "300" | "400" | "500" | "600" | "700" | "800" | "900"
    )
    .then(|| ComputedValue::Keyword(source.to_owned()))
}

fn parse_keyword(source: &str, accepted: &[&str]) -> Option<ComputedValue> {
    accepted
        .iter()
        .find(|candidate| source.eq_ignore_ascii_case(candidate))
        .map(|candidate| ComputedValue::Keyword((*candidate).to_owned()))
}

fn parse_color(source: &str) -> Option<ColorValue> {
    let lower = source.to_ascii_lowercase();
    match lower.as_str() {
        "transparent" => return Some(ColorValue::Transparent),
        "currentcolor" => return Some(ColorValue::CurrentColor),
        _ => {}
    }
    if let Some(named) = named_color(&lower) {
        return Some(ColorValue::Named(named));
    }
    parse_hex_color(&lower)
}

fn named_color(source: &str) -> Option<NamedColor> {
    Some(match source {
        "black" => NamedColor::Black,
        "silver" => NamedColor::Silver,
        "gray" => NamedColor::Gray,
        "white" => NamedColor::White,
        "maroon" => NamedColor::Maroon,
        "red" => NamedColor::Red,
        "purple" => NamedColor::Purple,
        "fuchsia" => NamedColor::Fuchsia,
        "green" => NamedColor::Green,
        "lime" => NamedColor::Lime,
        "olive" => NamedColor::Olive,
        "yellow" => NamedColor::Yellow,
        "navy" => NamedColor::Navy,
        "blue" => NamedColor::Blue,
        "teal" => NamedColor::Teal,
        "aqua" => NamedColor::Aqua,
        "orange" => NamedColor::Orange,
        "gold" => NamedColor::Gold,
        "crimson" => NamedColor::Crimson,
        "rebeccapurple" => NamedColor::RebeccaPurple,
        _ => return None,
    })
}

fn parse_hex_color(source: &str) -> Option<ColorValue> {
    let hex = source.strip_prefix('#')?;
    let nibble = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
    let byte = |pair: &[u8]| Some(nibble(*pair.first()?)? * 16 + nibble(*pair.get(1)?)?);
    let (red, green, blue, alpha) = match hex.as_bytes() {
        [r, g, b] => {
            let (r, g, b) = (nibble(*r)?, nibble(*g)?, nibble(*b)?);
            (r * 17, g * 17, b * 17, 255)
        }
        [r, g, b, a] => {
            let (r, g, b, a) = (nibble(*r)?, nibble(*g)?, nibble(*b)?, nibble(*a)?);
            (r * 17, g * 17, b * 17, a * 17)
        }
        bytes @ [_, _, _, _, _, _] => (
            byte(&bytes[0..2])?,
            byte(&bytes[2..4])?,
            byte(&bytes[4..6])?,
            255,
        ),
        bytes @ [_, _, _, _, _, _, _, _] => (
            byte(&bytes[0..2])?,
            byte(&bytes[2..4])?,
            byte(&bytes[4..6])?,
            byte(&bytes[6..8])?,
        ),
        _ => return None,
    };
    Some(ColorValue::Rgba {
        red,
        green,
        blue,
        alpha,
    })
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
        } => {
            format!("#{red:02x}{green:02x}{blue:02x}")
        }
        ColorValue::Rgba {
            red,
            green,
            blue,
            alpha,
        } => {
            format!("#{red:02x}{green:02x}{blue:02x}{alpha:02x}")
        }
    }
}
