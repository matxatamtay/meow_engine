use super::model::{
    BorderWidthValue, BoxSizingValue, ColorValue, ComputedValue, CssNumber, DisplayValue, Length,
    LengthOrAuto, LengthOrNone, LengthUnit, NamedColor, TransformList, TransformOperation,
};
use crate::PropertyId;

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
        PropertyId::MinWidth | PropertyId::MinHeight => {
            parse_length(source, false).map(ComputedValue::Length)
        }
        PropertyId::MaxWidth | PropertyId::MaxHeight => {
            parse_length_or_none(source).map(ComputedValue::LengthOrNone)
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
        PropertyId::FlexDirection => {
            parse_keyword(source, &["row", "row-reverse", "column", "column-reverse"])
        }
        PropertyId::FlexGrow | PropertyId::FlexShrink => CssNumber::parse(source)
            .filter(|value| !value.is_negative())
            .map(ComputedValue::Number),
        PropertyId::FlexBasis => {
            parse_length_or_auto(source, false).map(ComputedValue::LengthOrAuto)
        }
        PropertyId::JustifyContent => parse_keyword(
            source,
            &[
                "flex-start",
                "flex-end",
                "center",
                "space-between",
                "space-around",
                "space-evenly",
            ],
        ),
        PropertyId::AlignItems => {
            parse_keyword(source, &["stretch", "flex-start", "flex-end", "center"])
        }
        PropertyId::Gap => parse_length(source, false).map(ComputedValue::Length),
        PropertyId::Transform => parse_transform(source).map(ComputedValue::Transform),
        PropertyId::FontSize => parse_font_size(source),
        PropertyId::LineHeight => parse_line_height(source),
        PropertyId::FontStyle => parse_keyword(source, &["normal", "italic", "oblique"]),
        PropertyId::FontWeight => parse_font_weight(source),
        PropertyId::TextAlign => parse_keyword(
            source,
            &["start", "end", "left", "right", "center", "justify"],
        ),
        PropertyId::TextDecorationLine => parse_text_decoration_line(source),
        PropertyId::Visibility => parse_keyword(source, &["visible", "hidden", "collapse"]),
        PropertyId::FontFamily => {
            (!source.is_empty()).then(|| ComputedValue::Keyword(source.to_owned()))
        }
    }
}

fn parse_text_decoration_line(source: &str) -> Option<ComputedValue> {
    let mut underline = false;
    let mut line_through = false;
    let mut count = 0;
    for token in source.split_whitespace() {
        count += 1;
        if token.eq_ignore_ascii_case("underline") && !underline {
            underline = true;
        } else if token.eq_ignore_ascii_case("line-through") && !line_through {
            line_through = true;
        } else if token.eq_ignore_ascii_case("none") && count == 1 {
            return source
                .split_whitespace()
                .nth(1)
                .is_none()
                .then(|| ComputedValue::Keyword("none".to_owned()));
        } else {
            return None;
        }
    }
    match (underline, line_through) {
        (true, true) => Some(ComputedValue::Keyword("underline line-through".to_owned())),
        (true, false) => Some(ComputedValue::Keyword("underline".to_owned())),
        (false, true) => Some(ComputedValue::Keyword("line-through".to_owned())),
        (false, false) => None,
    }
}

fn parse_display(source: &str) -> Option<DisplayValue> {
    Some(match source.to_ascii_lowercase().as_str() {
        "none" => DisplayValue::None,
        "block" => DisplayValue::Block,
        "inline" => DisplayValue::Inline,
        "inline-block" => DisplayValue::InlineBlock,
        "flex" => DisplayValue::Flex,
        "grid" => DisplayValue::Grid,
        _ => return None,
    })
}

fn parse_box_sizing(source: &str) -> Option<BoxSizingValue> {
    Some(match source.to_ascii_lowercase().as_str() {
        "content-box" => BoxSizingValue::ContentBox,
        "border-box" => BoxSizingValue::BorderBox,
        _ => return None,
    })
}

fn parse_length_or_auto(source: &str, allow_negative: bool) -> Option<LengthOrAuto> {
    if source.eq_ignore_ascii_case("auto") {
        Some(LengthOrAuto::Auto)
    } else {
        parse_length(source, allow_negative).map(LengthOrAuto::Length)
    }
}

fn parse_length_or_none(source: &str) -> Option<LengthOrNone> {
    if source.eq_ignore_ascii_case("none") {
        Some(LengthOrNone::None)
    } else {
        parse_length(source, false).map(LengthOrNone::Length)
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
    named_color(&lower)
        .map(ColorValue::Named)
        .or_else(|| parse_hex_color(&lower))
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

fn parse_transform(source: &str) -> Option<TransformList> {
    if source.eq_ignore_ascii_case("none") {
        return Some(TransformList::default());
    }
    let mut operations = Vec::new();
    let mut rest = source.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let name = rest[..open].trim().to_ascii_lowercase();
        let mut depth = 0_i32;
        let mut close = None;
        for (offset, character) in rest[open..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let arguments = rest[open + 1..close].trim();
        operations.push(parse_transform_function(&name, arguments)?);
        rest = rest[close + 1..].trim_start();
    }
    (!operations.is_empty()).then_some(TransformList(operations))
}

fn parse_transform_function(name: &str, arguments: &str) -> Option<TransformOperation> {
    let values = split_function_arguments(arguments);
    match name {
        "translate" => {
            let x = parse_length(values.first()?, true)?;
            let y = match values.get(1) {
                Some(value) => parse_length(value, true)?,
                None => Length {
                    number: CssNumber::zero(),
                    unit: LengthUnit::Px,
                },
            };
            (values.len() <= 2).then_some(TransformOperation::Translate { x, y })
        }
        "translatex" if values.len() == 1 => Some(TransformOperation::Translate {
            x: parse_length(values.first()?, true)?,
            y: Length {
                number: CssNumber::zero(),
                unit: LengthUnit::Px,
            },
        }),
        "translatey" if values.len() == 1 => Some(TransformOperation::Translate {
            x: Length {
                number: CssNumber::zero(),
                unit: LengthUnit::Px,
            },
            y: parse_length(values.first()?, true)?,
        }),
        "scale" => {
            let x = CssNumber::parse(values.first()?)?;
            let y = match values.get(1) {
                Some(value) => CssNumber::parse(value)?,
                None => x,
            };
            (values.len() <= 2).then_some(TransformOperation::Scale { x, y })
        }
        "scalex" if values.len() == 1 => Some(TransformOperation::Scale {
            x: CssNumber::parse(values.first()?)?,
            y: CssNumber::from_scaled(super::model::CSS_NUMBER_SCALE),
        }),
        "scaley" if values.len() == 1 => Some(TransformOperation::Scale {
            x: CssNumber::from_scaled(super::model::CSS_NUMBER_SCALE),
            y: CssNumber::parse(values.first()?)?,
        }),
        "rotate" if values.len() == 1 => Some(TransformOperation::Rotate {
            degrees: parse_angle(values.first()?)?,
        }),
        "matrix" if values.len() == 6 => Some(TransformOperation::Matrix {
            a: CssNumber::parse(&values[0])?,
            b: CssNumber::parse(&values[1])?,
            c: CssNumber::parse(&values[2])?,
            d: CssNumber::parse(&values[3])?,
            e: CssNumber::parse(&values[4])?,
            f: CssNumber::parse(&values[5])?,
        }),
        _ => None,
    }
}

fn split_function_arguments(source: &str) -> Vec<String> {
    source
        .replace(',', " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn parse_angle(source: &str) -> Option<CssNumber> {
    let lower = source.trim().to_ascii_lowercase();
    if let Some(value) = lower.strip_suffix("deg") {
        return CssNumber::parse(value);
    }
    if let Some(value) = lower.strip_suffix("turn") {
        let turns = CssNumber::parse(value)?.scaled();
        return Some(CssNumber::from_scaled(turns.saturating_mul(360)));
    }
    None
}
