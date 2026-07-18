use meow_css::{PropertyId, parse_selector_list, parse_stylesheet};
use meow_html::{Document, parse_utf8};

use super::*;

fn document(source: &str) -> Document {
    parse_utf8(source.as_bytes()).document
}

fn element(document: &Document, selector: &str) -> meow_html::NodeHandle {
    document
        .query_selector(&parse_selector_list(selector).expect("test selector should parse"))
        .expect("test element should exist")
}

#[test]
fn origin_and_important_order_follow_the_w11_ladder() {
    let document = document("<div id='target' class='card'></div>");
    let ua = parse_stylesheet("#target { color: navy !important; display: block; }");
    let user = parse_stylesheet(".card { color: green !important; display: grid; }");
    let author = parse_stylesheet("div { color: red !important; display: flex; }");
    let snapshot = compute_styles(
        &document,
        &[
            CascadeStylesheet::new(CascadeOrigin::UserAgent, &ua),
            CascadeStylesheet::new(CascadeOrigin::User, &user),
            CascadeStylesheet::new(CascadeOrigin::Author, &author),
        ],
    );
    let target = element(&document, "#target");
    let style = snapshot.style_for(target.id()).unwrap();

    assert_eq!(style.get(PropertyId::Color), "navy");
    assert_eq!(style.get(PropertyId::Display), "flex");
}

#[test]
fn specificity_stylesheet_rule_and_declaration_order_break_ties() {
    let document = document("<div id='target' class='card'></div>");
    let first =
        parse_stylesheet(".card { width: 1px; } #target { width: 2px; width: 3px; height: 1px; }");
    let second = parse_stylesheet("#target { width: 4px; } #target { height: 2px; }");
    let snapshot = compute_styles(
        &document,
        &[
            CascadeStylesheet::new(CascadeOrigin::Author, &first),
            CascadeStylesheet::new(CascadeOrigin::Author, &second),
        ],
    );
    let style = snapshot
        .style_for(element(&document, "#target").id())
        .unwrap();

    assert_eq!(style.get(PropertyId::Width), "4px");
    assert_eq!(style.get(PropertyId::Height), "2px");
}

#[test]
fn inheritance_initial_and_unset_resolve_parent_first() {
    let document = document("<main id='parent'><span id='child'><em id='leaf'></em></span></main>");
    let author = parse_stylesheet(
        "#parent { color: red; font-size: 20px; width: 10px; display: block; }
         #child { color: unset; font-size: initial; width: inherit; display: unset; }
         #leaf { color: initial; visibility: hidden; opacity: .5; }",
    );
    let snapshot = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &author)],
    );
    let child = snapshot
        .style_for(element(&document, "#child").id())
        .unwrap();
    let leaf = snapshot
        .style_for(element(&document, "#leaf").id())
        .unwrap();

    assert_eq!(child.get(PropertyId::Color), "red");
    assert_eq!(child.get(PropertyId::FontSize), "medium");
    assert_eq!(child.get(PropertyId::Width), "10px");
    assert_eq!(child.get(PropertyId::Display), "inline");
    assert_eq!(leaf.get(PropertyId::Color), "black");
    assert_eq!(leaf.get(PropertyId::FontSize), "medium");
    assert_eq!(leaf.get(PropertyId::Visibility), "hidden");
    assert_eq!(leaf.get(PropertyId::Opacity), ".5");
}

#[test]
fn unsupported_selectors_are_ignored_with_stable_diagnostics() {
    let document = document("<div id='target'></div>");
    let author = parse_stylesheet("div:hover { color: red; } #target { color: blue; }");
    let snapshot = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &author)],
    );

    assert_eq!(snapshot.diagnostics().len(), 1);
    assert!(
        snapshot.diagnostics()[0]
            .message
            .contains("unsupported pseudo-class")
    );
    assert_eq!(
        snapshot
            .style_for(element(&document, "#target").id())
            .unwrap()
            .get(PropertyId::Color),
        "blue"
    );
}
