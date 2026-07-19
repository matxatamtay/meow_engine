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

fn flow_layout(html: &str, css: &str, width: u32, height: u32) -> (Document, LayoutTree) {
    let document = parse_utf8(html.as_bytes()).document;
    let css = parse_stylesheet(css);
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let boxes = build_box_tree(&document, &styles);
    let layout = layout_normal_flow(&boxes, &styles, LayoutViewport::new(width, height));
    (document, layout)
}

#[test]
fn adjacent_block_margins_collapse_to_the_larger_positive_margin() {
    let (document, tree) = flow_layout(
        "<main><section id='first'></section><section id='second'></section></main>",
        "html, body, main, section { display:block } #first { height:20px; margin-bottom:10px } #second { height:30px; margin-top:15px }",
        300,
        400,
    );
    let first = tree.find_source(source(&document, "#first")).unwrap();
    let second = tree.find_source(source(&document, "#second")).unwrap();
    let first_bottom = first.border_box_rect().y.0 + first.border_box_height().0;
    assert_eq!(second.border_box_rect().y.0 - first_bottom, 15);
}

#[test]
fn negative_and_mixed_margins_follow_the_w15_subset() {
    assert_eq!(collapse_margins(CssPx(-10), CssPx(-5)), CssPx(-10));
    assert_eq!(collapse_margins(CssPx(20), CssPx(-5)), CssPx(15));
    assert_eq!(collapse_margins(CssPx(7), CssPx(12)), CssPx(12));
}

#[test]
fn explicit_height_records_vertical_overflow_without_clipping_layout() {
    let (document, tree) = flow_layout(
        "<main id='outer'><section id='inner'></section></main>",
        "html, body, main, section { display:block } #outer { height:20px } #inner { height:40px }",
        300,
        400,
    );
    let outer = tree.find_source(source(&document, "#outer")).unwrap();
    let inner = tree.find_source(source(&document, "#inner")).unwrap();
    assert_eq!(outer.content.height, CssPx(20));
    assert_eq!(inner.content.height, CssPx(40));
    assert!(outer.overflow.vertical);
    assert_eq!(outer.overflow.scroll_height, CssPx(40));
}

#[test]
fn min_and_max_height_clamp_content_height() {
    let (document, tree) = flow_layout(
        "<main><section id='min'></section><section id='max'><span>line</span><span>line</span></section></main>",
        "html, body, main, section { display:block } span { display:block; height:20px } #min { height:10px; min-height:30px } #max { max-height:25px }",
        300,
        400,
    );
    let min = tree.find_source(source(&document, "#min")).unwrap();
    let max = tree.find_source(source(&document, "#max")).unwrap();
    assert_eq!(min.content.height, CssPx(30));
    assert_eq!(max.content.height, CssPx(25));
    assert!(max.overflow.vertical);
    assert_eq!(max.overflow.scroll_height, CssPx(40));
}

#[test]
fn flex_row_distributes_grow_and_gap_for_nav_layout() {
    let (document, tree) = flow_layout(
        "<nav id='nav'><a id='brand'>Brand</a><a id='links'>Links</a></nav>",
        "html, body { display:block; margin:0 } #nav { display:flex; width:300px; gap:10px } #brand { flex:1 1 0% } #links { flex:2 1 0% }",
        320,
        200,
    );
    let brand = tree.find_source(source(&document, "#brand")).unwrap();
    let links = tree.find_source(source(&document, "#links")).unwrap();
    assert_eq!(brand.border_box_width(), CssPx(96));
    assert_eq!(links.border_box_width(), CssPx(194));
    assert_eq!(
        links.border_box_rect().x.0 - (brand.border_box_rect().x.0 + 96),
        10
    );
}

#[test]
fn flex_row_shrinks_cards_by_weighted_basis() {
    let (document, tree) = flow_layout(
        "<main id='cards'><section id='a'></section><section id='b'></section></main>",
        "html, body { display:block; margin:0 } #cards { display:flex; width:300px } section { display:block; flex:0 1 200px }",
        320,
        200,
    );
    assert_eq!(
        tree.find_source(source(&document, "#a"))
            .unwrap()
            .border_box_width(),
        CssPx(150)
    );
    assert_eq!(
        tree.find_source(source(&document, "#b"))
            .unwrap()
            .border_box_width(),
        CssPx(150)
    );
}
