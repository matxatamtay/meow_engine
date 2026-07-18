use std::{fs, path::Path};

use meow_engine::{FontDatabase, FontRequest, FontSlant};

#[test]
fn latin_vietnamese_and_arabic_fallback_fixtures_are_stable() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/font-fallback.tsv");
    let contents = fs::read_to_string(fixture).unwrap();
    for (line_index, line) in contents.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let [families, weight, slant, text, expected] = line
            .split('\t')
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_else(|_| panic!("invalid fixture line {}", line_index + 1));
        let mut database = FontDatabase::deterministic();
        let request = FontRequest {
            families: families.split(',').map(str::to_owned).collect(),
            weight: weight.parse().unwrap(),
            slant: match slant {
                "normal" => FontSlant::Normal,
                "italic" => FontSlant::Italic,
                _ => panic!("invalid slant"),
            },
            locale: Some("vi-VN".to_owned()),
        };
        let actual = database
            .resolve_text(&request, text)
            .into_iter()
            .map(|span| span.family)
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(actual, expected, "fixture line {}", line_index + 1);
    }
}
