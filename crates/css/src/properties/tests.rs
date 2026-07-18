use crate::{CssWideKeyword, Declaration, SpecifiedValue};

use super::*;

fn declaration(name: &str, value: &str) -> Declaration {
    Declaration {
        name: name.to_owned(),
        value: value.to_owned(),
        important: false,
        location: crate::CssSourceLocation { line: 1, column: 1 },
    }
}

#[test]
fn registry_has_stable_names_initials_and_inheritance() {
    assert_eq!(ALL_PROPERTIES.len(), 13);
    assert_eq!(PropertyId::Color.name(), "color");
    assert!(PropertyId::Color.inherited());
    assert!(!PropertyId::Display.inherited());
    assert_eq!(PropertyId::Width.initial_value(), "auto");
}

#[test]
fn parses_supported_values_and_css_wide_keywords() {
    let color = parse_property_declaration(&declaration("COLOR", "  rebeccapurple  "))
        .expect("color is supported");
    assert_eq!(color.property, PropertyId::Color);
    assert_eq!(
        color.value,
        SpecifiedValue::Value("rebeccapurple".to_owned())
    );

    let unset = parse_property_declaration(&declaration("display", "UnSeT /**/"))
        .expect("display is supported");
    assert_eq!(unset.value, SpecifiedValue::CssWide(CssWideKeyword::Unset));
}

#[test]
fn ignores_unknown_and_empty_declarations() {
    assert!(parse_property_declaration(&declaration("border-radius", "4px")).is_none());
    assert!(parse_property_declaration(&declaration("color", "   ")).is_none());
}
