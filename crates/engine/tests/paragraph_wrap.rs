use std::{fs, path::Path};

use meow_engine::{FontDatabase, FontRequest, TextAlign, layout_paragraph};

const EXPECTED_FIXTURE_COUNT: usize = 6;

#[test]
fn paragraph_wrap_is_deterministic_across_widths_and_alignment() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/paragraph-wrap");
    let mut fixtures = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for fixture in fixtures {
        let text = fs::read_to_string(fixture.join("input.txt")).unwrap();
        let config = fs::read_to_string(fixture.join("config.txt")).unwrap();
        let mut width = None;
        let mut align = None;
        for line in config.lines() {
            let (name, value) = line.split_once('=').unwrap();
            match name {
                "width" => width = Some(value.parse::<i32>().unwrap()),
                "align" => align = Some(parse_align(value)),
                _ => panic!("unknown config key {name}"),
            }
        }
        let mut database = FontDatabase::deterministic();
        let request = FontRequest::new(["Meow Sans", "Meow Arabic"]);
        let actual = layout_paragraph(
            &mut database,
            &request,
            &text,
            width.unwrap(),
            align.unwrap(),
        )
        .dump();
        let expected_path = fixture.join("expected.dump");
        if std::env::var_os("UPDATE_W19_SNAPSHOTS").is_some() {
            fs::write(&expected_path, &actual).unwrap();
        }
        let expected = fs::read_to_string(expected_path).unwrap();
        assert_eq!(actual, expected, "fixture {} changed", fixture.display());
    }
}

fn parse_align(value: &str) -> TextAlign {
    match value.trim() {
        "start" => TextAlign::Start,
        "end" => TextAlign::End,
        "left" => TextAlign::Left,
        "right" => TextAlign::Right,
        "center" => TextAlign::Center,
        "justify" => TextAlign::Justify,
        value => panic!("unknown text alignment {value}"),
    }
}
