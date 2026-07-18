use std::{collections::BTreeSet, fs, path::Path};

const EXPECTED_FIXTURE_COUNT: usize = 100;

#[test]
fn css_fixtures_have_stable_rule_dumps() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut css_paths = fixture_paths(&directory, "css");
    let dump_paths = fixture_paths(&directory, "dump");

    assert_eq!(
        css_paths.len(),
        EXPECTED_FIXTURE_COUNT,
        "W9 requires exactly {EXPECTED_FIXTURE_COUNT} CSS fixtures"
    );
    assert_eq!(
        dump_paths.len(),
        EXPECTED_FIXTURE_COUNT,
        "every CSS fixture must have exactly one golden dump"
    );

    let css_stems = stems(&css_paths);
    let dump_stems = stems(&dump_paths);
    assert_eq!(
        css_stems, dump_stems,
        "CSS fixtures and dumps must be paired"
    );

    css_paths.sort_by_key(|path| fixture_number(path));
    for (index, css_path) in css_paths.into_iter().enumerate() {
        let expected_prefix = format!("{:02}-", index + 1);
        let file_name = css_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("fixture names should be UTF-8");
        assert!(
            file_name.starts_with(&expected_prefix),
            "fixture sequence is incomplete at {expected_prefix}: {file_name}"
        );

        let expected_path = css_path.with_extension("dump");
        let source = fs::read_to_string(&css_path).expect("CSS fixture should be readable");
        let expected = fs::read_to_string(&expected_path).expect("dump fixture should be readable");
        let actual = meow_css::parse_stylesheet(&source).dump();
        assert_eq!(actual, expected, "fixture {} changed", css_path.display());
    }
}

fn fixture_paths(directory: &Path, extension: &str) -> Vec<std::path::PathBuf> {
    fs::read_dir(directory)
        .expect("fixture directory should exist")
        .map(|entry| entry.expect("fixture entry should be readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|candidate| candidate == extension)
        })
        .collect()
}

fn fixture_number(path: &Path) -> usize {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.split_once('-'))
        .and_then(|(number, _)| number.parse().ok())
        .expect("fixture names should start with a numeric sequence")
}

fn stems(paths: &[std::path::PathBuf]) -> BTreeSet<String> {
    paths
        .iter()
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .expect("fixture stems should be UTF-8")
                .to_owned()
        })
        .collect()
}
