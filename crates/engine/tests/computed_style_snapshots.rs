use std::{fs, path::Path};

use meow_css::parse_stylesheet;
use meow_engine::{CascadeOrigin, CascadeStylesheet, compute_styles};
use meow_html::parse_utf8;

const EXPECTED_FIXTURE_COUNT: usize = 2;

#[test]
fn computed_style_fixtures_are_byte_stable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/computed-style");
    let mut fixtures = fs::read_dir(&root)
        .expect("computed-style fixture directory should exist")
        .map(|entry| entry.expect("fixture entry should be readable").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for fixture in fixtures {
        let html = fs::read(fixture.join("document.html")).expect("fixture HTML should exist");
        let ua = parse_stylesheet(&read_optional(&fixture.join("ua.css")));
        let user = parse_stylesheet(&read_optional(&fixture.join("user.css")));
        let author = parse_stylesheet(&read_optional(&fixture.join("author.css")));
        let document = parse_utf8(&html).document;
        let actual = compute_styles(
            &document,
            &[
                CascadeStylesheet::new(CascadeOrigin::UserAgent, &ua),
                CascadeStylesheet::new(CascadeOrigin::User, &user),
                CascadeStylesheet::new(CascadeOrigin::Author, &author),
            ],
        )
        .dump();
        let expected_path = fixture.join("expected.dump");
        if std::env::var_os("UPDATE_W11_SNAPSHOTS").is_some() {
            fs::write(&expected_path, &actual).expect("snapshot should update");
        }
        let expected = fs::read_to_string(&expected_path).expect("expected dump should exist");
        assert_eq!(actual, expected, "fixture {} changed", fixture.display());
    }
}

fn read_optional(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => panic!("could not read {}: {error}", path.display()),
    }
}
