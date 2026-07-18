use std::{fs, path::Path};

use meow_css::parse_stylesheet;
use meow_display_list::Viewport;
use meow_engine::{
    CascadeOrigin, CascadeStylesheet, FontDatabase, LayoutViewport, build_box_tree,
    build_fragment_display_list, compute_styles, layout_fragment_tree,
};
use meow_html::parse_utf8;
use meow_renderer::{ReferenceRenderer, Renderer};

const EXPECTED_FIXTURE_COUNT: usize = 3;

#[test]
fn readable_article_visuals_match_pixel_hashes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inline-fragments");
    let mut fixtures = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    let viewport = Viewport::new(320, 360).unwrap();
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
        let output = layout_fragment_tree(
            &boxes,
            &styles,
            LayoutViewport::new(viewport.width, viewport.height),
            &mut fonts,
        );
        let list =
            build_fragment_display_list(&output.layout, &styles, &output.fragments, viewport)
                .unwrap();
        let frame = ReferenceRenderer::new().render(viewport, &list).unwrap();
        let actual = format!("{:016x}\n", fnv1a64(frame.premultiplied_rgba()));
        let expected_path = fixture.join("expected.hash");
        if std::env::var_os("UPDATE_W20_VISUALS").is_some() {
            fs::write(&expected_path, &actual).unwrap();
        }
        if std::env::var_os("WRITE_W20_PREVIEWS").is_some() {
            let name = fixture.file_name().unwrap();
            let output = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/w20-previews")
                .join(name)
                .with_extension("png");
            fs::create_dir_all(output.parent().unwrap()).unwrap();
            fs::write(output, frame.encode_png().unwrap()).unwrap();
        }
        let expected = fs::read_to_string(expected_path).unwrap();
        assert_eq!(actual, expected, "fixture {} changed", fixture.display());
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
