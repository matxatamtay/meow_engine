use encoding_rs::{UTF_8, WINDOWS_1252};

use super::*;

#[test]
fn creates_document_skeleton_for_empty_input() {
    let parsed = parse_utf8(b"");
    assert_eq!(
        parsed.document.dump(),
        "#document\n  <html>\n    <head>\n    <body>\n"
    );
}

#[test]
fn parses_malformed_html_into_a_stable_tree() {
    let parsed = parse_utf8(b"<!doctype html><title>x</title><p id=p>one<div>two</p>three");
    assert_eq!(
        parsed.document.dump(),
        concat!(
            "#document\n",
            "  <!DOCTYPE html>\n",
            "  <html>\n",
            "    <head>\n",
            "      <title>\n",
            "        \"x\"\n",
            "    <body>\n",
            "      <p id=\"p\">\n",
            "        \"one\"\n",
            "      <div>\n",
            "        \"two\"\n",
            "        <p>\n",
            "        \"three\"\n",
        )
    );
}

#[test]
fn streaming_decoder_preserves_split_multibyte_sequences() {
    let source = "<p>mèo 🐈</p>".as_bytes();
    let mut parser = StreamingParser::new(UTF_8);
    for byte in source {
        parser.feed(std::slice::from_ref(byte));
    }
    let parsed = parser.finish();

    assert!(!parsed.had_replacements);
    assert!(parsed.document.dump().contains("mèo 🐈"));
}

#[test]
fn decodes_legacy_encoding_and_reports_replacements() {
    let parsed = parse_bytes(b"<p>caf\xe9</p>", WINDOWS_1252);
    assert!(parsed.document.dump().contains("café"));
    assert!(!parsed.had_replacements);

    let malformed = parse_utf8(b"<p>\xff</p>");
    assert!(malformed.had_replacements);
    assert!(malformed.document.dump().contains('�'));
}

#[test]
fn exposes_first_base_href() {
    let parsed = parse_utf8(b"<base href='../assets/'><base href='/ignored/'>");
    assert_eq!(
        parsed.document.first_base_href().as_deref(),
        Some("../assets/")
    );
}

#[test]
fn discovers_inline_and_linked_stylesheets_in_tree_order() {
    let parsed = parse_utf8(
        br#"<style type=" TEXT/CSS " media="screen">a { color: red }</style>
            <link rel="preload StyleSheet" type=" text/css " href="theme.css" media="print">
            <style type="text/less">ignored</style>
            <link rel="stylesheet" type="text/less" href="ignored.less">
            <link rel="stylesheet">"#,
    );
    let candidates = parsed.document.stylesheet_candidates();

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].media.as_deref(), Some("screen"));
    assert!(matches!(
        &candidates[0].kind,
        StylesheetCandidateKind::Inline(css) if css == "a { color: red }"
    ));
    assert_eq!(candidates[1].media.as_deref(), Some("print"));
    assert!(matches!(
        &candidates[1].kind,
        StylesheetCandidateKind::Linked(href) if href == "theme.css"
    ));
}
