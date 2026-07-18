use meow_css::parse_stylesheet;
use meow_html::parse_utf8;

use crate::{
    CascadeOrigin, CascadeStylesheet, FontDatabase, LayoutViewport, build_box_tree, compute_styles,
    layout_fragment_tree,
};

#[test]
fn inline_styles_survive_into_glyph_fragments() {
    let document = parse_utf8(
        b"<p id='p'>plain <strong>bold</strong> <em>italic</em> <u>under</u> <del>strike</del></p>",
    )
    .document;
    let css = parse_stylesheet(
        "html, body, p { display:block } strong, em, u, del { display:inline } \
         strong { font-weight:bold } em { font-style:italic } \
         u { color:red; text-decoration-line:underline } del { text-decoration-line:line-through }",
    );
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let boxes = build_box_tree(&document, &styles);
    let mut fonts = FontDatabase::deterministic();
    let output = layout_fragment_tree(&boxes, &styles, LayoutViewport::new(240, 160), &mut fonts);
    let glyphs = output.fragments.paragraphs()[0]
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .collect::<Vec<_>>();
    assert!(glyphs.iter().any(|glyph| glyph.style.weight == 700));
    assert!(
        glyphs
            .iter()
            .any(|glyph| glyph.style.slant == crate::FontSlant::Italic)
    );
    assert!(glyphs.iter().any(|glyph| glyph.style.decorations.underline));
    assert!(
        glyphs
            .iter()
            .any(|glyph| glyph.style.decorations.line_through)
    );
    assert!(glyphs.iter().any(|glyph| glyph.style.color.red() == 255));
}

#[test]
fn measured_paragraph_height_pushes_following_block() {
    let document = parse_utf8(
        b"<main><p id='text'>This paragraph wraps across several deterministic lines.</p><footer id='footer'></footer></main>",
    )
    .document;
    let css = parse_stylesheet(
        "html, body, main, p, footer { display:block } main { width:96px } footer { height:20px }",
    );
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let boxes = build_box_tree(&document, &styles);
    let mut fonts = FontDatabase::deterministic();
    let output = layout_fragment_tree(&boxes, &styles, LayoutViewport::new(160, 240), &mut fonts);
    let paragraph = &output.fragments.paragraphs()[0];
    let footer = document
        .query_selector(&meow_css::parse_selector_list("#footer").unwrap())
        .unwrap();
    let footer = output.layout.find_source(footer.id()).unwrap();
    assert!(footer.border_box_rect().y.0 >= paragraph.rect.y.0 + paragraph.rect.height.0);
}
