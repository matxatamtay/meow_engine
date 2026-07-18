use std::{fs, path::Path};

use meow_css::parse_selector_list;
use meow_html::parse_utf8;

const EXPECTED_CASE_COUNT: usize = 70;
const EXPECTED_INVALID_COUNT: usize = 19;

#[test]
fn selector_conformance_cases_match_in_document_order() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/selectors");
    let html = fs::read(fixtures.join("document.html")).expect("selector document should exist");
    let cases = fs::read_to_string(fixtures.join("cases.tsv"))
        .expect("selector conformance cases should exist");
    let document = parse_utf8(&html).document;
    let mut count = 0;

    for (line_index, line) in cases.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') && !line.contains('\t') {
            continue;
        }
        count += 1;
        let (selector_source, expected) = line.split_once('\t').unwrap_or_else(|| {
            panic!(
                "case line {} must contain one tab: {line:?}",
                line_index + 1
            )
        });
        let selectors = parse_selector_list(selector_source).unwrap_or_else(|error| {
            panic!(
                "case line {} selector {selector_source:?} should parse: {error}",
                line_index + 1
            )
        });
        let actual = document
            .query_selector_all(&selectors)
            .into_iter()
            .map(|element| {
                document
                    .element_attribute(&element, "id")
                    .unwrap_or_else(|| panic!("every fixture element must have an id: {element:?}"))
            })
            .collect::<Vec<_>>()
            .join(",");
        let expected = if expected == "-" { "" } else { expected };
        assert_eq!(
            actual,
            expected,
            "selector {selector_source:?} from case line {}",
            line_index + 1
        );
    }

    assert_eq!(
        count, EXPECTED_CASE_COUNT,
        "W10 selector conformance case count changed"
    );
}

#[test]
fn invalid_selector_corpus_is_rejected() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/selectors/invalid.txt");
    let source = fs::read_to_string(path).expect("invalid selector corpus should exist");
    let mut count = 0;

    for (line_index, line) in source.lines().enumerate() {
        let selector = line.trim();
        if selector.is_empty() || selector.starts_with("# ") {
            continue;
        }
        count += 1;
        assert!(
            parse_selector_list(selector).is_err(),
            "invalid selector on line {} was accepted: {selector:?}",
            line_index + 1
        );
    }

    assert_eq!(
        count, EXPECTED_INVALID_COUNT,
        "W10 invalid selector case count changed"
    );
}
