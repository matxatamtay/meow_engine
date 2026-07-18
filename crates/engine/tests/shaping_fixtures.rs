use std::{fs, path::Path};

use meow_engine::{FontDatabase, FontRequest, shape_text};

const EXPECTED_FIXTURE_COUNT: usize = 3;

#[test]
fn vietnamese_arabic_and_mixed_direction_fixtures_are_stable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shaping");
    let mut fixtures = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for fixture in fixtures {
        let text = fs::read_to_string(fixture.join("input.txt")).unwrap();
        let text = text.trim_end_matches('\n');
        let mut database = FontDatabase::deterministic();
        let request = FontRequest::new(["Meow Sans", "Meow Arabic"]);
        let actual = shape_text(&mut database, &request, text).dump();
        let expected_path = fixture.join("expected.dump");
        if std::env::var_os("UPDATE_W18_SNAPSHOTS").is_some() {
            fs::write(&expected_path, &actual).unwrap();
        }
        let expected = fs::read_to_string(expected_path).unwrap();
        assert_eq!(actual, expected, "fixture {} changed", fixture.display());
    }
}
