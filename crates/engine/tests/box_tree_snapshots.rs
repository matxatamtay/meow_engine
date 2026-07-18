use std::{fs, path::Path};

use meow_css::parse_stylesheet;
use meow_engine::{CascadeOrigin, CascadeStylesheet, build_box_tree, compute_styles};
use meow_html::parse_utf8;

const EXPECTED_FIXTURE_COUNT: usize = 4;

#[test]
fn box_tree_fixtures_are_byte_stable_and_distinct_from_dom_dumps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/box-tree");
    let mut fixtures = fs::read_dir(&root)
        .expect("box-tree fixture directory should exist")
        .map(|entry| entry.expect("fixture entry should be readable").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for fixture in fixtures {
        let html = fs::read(fixture.join("document.html")).expect("fixture HTML should exist");
        let css = parse_stylesheet(
            &fs::read_to_string(fixture.join("author.css")).expect("fixture CSS should exist"),
        );
        let document = parse_utf8(&html).document;
        let styles = compute_styles(
            &document,
            &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
        );
        let actual = build_box_tree(&document, &styles).dump();
        assert!(actual.starts_with("#box-tree\n"));
        assert_ne!(actual, document.dump());
        let expected_path = fixture.join("expected.dump");
        if std::env::var_os("UPDATE_W13_SNAPSHOTS").is_some() {
            fs::write(&expected_path, &actual).expect("snapshot should update");
        }
        let expected = fs::read_to_string(&expected_path).expect("expected dump should exist");
        assert_eq!(actual, expected, "fixture {} changed", fixture.display());
    }
}
