//! W16 background and border painting for resolved layout trees.

use meow_css::{ColorValue, ComputedValue, NamedColor, PropertyId};
use meow_display_list::{DisplayList, DisplayListError, Rectangle, Rgba8, Viewport};

use crate::{ComputedStyleSnapshot, CssPx, LayoutBox, LayoutRect, LayoutTree};

const CANVAS: Rgba8 = Rgba8::rgb(255, 255, 255);

/// Lowers one resolved layout tree into backend-neutral paint commands.
pub fn build_layout_display_list(
    layout: &LayoutTree,
    styles: &ComputedStyleSnapshot,
    viewport: Viewport,
) -> Result<DisplayList, DisplayListError> {
    let mut list = DisplayList::new();
    list.clear(CANVAS);
    list.push_clip(Rectangle::new(0, 0, viewport.width, viewport.height))?;
    for root in layout.roots() {
        paint_box(root, styles, viewport, &mut list)?;
    }
    list.pop_clip()?;
    Ok(list)
}

fn paint_box(
    node: &LayoutBox,
    styles: &ComputedStyleSnapshot,
    viewport: Viewport,
    list: &mut DisplayList,
) -> Result<(), DisplayListError> {
    if let Some(style) = node.source.and_then(|source| styles.style_for(source)) {
        let border_box = node.border_box_rect();
        let background = color_property(style.typed(PropertyId::BackgroundColor));
        if background.alpha() != 0 {
            fill_signed(list, border_box, background, viewport)?;
        }
        let border_color = color_property(style.typed(PropertyId::Color));
        if border_color.alpha() != 0 {
            paint_borders(list, node, border_color, viewport)?;
        }
    }
    for child in &node.children {
        paint_box(child, styles, viewport, list)?;
    }
    Ok(())
}

fn paint_borders(
    list: &mut DisplayList,
    node: &LayoutBox,
    color: Rgba8,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    let rect = node.border_box_rect();
    fill_signed(
        list,
        LayoutRect {
            height: node.border.top,
            ..rect
        },
        color,
        viewport,
    )?;
    fill_signed(
        list,
        LayoutRect {
            x: CssPx(rect.x.0 + rect.width.0 - node.border.right.0),
            width: node.border.right,
            ..rect
        },
        color,
        viewport,
    )?;
    fill_signed(
        list,
        LayoutRect {
            y: CssPx(rect.y.0 + rect.height.0 - node.border.bottom.0),
            height: node.border.bottom,
            ..rect
        },
        color,
        viewport,
    )?;
    fill_signed(
        list,
        LayoutRect {
            width: node.border.left,
            ..rect
        },
        color,
        viewport,
    )
}

fn fill_signed(
    list: &mut DisplayList,
    rect: LayoutRect,
    color: Rgba8,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    let x0 = rect.x.0.max(0).min(viewport.width as i32);
    let y0 = rect.y.0.max(0).min(viewport.height as i32);
    let x1 = rect
        .x
        .0
        .saturating_add(rect.width.0)
        .max(0)
        .min(viewport.width as i32);
    let y1 = rect
        .y
        .0
        .saturating_add(rect.height.0)
        .max(0)
        .min(viewport.height as i32);
    if x1 <= x0 || y1 <= y0 {
        return Ok(());
    }
    list.fill_rectangle(
        Rectangle::new(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32),
        color,
    )
}

fn color_property(value: &ComputedValue) -> Rgba8 {
    let ComputedValue::Color(color) = value else {
        unreachable!("paint color properties have typed colors");
    };
    match color {
        ColorValue::Transparent => Rgba8::new(0, 0, 0, 0),
        ColorValue::CurrentColor => unreachable!("currentColor resolves during computed style"),
        ColorValue::Named(named) => named_color(*named),
        ColorValue::Rgba {
            red,
            green,
            blue,
            alpha,
        } => Rgba8::new(*red, *green, *blue, *alpha),
    }
}

const fn named_color(color: NamedColor) -> Rgba8 {
    match color {
        NamedColor::Black => Rgba8::rgb(0, 0, 0),
        NamedColor::Silver => Rgba8::rgb(192, 192, 192),
        NamedColor::Gray => Rgba8::rgb(128, 128, 128),
        NamedColor::White => Rgba8::rgb(255, 255, 255),
        NamedColor::Maroon => Rgba8::rgb(128, 0, 0),
        NamedColor::Red => Rgba8::rgb(255, 0, 0),
        NamedColor::Purple => Rgba8::rgb(128, 0, 128),
        NamedColor::Fuchsia => Rgba8::rgb(255, 0, 255),
        NamedColor::Green => Rgba8::rgb(0, 128, 0),
        NamedColor::Lime => Rgba8::rgb(0, 255, 0),
        NamedColor::Olive => Rgba8::rgb(128, 128, 0),
        NamedColor::Yellow => Rgba8::rgb(255, 255, 0),
        NamedColor::Navy => Rgba8::rgb(0, 0, 128),
        NamedColor::Blue => Rgba8::rgb(0, 0, 255),
        NamedColor::Teal => Rgba8::rgb(0, 128, 128),
        NamedColor::Aqua => Rgba8::rgb(0, 255, 255),
        NamedColor::Orange => Rgba8::rgb(255, 165, 0),
        NamedColor::Gold => Rgba8::rgb(255, 215, 0),
        NamedColor::Crimson => Rgba8::rgb(220, 20, 60),
        NamedColor::RebeccaPurple => Rgba8::rgb(102, 51, 153),
    }
}
