use meow_html::parse_utf8;

const FIXTURES: &[(&str, &[u8], &str)] = &[
    (
        "unclosed-list",
        include_bytes!("fixtures/unclosed-list.html"),
        include_str!("fixtures/unclosed-list.dom"),
    ),
    (
        "nested-paragraphs",
        include_bytes!("fixtures/nested-paragraphs.html"),
        include_str!("fixtures/nested-paragraphs.dom"),
    ),
];

#[test]
fn malformed_html_fixtures_have_stable_dom_dumps() {
    for (name, html, expected) in FIXTURES {
        let actual = parse_utf8(html).document.dump();
        assert_eq!(&actual, expected, "fixture {name}");
    }
}
