use cssparser::{Parser, ParserInput};

use crate::Declaration;

use super::model::{CssWideKeyword, PropertyDeclaration, PropertyId, SpecifiedValue};

#[must_use]
pub fn parse_property_declaration(declaration: &Declaration) -> Option<PropertyDeclaration> {
    parse_property_declarations(declaration).into_iter().next()
}

#[must_use]
pub fn parse_property_declarations(declaration: &Declaration) -> Vec<PropertyDeclaration> {
    let value = declaration.value.trim();
    if value.is_empty() || declaration.name.starts_with("--") {
        return Vec::new();
    }
    let name = declaration.name.to_ascii_lowercase();
    match name.as_str() {
        "margin" => expand_box(
            value,
            [
                PropertyId::MarginTop,
                PropertyId::MarginRight,
                PropertyId::MarginBottom,
                PropertyId::MarginLeft,
            ],
        ),
        "padding" => expand_box(
            value,
            [
                PropertyId::PaddingTop,
                PropertyId::PaddingRight,
                PropertyId::PaddingBottom,
                PropertyId::PaddingLeft,
            ],
        ),
        "border-width" => expand_box(
            value,
            [
                PropertyId::BorderTopWidth,
                PropertyId::BorderRightWidth,
                PropertyId::BorderBottomWidth,
                PropertyId::BorderLeftWidth,
            ],
        ),
        "flex" => expand_flex(value),
        _ => PropertyId::from_name(&name)
            .map(|property| PropertyDeclaration {
                property,
                value: specified_value(value),
            })
            .into_iter()
            .collect(),
    }
}

#[must_use]
pub fn parse_css_wide_keyword(value: &str) -> Option<CssWideKeyword> {
    let mut input = ParserInput::new(value);
    let mut input = Parser::new(&mut input);
    let ident = input.expect_ident().ok()?.to_string();
    input.expect_exhausted().ok()?;
    if ident.eq_ignore_ascii_case("inherit") {
        Some(CssWideKeyword::Inherit)
    } else if ident.eq_ignore_ascii_case("initial") {
        Some(CssWideKeyword::Initial)
    } else if ident.eq_ignore_ascii_case("unset") {
        Some(CssWideKeyword::Unset)
    } else {
        None
    }
}

fn specified_value(value: &str) -> SpecifiedValue {
    parse_css_wide_keyword(value)
        .map(SpecifiedValue::CssWide)
        .unwrap_or_else(|| SpecifiedValue::Value(value.to_owned()))
}

fn expand_box(value: &str, properties: [PropertyId; 4]) -> Vec<PropertyDeclaration> {
    if let Some(keyword) = parse_css_wide_keyword(value) {
        return properties
            .into_iter()
            .map(|property| PropertyDeclaration {
                property,
                value: SpecifiedValue::CssWide(keyword),
            })
            .collect();
    }
    let components = split_top_level_whitespace(value);
    let sides: [&str; 4] = match components.as_slice() {
        [all] => [all, all, all, all],
        [vertical, horizontal] => [vertical, horizontal, vertical, horizontal],
        [top, horizontal, bottom] => [top, horizontal, bottom, horizontal],
        [top, right, bottom, left] => [top, right, bottom, left],
        _ => return Vec::new(),
    };
    properties
        .into_iter()
        .zip(sides)
        .map(|(property, value)| PropertyDeclaration {
            property,
            value: SpecifiedValue::Value(value.to_owned()),
        })
        .collect()
}

fn split_top_level_whitespace(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut output = Vec::new();
    let mut depth = 0_u32;
    let mut start = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => {
                depth += 1;
                start.get_or_insert(index);
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                start.get_or_insert(index);
            }
            byte if byte.is_ascii_whitespace() && depth == 0 => {
                if let Some(component_start) = start.take() {
                    output.push(source[component_start..index].trim());
                }
            }
            _ => {
                start.get_or_insert(index);
            }
        }
        index += 1;
    }
    if let Some(component_start) = start {
        output.push(source[component_start..].trim());
    }
    output
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect()
}

fn expand_flex(value: &str) -> Vec<PropertyDeclaration> {
    if let Some(keyword) = parse_css_wide_keyword(value) {
        return [
            PropertyId::FlexGrow,
            PropertyId::FlexShrink,
            PropertyId::FlexBasis,
        ]
        .into_iter()
        .map(|property| PropertyDeclaration {
            property,
            value: SpecifiedValue::CssWide(keyword),
        })
        .collect();
    }
    let lower = value.trim().to_ascii_lowercase();
    let triple = match lower.as_str() {
        "none" => Some(("0", "0", "auto")),
        "auto" => Some(("1", "1", "auto")),
        "initial" => Some(("0", "1", "auto")),
        _ => None,
    };
    let (grow, shrink, basis) = if let Some((grow, shrink, basis)) = triple {
        (grow.to_owned(), shrink.to_owned(), basis.to_owned())
    } else {
        let tokens = split_top_level_whitespace(value);
        match tokens.as_slice() {
            [grow] => ((*grow).to_owned(), "1".to_owned(), "0%".to_owned()),
            [grow, second] if is_number(second) => {
                ((*grow).to_owned(), (*second).to_owned(), "0%".to_owned())
            }
            [grow, basis] => ((*grow).to_owned(), "1".to_owned(), (*basis).to_owned()),
            [grow, shrink, basis] => (
                (*grow).to_owned(),
                (*shrink).to_owned(),
                (*basis).to_owned(),
            ),
            _ => return Vec::new(),
        }
    };
    [
        (PropertyId::FlexGrow, grow),
        (PropertyId::FlexShrink, shrink),
        (PropertyId::FlexBasis, basis),
    ]
    .into_iter()
    .map(|(property, value)| PropertyDeclaration {
        property,
        value: SpecifiedValue::Value(value),
    })
    .collect()
}

fn is_number(value: &str) -> bool {
    value.parse::<f64>().is_ok()
}
