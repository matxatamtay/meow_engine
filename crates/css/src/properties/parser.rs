use cssparser::{Parser, ParserInput};

use crate::Declaration;

use super::model::{CssWideKeyword, PropertyDeclaration, PropertyId, SpecifiedValue};

/// Converts one syntax-level declaration into the supported W11 property subset.
#[must_use]
pub fn parse_property_declaration(declaration: &Declaration) -> Option<PropertyDeclaration> {
    let property = PropertyId::from_name(&declaration.name)?;
    let value = declaration.value.trim();
    if value.is_empty() {
        return None;
    }

    let value = parse_css_wide_keyword(value)
        .map(SpecifiedValue::CssWide)
        .unwrap_or_else(|| SpecifiedValue::Value(value.to_owned()));

    Some(PropertyDeclaration { property, value })
}

fn parse_css_wide_keyword(value: &str) -> Option<CssWideKeyword> {
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
