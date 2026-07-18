use super::*;

#[test]
fn parses_supported_selector_shape_and_specificity() {
    let list =
        parse_selector_list("main#app.card[data-state='open' i] > ul.items li:nth-child(2n+1)")
            .expect("selector should parse");
    let selector = &list.selectors[0];

    assert_eq!(selector.segments.len(), 3);
    assert_eq!(selector.segments[1].combinator, Some(Combinator::Child));
    assert_eq!(
        selector.segments[2].combinator,
        Some(Combinator::Descendant)
    );
    assert_eq!(
        selector.specificity(),
        Specificity {
            ids: 1,
            classes: 4,
            types: 3,
        }
    );
}

#[test]
fn parses_all_attribute_matchers_and_modifiers() {
    let list = parse_selector_list("[a][b=x][c~=x][d|=x][e^=x][f$=x][g*=x i][h='x' s]")
        .expect("attributes should parse");
    let selectors = &list.selectors[0].segments[0].compound.simple_selectors;

    assert_eq!(selectors.len(), 8);
    assert!(matches!(
        selectors[0],
        SimpleSelector::Attribute(AttributeSelector {
            matcher: AttributeMatcher::Exists,
            ..
        })
    ));
    assert!(matches!(
        selectors[6],
        SimpleSelector::Attribute(AttributeSelector {
            case_sensitivity: AttributeCaseSensitivity::AsciiInsensitive,
            ..
        })
    ));
    assert!(matches!(
        selectors[7],
        SimpleSelector::Attribute(AttributeSelector {
            case_sensitivity: AttributeCaseSensitivity::CaseSensitive,
            ..
        })
    ));
}

#[test]
fn nth_expressions_match_one_based_indices() {
    assert!(AnPlusB { a: 2, b: 1 }.matches(5));
    assert!(!AnPlusB { a: 2, b: 1 }.matches(4));
    assert!(AnPlusB { a: -1, b: 3 }.matches(1));
    assert!(AnPlusB { a: -1, b: 3 }.matches(3));
    assert!(!AnPlusB { a: -1, b: 3 }.matches(4));
    assert!(AnPlusB { a: 0, b: 2 }.matches(2));
}

#[test]
fn rejects_unsupported_and_malformed_selectors() {
    for source in [
        "",
        "a,",
        "a >",
        ". class",
        "a::before",
        "a:hover",
        "a:not(.x)",
        "[a?=b]",
        "svg|circle",
    ] {
        assert!(
            parse_selector_list(source).is_err(),
            "{source:?} should be rejected"
        );
    }
}

#[test]
fn css_escapes_are_decoded_by_the_tokenizer() {
    let list = parse_selector_list(r".caf\e9 #\31 23").expect("escapes should parse");
    assert!(matches!(
        &list.selectors[0].segments[0].compound.simple_selectors[0],
        SimpleSelector::Class(value) if value == "café"
    ));
    assert!(matches!(
        &list.selectors[0].segments[0].compound.simple_selectors[1],
        SimpleSelector::Id(value) if value == "123"
    ));
}
