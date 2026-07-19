use meow_css::parse_stylesheet;
use meow_display_list::Viewport;
use meow_engine::{
    CascadeOrigin, CascadeStylesheet, FontDatabase, LayoutViewport, build_box_tree,
    build_fragment_display_list, compute_styles, layout_fragment_tree,
};
use meow_html::parse_utf8;
use meow_renderer::{ReferenceRenderer, Renderer};

#[test]
fn translated_opacity_group_matches_reference_pixels() {
    let frame = render(
        "<main><section></section></main>",
        "html,body,main,section{display:block;margin:0}main{width:40px;height:20px}section{width:8px;height:8px;background-color:red;opacity:.5;transform:translate(20px,5px)}",
        40,
        40,
    );
    assert_eq!(pixel(&frame, 2, 2), [255, 255, 255, 255]);
    let (min_x, min_y, max_x, max_y, pink_pixels) = colored_bounds(&frame, |pixel| {
        pixel[0] > 240 && (100..=170).contains(&pixel[1]) && (100..=170).contains(&pixel[2])
    })
    .expect("translated opacity group should paint pink pixels");
    assert!(
        (45..=80).contains(&pink_pixels),
        "pink_pixels={pink_pixels}"
    );
    assert!(
        min_x >= 18 && min_y >= 18,
        "bounds=({min_x},{min_y})-({max_x},{max_y})"
    );
    assert!(max_x > min_x && max_y > min_y);
}

#[test]
fn rotated_rectangle_moves_around_its_border_box_center() {
    let frame = render(
        "<main><section></section></main>",
        "html,body,main,section{display:block;margin:0}main{width:30px;height:30px}section{width:4px;height:10px;background-color:blue;transform:translate(10px,8px) rotate(90deg)}",
        30,
        30,
    );
    let (min_x, min_y, max_x, max_y, blue_pixels) = colored_bounds(&frame, |pixel| {
        pixel[2] > 200 && pixel[0] < 60 && pixel[1] < 60
    })
    .expect("rotated rectangle should paint blue pixels");
    assert!(
        (24..=48).contains(&blue_pixels),
        "blue_pixels={blue_pixels}"
    );
    assert!(
        max_x - min_x > max_y - min_y,
        "bounds=({min_x},{min_y})-({max_x},{max_y})"
    );
    assert_eq!(pixel(&frame, 0, 0), [255, 255, 255, 255]);
}

fn render(html: &str, css: &str, width: u32, height: u32) -> meow_renderer::Framebuffer {
    let document = parse_utf8(html.as_bytes()).document;
    let stylesheet = parse_stylesheet(css);
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &stylesheet)],
    );
    let boxes = build_box_tree(&document, &styles);
    let mut fonts = FontDatabase::deterministic();
    let output = layout_fragment_tree(
        &boxes,
        &styles,
        LayoutViewport::new(width, height),
        &mut fonts,
    );
    let viewport = Viewport::new(width, height).unwrap();
    let list =
        build_fragment_display_list(&output.layout, &styles, &output.fragments, viewport).unwrap();
    ReferenceRenderer::new().render(viewport, &list).unwrap()
}

fn colored_bounds(
    frame: &meow_renderer::Framebuffer,
    predicate: impl Fn(&[u8]) -> bool,
) -> Option<(u32, u32, u32, u32, usize)> {
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut count = 0;
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let offset = ((y * frame.width() + x) * 4) as usize;
            let pixel = &frame.premultiplied_rgba()[offset..offset + 4];
            if predicate(pixel) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                count += 1;
            }
        }
    }
    (count > 0).then_some((min_x, min_y, max_x, max_y, count))
}

fn pixel(frame: &meow_renderer::Framebuffer, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.premultiplied_rgba()[offset..offset + 4]
        .try_into()
        .unwrap()
}
