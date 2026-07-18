use crate::{FontDatabase, FontRequest};

use super::*;

#[test]
fn combining_marks_share_the_base_cluster_and_zero_advance() {
    let mut database = FontDatabase::deterministic();
    let shaped = shape_text(
        &mut database,
        &FontRequest::new(["Meow Sans Vietnamese"]),
        "e\u{302}\u{301}",
    );
    let glyphs = &shaped.runs[0].glyphs;
    assert_eq!(glyphs.len(), 3);
    assert!(glyphs.iter().all(|glyph| glyph.cluster == 0));
    assert_eq!(glyphs[1].advance, 0);
    assert_eq!(glyphs[2].advance, 0);
}

#[test]
fn rtl_run_reverses_glyph_visual_order() {
    let mut database = FontDatabase::deterministic();
    let shaped = shape_text(
        &mut database,
        &FontRequest::new(["Meow Arabic"]),
        "abc مرحبا",
    );
    let rtl = shaped
        .runs
        .iter()
        .find(|run| run.direction == TextDirection::Rtl)
        .unwrap();
    assert_eq!(rtl.glyphs.first().unwrap().character, 'ا');
    assert_eq!(rtl.glyphs.last().unwrap().character, 'م');
}
