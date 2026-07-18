use meow_css::parse_stylesheet;
use meow_html::parse_utf8;

use crate::{CascadeOrigin, CascadeStylesheet, compute_styles};

use super::*;

#[test]
fn display_none_removes_the_principal_box_and_subtree() {
    let document =
        parse_utf8(b"<main><p id='gone'><span>hidden</span></p><p id='kept'>shown</p></main>")
            .document;
    let css = parse_stylesheet("main, p { display:block } #gone { display:none }");
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let dump = build_box_tree(&document, &styles).dump();

    assert!(!dump.contains("gone"));
    assert!(!dump.contains("hidden"));
    assert!(dump.contains("kept"));
    assert!(dump.contains("shown"));
}

#[test]
fn mixed_children_generate_anonymous_block_wrappers() {
    let document = parse_utf8(
        b"<main id='root'>before<span id='inline'>inside</span><section id='block'></section>after</main>",
    )
    .document;
    let css = parse_stylesheet("main, section { display:block } span { display:inline }");
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let dump = build_box_tree(&document, &styles).dump();

    assert_eq!(dump.matches("anonymous-block").count(), 2);
    assert!(dump.contains("principal-inline"));
    assert!(dump.contains("principal-block"));
}
