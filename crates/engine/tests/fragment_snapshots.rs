use std::{fs, path::Path};

use meow_css::parse_stylesheet;
use meow_engine::{
    CascadeOrigin, CascadeStylesheet, FontDatabase, LayoutViewport, build_box_tree, compute_styles,
    layout_fragment_tree,
};
use meow_html::parse_utf8;

const EXPECTED_FIXTURE_COUNT: usize = 3;

#[test]
fn inline_fragment_fixtures_are_byte_stable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inline-fragments");
    let mut fixtures = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for fixture in fixtures {
        let html = fs::read(fixture.join("document.html")).unwrap();
        let css = parse_stylesheet(&fs::read_to_string(fixture.join("author.css")).unwrap());
        let document = parse_utf8(&html).document;
        let styles = compute_styles(
            &document,
            &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
        );
        let boxes = build_box_tree(&document, &styles);
        let mut fonts = FontDatabase::deterministic();
        let actual =
            layout_fragment_tree(&boxes, &styles, LayoutViewport::new(320, 360), &mut fonts)
                .fragments
                .dump();
        let expected_path = fixture.join("expected.dump");
        if std::env::var_os("UPDATE_W20_SNAPSHOTS").is_some() {
            fs::write(&expected_path, &actual).unwrap();
        }
        let expected = fs::read_to_string(expected_path).unwrap();
        assert_eq!(actual, expected, "fixture {} changed", fixture.display());
    }
}
