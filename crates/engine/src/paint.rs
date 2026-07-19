//! W16 background and border painting for resolved layout trees.

use meow_css::{
    CSS_NUMBER_SCALE, ColorValue, ComputedValue, Length, LengthUnit, NamedColor, PropertyId,
    TransformList, TransformOperation,
};
use meow_display_list::{Affine2D, DisplayList, DisplayListError, Rectangle, Rgba8, Viewport};

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

pub(crate) fn paint_box(
    node: &LayoutBox,
    styles: &ComputedStyleSnapshot,
    viewport: Viewport,
    list: &mut DisplayList,
) -> Result<(), DisplayListError> {
    paint_box_at(node, styles, viewport, list, CssPx(0), CssPx(0))
}

pub(crate) fn paint_box_at(
    node: &LayoutBox,
    styles: &ComputedStyleSnapshot,
    viewport: Viewport,
    list: &mut DisplayList,
    offset_x: CssPx,
    offset_y: CssPx,
) -> Result<(), DisplayListError> {
    let pushed = begin_stacking_context(node, styles, list, offset_x, offset_y);
    paint_box_self_at(node, styles, viewport, list, offset_x, offset_y)?;
    for child in &node.children {
        paint_box_at(child, styles, viewport, list, offset_x, offset_y)?;
    }
    if pushed {
        list.pop_layer()?;
    }
    Ok(())
}

pub(crate) fn paint_box_self_at(
    node: &LayoutBox,
    styles: &ComputedStyleSnapshot,
    viewport: Viewport,
    list: &mut DisplayList,
    offset_x: CssPx,
    offset_y: CssPx,
) -> Result<(), DisplayListError> {
    if let Some(style) = node.source.and_then(|source| styles.style_for(source)) {
        let border_box = translated(node.border_box_rect(), offset_x, offset_y);
        let background = color_property(style.typed(PropertyId::BackgroundColor));
        if background.alpha() != 0 {
            fill_signed(list, border_box, background, viewport)?;
        }
        let border_color = color_property(style.typed(PropertyId::Color));
        if border_color.alpha() != 0 {
            paint_borders(list, node, border_color, viewport, offset_x, offset_y)?;
        }
    }
    Ok(())
}

pub(crate) fn begin_stacking_context(
    node: &LayoutBox,
    styles: &ComputedStyleSnapshot,
    list: &mut DisplayList,
    offset_x: CssPx,
    offset_y: CssPx,
) -> bool {
    let Some(style) = node.source.and_then(|source| styles.style_for(source)) else {
        return false;
    };
    let opacity = opacity_value(style.typed(PropertyId::Opacity));
    let transform = transform_value(
        style.typed(PropertyId::Transform),
        translated(node.border_box_rect(), offset_x, offset_y),
    );
    if opacity == u16::MAX && transform == Affine2D::IDENTITY {
        return false;
    }
    list.push_layer(transform, opacity);
    true
}

fn paint_borders(
    list: &mut DisplayList,
    node: &LayoutBox,
    color: Rgba8,
    viewport: Viewport,
    offset_x: CssPx,
    offset_y: CssPx,
) -> Result<(), DisplayListError> {
    let rect = translated(node.border_box_rect(), offset_x, offset_y);
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

pub(crate) const fn translated(rect: LayoutRect, x: CssPx, y: CssPx) -> LayoutRect {
    LayoutRect {
        x: CssPx(rect.x.0 + x.0),
        y: CssPx(rect.y.0 + y.0),
        ..rect
    }
}

pub(crate) fn fill_signed(
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

fn opacity_value(value: &ComputedValue) -> u16 {
    let ComputedValue::Number(number) = value else {
        unreachable!("opacity has a number value");
    };
    ((number.scaled().clamp(0, CSS_NUMBER_SCALE) as u128 * u128::from(u16::MAX))
        / CSS_NUMBER_SCALE as u128) as u16
}

fn transform_value(value: &ComputedValue, border_box: LayoutRect) -> Affine2D {
    let ComputedValue::Transform(TransformList(operations)) = value else {
        unreachable!("transform has a transform-list value");
    };
    if operations.is_empty() {
        return Affine2D::IDENTITY;
    }
    let mut transform = Affine2D::IDENTITY;
    for operation in operations {
        let operation = match operation {
            TransformOperation::Translate { x, y } => Affine2D::translation(
                transform_length(*x, border_box.width),
                transform_length(*y, border_box.height),
            ),
            TransformOperation::Scale { x, y } => Affine2D::scale(
                x.scaled() as f64 / CSS_NUMBER_SCALE as f64,
                y.scaled() as f64 / CSS_NUMBER_SCALE as f64,
            ),
            TransformOperation::Rotate { degrees } => {
                Affine2D::rotation_degrees(degrees.scaled() as f64 / CSS_NUMBER_SCALE as f64)
            }
            TransformOperation::Matrix { a, b, c, d, e, f } => Affine2D::from_f64(
                a.scaled() as f64 / CSS_NUMBER_SCALE as f64,
                b.scaled() as f64 / CSS_NUMBER_SCALE as f64,
                c.scaled() as f64 / CSS_NUMBER_SCALE as f64,
                d.scaled() as f64 / CSS_NUMBER_SCALE as f64,
                e.scaled() as f64 / CSS_NUMBER_SCALE as f64,
                f.scaled() as f64 / CSS_NUMBER_SCALE as f64,
            ),
        };
        transform = transform.multiply(operation);
    }
    let center_x = border_box.x.0.saturating_add(border_box.width.0 / 2);
    let center_y = border_box.y.0.saturating_add(border_box.height.0 / 2);
    Affine2D::translation(center_x, center_y)
        .multiply(transform)
        .multiply(Affine2D::translation(-center_x, -center_y))
}

fn transform_length(length: Length, basis: CssPx) -> i32 {
    let scaled = match length.unit {
        LengthUnit::Px => length.number.scaled(),
        LengthUnit::Percent => length.number.scaled().saturating_mul(i64::from(basis.0)) / 100,
        LengthUnit::Em | LengthUnit::Rem => length.number.scaled().saturating_mul(16),
    };
    (scaled / CSS_NUMBER_SCALE).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(crate) fn color_property(value: &ComputedValue) -> Rgba8 {
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
