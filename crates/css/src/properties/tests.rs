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
    assert_eq!(ALL_PROPERTIES.len(), 26);
    assert_eq!(W11_SNAPSHOT_PROPERTIES.len(), 13);
    assert_eq!(PropertyId::Color.name(), "color");
    assert!(PropertyId::Color.inherited());
    assert!(!PropertyId::Display.inherited());
    assert_eq!(PropertyId::MarginTop.initial_value(), "0px");
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
fn expands_box_shorthands_in_css_order() {
    let values = parse_property_declarations(&declaration("margin", "1px 2px 3px"));
    assert_eq!(values.len(), 4);
    assert_eq!(values[0].property, PropertyId::MarginTop);
    assert_eq!(values[1].value, SpecifiedValue::Value("2px".to_owned()));
    assert_eq!(values[2].value, SpecifiedValue::Value("3px".to_owned()));
    assert_eq!(values[3].value, SpecifiedValue::Value("2px".to_owned()));
}

#[test]
fn typed_values_validate_and_serialize_deterministically() {
    let width = parse_computed_value(PropertyId::Width, "12.5000px").unwrap();
    let color = parse_computed_value(PropertyId::Color, "#AbC8").unwrap();
    let opacity = parse_computed_value(PropertyId::Opacity, "1.4").unwrap();
    assert_eq!(width.to_css(), "12.5px");
    assert_eq!(color.to_css(), "#aabbcc88");
    assert_eq!(opacity.to_css(), "1");
    assert!(parse_computed_value(PropertyId::PaddingTop, "-1px").is_none());
}

#[test]
fn ignores_unknown_empty_and_custom_declarations() {
    assert!(parse_property_declarations(&declaration("border-radius", "4px")).is_empty());
    assert!(parse_property_declarations(&declaration("color", "   ")).is_empty());
    assert!(parse_property_declarations(&declaration("--theme", "red")).is_empty());
}
