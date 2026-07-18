use meow_css::parse_stylesheet;
use meow_display_list::{DisplayCommand, Rgba8, Viewport};
use meow_engine::{
    CascadeOrigin, CascadeStylesheet, LayoutViewport, build_box_tree, build_layout_display_list,
    compute_styles, layout_normal_flow,
};
use meow_html::parse_utf8;

#[test]
fn backgrounds_borders_clips_and_child_stacking_have_stable_order() {
    let document = parse_utf8(b"<main id='parent'><section id='child'></section></main>").document;
    let css = parse_stylesheet(
        "html, body, main, section { display:block; margin:0; padding:0 } \
         #parent { width:40px; height:30px; padding:2px; border-width:3px; background-color:red; color:blue } \
         #child { width:10px; height:10px; background-color:lime }",
    );
    let styles = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &css)],
    );
    let boxes = build_box_tree(&document, &styles);
    let layout = layout_normal_flow(&boxes, &styles, LayoutViewport::new(80, 60));
    let list = build_layout_display_list(&layout, &styles, Viewport::new(80, 60).unwrap()).unwrap();

    assert!(matches!(
        list.commands().first(),
        Some(DisplayCommand::Clear(_))
    ));
    assert!(matches!(
        list.commands().get(1),
        Some(DisplayCommand::PushClip(_))
    ));
    assert!(matches!(
        list.commands().last(),
        Some(DisplayCommand::PopClip)
    ));
    let fills = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::FillRectangle { color, .. } => Some(*color),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fills[0], Rgba8::rgb(255, 0, 0));
    assert_eq!(fills[1..5], [Rgba8::rgb(0, 0, 255); 4]);
    assert_eq!(fills[5], Rgba8::rgb(0, 255, 0));
}
