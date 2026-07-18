use meow_css::parse_stylesheet;
use meow_html::{Document, parse_utf8};

use crate::{
    CascadeOrigin, CascadeStylesheet, build_box_tree, compute_styles, parse_selector_list,
};

use super::*;

fn layout(html: &str, css: &str, width: u32) -> (Document, LayoutTree) {
    let document = parse_utf8(html.as_bytes()).document;
    let css = parse_stylesheet(css);
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let boxes = build_box_tree(&document, &styles);
    let layout = layout_box_tree(&boxes, &styles, LayoutViewport::new(width, 600));
    (document, layout)
}

fn source(document: &Document, selector: &str) -> meow_html::NodeId {
    document
        .query_selector(&parse_selector_list(selector).unwrap())
        .unwrap()
        .id()
}

#[test]
fn auto_width_fills_the_containing_block_after_edges() {
    let (document, tree) = layout(
        "<main id='target'></main>",
        "html, body, main { display:block } #target { margin:0 10px; padding:0 20px; border-width:0 5px; }",
        300,
    );
    let target = tree.find_source(source(&document, "#target")).unwrap();
    assert_eq!(target.content.width, CssPx(230));
    assert_eq!(target.border_box_width(), CssPx(280));
}

#[test]
fn border_box_and_auto_margins_center_the_used_box() {
    let (document, tree) = layout(
        "<main id='target'></main>",
        "html, body, main { display:block } #target { width:200px; box-sizing:border-box; margin:0 auto; padding:0 20px; border-width:0 5px; }",
        300,
    );
    let target = tree.find_source(source(&document, "#target")).unwrap();
    assert_eq!(target.content.width, CssPx(150));
    assert_eq!(target.margin.left, CssPx(50));
    assert_eq!(target.margin.right, CssPx(50));
}

#[test]
fn min_and_max_width_clamp_the_selected_sizing_box() {
    let (document, tree) = layout(
        "<main><section id='min'></section><section id='max'></section></main>",
        "html, body, main, section { display:block } #min { width:20px; min-width:80px } #max { width:180px; max-width:120px }",
        300,
    );
    assert_eq!(
        tree.find_source(source(&document, "#min"))
            .unwrap()
            .content
            .width,
        CssPx(80)
    );
    assert_eq!(
        tree.find_source(source(&document, "#max"))
            .unwrap()
            .content
            .width,
        CssPx(120)
    );
}
