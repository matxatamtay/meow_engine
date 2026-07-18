use std::{fs, path::PathBuf};

use super::*;

#[test]
fn requested_family_style_and_weight_are_scored_before_fallback() {
    let mut database = FontDatabase::deterministic();
    let request = FontRequest {
        families: vec!["Meow Sans".to_owned()],
        weight: 700,
        slant: FontSlant::Normal,
        locale: Some("vi-VN".to_owned()),
    };
    let face = database.select_face(&request, 'ệ');
    let face = database.face(face).unwrap();
    assert_eq!(face.family, "Meow Sans");
    assert_eq!(face.weight, 700);
}

#[test]
fn invalid_opentype_bytes_are_rejected_by_skrifa() {
    let mut database = FontDatabase::default();
    let result = database.register_font_bytes(
        b"not a font",
        "Broken",
        400,
        FontSlant::Normal,
        vec![Script::Latin],
        FontCoverage::latin_basic(),
    );
    assert!(result.is_err());
}

#[test]
fn discovery_is_sorted_filtered_and_non_mutating() {
    let root = std::env::temp_dir().join(format!("meow-font-discovery-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("z.ttf"), []).unwrap();
    fs::write(root.join("nested/a.otf"), []).unwrap();
    fs::write(root.join("skip.txt"), []).unwrap();
    let paths = FontDatabase::discover_system_paths(&[PathBuf::from(&root)]);
    assert_eq!(paths, vec![root.join("nested/a.otf"), root.join("z.ttf")]);
    fs::remove_dir_all(root).unwrap();
}
