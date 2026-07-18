use std::{fmt::Write as _, fs, path::Path};

use meow_css::parse_stylesheet;
use meow_display_list::Viewport;
use meow_engine::{
    CascadeOrigin, CascadeStylesheet, LayoutViewport, build_box_tree, build_layout_display_list,
    compute_styles, layout_normal_flow,
};
use meow_html::parse_utf8;
use meow_renderer::{ReferenceRenderer, Renderer};

const EXPECTED_CASES: usize = 500;
const WIDTHS: [u32; 5] = [18, 26, 34, 42, 50];
const HEIGHTS: [u32; 5] = [12, 20, 28, 36, 44];
const PADDINGS: [u32; 5] = [0, 2, 4, 6, 8];
const BORDERS: [u32; 4] = [0, 1, 3, 5];
const BACKGROUNDS: [&str; 5] = ["red", "lime", "blue", "gold", "rebeccapurple"];
const FOREGROUNDS: [&str; 5] = ["navy", "maroon", "teal", "black", "crimson"];

#[test]
fn five_hundred_visual_fixtures_match_pixel_hashes() {
    let viewport = Viewport::new(96, 96).unwrap();
    let mut actual = String::new();
    let mut case_index = 0;

    for (width_index, width) in WIDTHS.into_iter().enumerate() {
        for (height_index, height) in HEIGHTS.into_iter().enumerate() {
            for (padding_index, padding) in PADDINGS.into_iter().enumerate() {
                for (border_index, border) in BORDERS.into_iter().enumerate() {
                    let background = BACKGROUNDS[(width_index + height_index) % BACKGROUNDS.len()];
                    let foreground =
                        FOREGROUNDS[(padding_index + border_index) % FOREGROUNDS.len()];
                    let css = format!(
                        "html, body, main {{ display:block; margin:0; padding:0; border-width:0; }} \
                         #target {{ width:{width}px; height:{height}px; margin:3px; padding:{padding}px; \
                         border-width:{border}px; background-color:{background}; color:{foreground}; }}"
                    );
                    let document = parse_utf8(b"<main id='target'></main>").document;
                    let stylesheet = parse_stylesheet(&css);
                    let styles = compute_styles(
                        &document,
                        &[CascadeStylesheet::new(CascadeOrigin::Author, &stylesheet)],
                    );
                    let boxes = build_box_tree(&document, &styles);
                    let layout = layout_normal_flow(
                        &boxes,
                        &styles,
                        LayoutViewport::new(viewport.width, viewport.height),
                    );
                    let list = build_layout_display_list(&layout, &styles, viewport).unwrap();
                    let frame = ReferenceRenderer::new().render(viewport, &list).unwrap();
                    let hash = fnv1a64(frame.premultiplied_rgba());
                    writeln!(
                        actual,
                        "case={case_index:03} width={width} height={height} padding={padding} border={border} background={background} foreground={foreground} hash={hash:016x}"
                    )
                    .unwrap();
                    case_index += 1;
                }
            }
        }
    }

    assert_eq!(case_index, EXPECTED_CASES);
    assert_eq!(actual.lines().count(), EXPECTED_CASES);
    let expected_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/visual-500.hashes");
    if std::env::var_os("UPDATE_W16_VISUALS").is_some() {
        fs::write(&expected_path, &actual).expect("visual fixture hashes should update");
    }
    let expected = fs::read_to_string(&expected_path).expect("visual hash fixture should exist");
    assert_eq!(actual, expected);
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
