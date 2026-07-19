use std::collections::BTreeMap;

use meow_css::{
    BoxSizingValue, CSS_NUMBER_SCALE, ComputedValue, LengthOrAuto, LengthOrNone, PropertyId,
};

use crate::{BoxId, BoxKind, BoxTree, ComputedStyle, ComputedStyleSnapshot};

use super::{
    horizontal::{layout_box_tree, relayout_children_horizontal, resolve_length},
    model::{CssPx, LayoutBox, LayoutTree, LayoutViewport},
};

const TEXT_LINE_HEIGHT: CssPx = CssPx(16);

/// Runs W15 vertical normal flow after W14 horizontal resolution.
#[must_use]
pub fn layout_normal_flow(
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    viewport: LayoutViewport,
) -> LayoutTree {
    layout_normal_flow_with_inline_heights(boxes, styles, viewport, &BTreeMap::new())
}

/// Runs normal flow with measured inline formatting heights supplied by W20.
#[must_use]
pub fn layout_normal_flow_with_inline_heights(
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    viewport: LayoutViewport,
    inline_heights: &BTreeMap<BoxId, CssPx>,
) -> LayoutTree {
    let mut tree = layout_box_tree(boxes, styles, viewport);
    flow_siblings(
        tree.roots_mut(),
        boxes,
        styles,
        CssPx(0),
        Some(viewport.height),
        inline_heights,
    );
    tree
}

fn flow_siblings(
    siblings: &mut [LayoutBox],
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    start_y: CssPx,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
) -> CssPx {
    let mut cursor = start_y;
    let mut previous_bottom = None;
    for sibling in siblings {
        let spacing = previous_bottom.map_or(sibling.margin.top, |bottom| {
            collapse_margins(bottom, sibling.margin.top)
        });
        let border_top = CssPx(cursor.0 + spacing.0);
        flow_box(
            sibling,
            boxes,
            styles,
            border_top,
            containing_height,
            inline_heights,
        );
        let rect = sibling.border_box_rect();
        cursor = CssPx(rect.y.0 + rect.height.0);
        previous_bottom = Some(sibling.margin.bottom);
    }
    CssPx(cursor.0 + previous_bottom.unwrap_or_default().0)
}

fn flow_box(
    node: &mut LayoutBox,
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    border_top: CssPx,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
) {
    if node.kind == BoxKind::TextRun {
        node.content.y = border_top;
        node.content.height = TEXT_LINE_HEIGHT;
        node.overflow.scroll_height = TEXT_LINE_HEIGHT;
        return;
    }

    node.content.y = CssPx(border_top.0 + node.border.top.0 + node.padding.top.0);
    let style = node.source.and_then(|source| styles.style_for(source));
    let vertical_non_content =
        node.padding.top.0 + node.padding.bottom.0 + node.border.top.0 + node.border.bottom.0;
    let specified = style
        .and_then(|style| specified_content_height(style, containing_height, vertical_non_content));

    let auto_extent = if node.kind == BoxKind::PrincipalFlex {
        flow_flex_container(node, boxes, styles, specified, inline_heights)
    } else if node.kind == BoxKind::ReplacedImage {
        replaced_image_height(node, boxes, styles)
    } else if node.children.is_empty() {
        if node.kind == BoxKind::PrincipalInline {
            TEXT_LINE_HEIGHT
        } else {
            CssPx(0)
        }
    } else if node
        .children
        .iter()
        .all(|child| child.kind.is_inline_level())
    {
        inline_heights
            .get(&node.box_id)
            .copied()
            .unwrap_or_else(|| flow_inline_children(node, boxes, styles, specified, inline_heights))
    } else {
        let end = flow_siblings(
            &mut node.children,
            boxes,
            styles,
            node.content.y,
            specified,
            inline_heights,
        );
        CssPx(end.0 - node.content.y.0).max_zero()
    };

    let content_height = style.map_or(auto_extent, |style| {
        resolve_used_height(
            style,
            auto_extent,
            containing_height,
            vertical_non_content,
            specified,
        )
    });
    node.content.height = content_height;
    update_overflow(node, auto_extent);
}

fn flow_inline_children(
    node: &mut LayoutBox,
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
) -> CssPx {
    let mut height = CssPx(0);
    for child in &mut node.children {
        flow_box(
            child,
            boxes,
            styles,
            node.content.y,
            containing_height,
            inline_heights,
        );
        height = height.max(child.border_box_height());
    }
    height
}

fn flow_flex_container(
    node: &mut LayoutBox,
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
) -> CssPx {
    let Some(style) = node.source.and_then(|source| styles.style_for(source)) else {
        return CssPx(0);
    };
    let direction = style.get(PropertyId::FlexDirection);
    if matches!(direction, "column" | "column-reverse") {
        flow_flex_column(
            node,
            boxes,
            styles,
            containing_height,
            inline_heights,
            direction == "column-reverse",
        )
    } else {
        flow_flex_row(
            node,
            boxes,
            styles,
            containing_height,
            inline_heights,
            direction == "row-reverse",
        )
    }
}

fn flow_flex_row(
    node: &mut LayoutBox,
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
    reverse: bool,
) -> CssPx {
    let Some(container_style) = node.source.and_then(|source| styles.style_for(source)) else {
        return CssPx(0);
    };
    let gap = length_value(container_style, PropertyId::Gap, node.content.width).max_zero();
    let item_count = node.children.len();
    if item_count == 0 {
        return CssPx(0);
    }
    let gaps = gap.0.saturating_mul(item_count.saturating_sub(1) as i32);
    let mut bases = Vec::with_capacity(item_count);
    let mut grows = Vec::with_capacity(item_count);
    let mut shrink_weights = Vec::with_capacity(item_count);
    for child in &node.children {
        let box_node = boxes.box_by_id(child.box_id);
        let style = child.source.and_then(|source| styles.style_for(source));
        let base = flex_basis(style, box_node, node.content.width, true).max_zero();
        bases.push(base);
        let grow = number_value(style, PropertyId::FlexGrow, 0);
        let shrink = number_value(style, PropertyId::FlexShrink, CSS_NUMBER_SCALE);
        grows.push(grow);
        shrink_weights.push(shrink.saturating_mul(i64::from(base.0.max(0))));
    }
    let available = node.content.width.0.saturating_sub(gaps);
    let base_total = bases.iter().map(|value| value.0).sum::<i32>();
    let free = available.saturating_sub(base_total);
    let final_sizes = distribute_flex_space(&bases, &grows, &shrink_weights, free);

    for (index, child) in node.children.iter_mut().enumerate() {
        set_border_box_width(child, final_sizes[index]);
        if let Some(box_node) = boxes.box_by_id(child.box_id) {
            relayout_children_horizontal(child, box_node, styles);
        }
        flow_box(
            child,
            boxes,
            styles,
            node.content.y,
            containing_height,
            inline_heights,
        );
    }
    let cross_size = CssPx(
        node.children
            .iter()
            .map(outer_height)
            .max()
            .unwrap_or_default(),
    )
    .max_zero();
    align_flex_row_items(node, container_style, cross_size);

    let used = node.children.iter().map(outer_width).sum::<i32>() + gaps;
    let remaining = node.content.width.0.saturating_sub(used).max(0);
    let (mut cursor, extra_gap) = justify_offsets(
        container_style.get(PropertyId::JustifyContent),
        remaining,
        item_count,
    );
    let order = if reverse {
        (0..item_count).rev().collect::<Vec<_>>()
    } else {
        (0..item_count).collect::<Vec<_>>()
    };
    for index in order {
        let child = &mut node.children[index];
        let border_x = node.content.x.0 + cursor + child.margin.left.0;
        translate_layout(child, border_x - child.border_box_rect().x.0, 0);
        cursor = cursor
            .saturating_add(outer_width(child))
            .saturating_add(gap.0)
            .saturating_add(extra_gap);
    }
    cross_size
}

fn flow_flex_column(
    node: &mut LayoutBox,
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
    reverse: bool,
) -> CssPx {
    let Some(container_style) = node.source.and_then(|source| styles.style_for(source)) else {
        return CssPx(0);
    };
    let gap = length_value(container_style, PropertyId::Gap, node.content.width).max_zero();
    for child in &mut node.children {
        flow_box(
            child,
            boxes,
            styles,
            node.content.y,
            containing_height,
            inline_heights,
        );
    }
    let count = node.children.len();
    let total = node.children.iter().map(outer_height).sum::<i32>()
        + gap.0.saturating_mul(count.saturating_sub(1) as i32);
    let available = containing_height.map_or(total, |height| height.0.max(total));
    let remaining = available.saturating_sub(total);
    let (mut cursor, extra_gap) = justify_offsets(
        container_style.get(PropertyId::JustifyContent),
        remaining,
        count,
    );
    let order = if reverse {
        (0..count).rev().collect::<Vec<_>>()
    } else {
        (0..count).collect::<Vec<_>>()
    };
    for index in order {
        let child = &mut node.children[index];
        let border_y = node.content.y.0 + cursor + child.margin.top.0;
        translate_layout(child, 0, border_y - child.border_box_rect().y.0);
        align_flex_column_item(child, container_style, node.content.width);
        cursor = cursor
            .saturating_add(outer_height(child))
            .saturating_add(gap.0)
            .saturating_add(extra_gap);
    }
    CssPx(total)
}

fn distribute_flex_space(
    bases: &[CssPx],
    grows: &[i64],
    shrink_weights: &[i64],
    free: i32,
) -> Vec<CssPx> {
    let weights = if free >= 0 { grows } else { shrink_weights };
    let total = weights.iter().copied().sum::<i64>();
    if total <= 0 || free == 0 {
        return bases.to_vec();
    }
    let magnitude = i64::from(free.unsigned_abs());
    let mut remaining = magnitude;
    let mut output = Vec::with_capacity(bases.len());
    for (index, (base, weight)) in bases.iter().zip(weights).enumerate() {
        let share = if index + 1 == bases.len() {
            remaining
        } else {
            magnitude.saturating_mul(*weight) / total
        };
        remaining = remaining.saturating_sub(share);
        let signed = if free >= 0 { share } else { -share };
        output.push(CssPx(
            i64::from(base.0)
                .saturating_add(signed)
                .clamp(0, i64::from(i32::MAX)) as i32,
        ));
    }
    output
}

fn flex_basis(
    style: Option<&ComputedStyle>,
    node: Option<&crate::BoxNode>,
    basis: CssPx,
    horizontal: bool,
) -> CssPx {
    if let Some(style) = style {
        match style.typed(PropertyId::FlexBasis) {
            ComputedValue::LengthOrAuto(LengthOrAuto::Length(length)) => {
                return resolve_length(*length, basis);
            }
            ComputedValue::LengthOrAuto(LengthOrAuto::Auto) => {}
            _ => unreachable!("flex-basis has a length-or-auto value"),
        }
        let property = if horizontal {
            PropertyId::Width
        } else {
            PropertyId::Height
        };
        if let ComputedValue::LengthOrAuto(LengthOrAuto::Length(length)) = style.typed(property) {
            return resolve_length(*length, basis);
        }
    }
    node.map_or(CssPx(0), |node| intrinsic_main_size(node, horizontal))
}

fn intrinsic_main_size(node: &crate::BoxNode, horizontal: bool) -> CssPx {
    if horizontal {
        if let Some(text) = &node.text {
            return CssPx(
                text.chars()
                    .count()
                    .saturating_mul(8)
                    .min(i32::MAX as usize) as i32,
            );
        }
        CssPx(
            node.children
                .iter()
                .map(|child| intrinsic_main_size(child, true).0)
                .sum::<i32>(),
        )
    } else if node.kind == BoxKind::TextRun {
        CssPx(16)
    } else {
        CssPx(
            node.children
                .iter()
                .map(|child| intrinsic_main_size(child, false).0)
                .sum::<i32>(),
        )
    }
}

fn set_border_box_width(node: &mut LayoutBox, width: CssPx) {
    let non_content =
        node.padding.left.0 + node.padding.right.0 + node.border.left.0 + node.border.right.0;
    node.content.width = CssPx(width.0.saturating_sub(non_content)).max_zero();
}

fn align_flex_row_items(node: &mut LayoutBox, style: &ComputedStyle, cross_size: CssPx) {
    let align = style.get(PropertyId::AlignItems);
    for child in &mut node.children {
        let free = cross_size.0.saturating_sub(outer_height(child)).max(0);
        let offset = match align {
            "flex-end" => free,
            "center" => free / 2,
            _ => 0,
        };
        let border_y = node.content.y.0 + offset + child.margin.top.0;
        translate_layout(child, 0, border_y - child.border_box_rect().y.0);
    }
}

fn align_flex_column_item(child: &mut LayoutBox, style: &ComputedStyle, cross_size: CssPx) {
    let free = cross_size.0.saturating_sub(outer_width(child)).max(0);
    let offset = match style.get(PropertyId::AlignItems) {
        "flex-end" => free,
        "center" => free / 2,
        _ => 0,
    };
    let desired = child.containing_block_width.0.saturating_sub(free) + offset;
    let delta = desired.saturating_sub(child.border_box_rect().x.0);
    translate_layout(child, delta, 0);
}

fn justify_offsets(value: &str, remaining: i32, count: usize) -> (i32, i32) {
    if count == 0 {
        return (0, 0);
    }
    match value {
        "flex-end" => (remaining, 0),
        "center" => (remaining / 2, 0),
        "space-between" if count > 1 => (0, remaining / (count as i32 - 1)),
        "space-around" => {
            let spacing = remaining / count as i32;
            (spacing / 2, spacing)
        }
        "space-evenly" => {
            let spacing = remaining / (count as i32 + 1);
            (spacing, spacing)
        }
        _ => (0, 0),
    }
}

fn number_value(style: Option<&ComputedStyle>, property: PropertyId, fallback: i64) -> i64 {
    style.map_or(fallback, |style| match style.typed(property) {
        ComputedValue::Number(value) => value.scaled(),
        _ => unreachable!("flex factor has a number value"),
    })
}

fn length_value(style: &ComputedStyle, property: PropertyId, basis: CssPx) -> CssPx {
    match style.typed(property) {
        ComputedValue::Length(length) => resolve_length(*length, basis),
        _ => unreachable!("property has a length value"),
    }
}

fn outer_width(node: &LayoutBox) -> i32 {
    node.margin.left.0 + node.border_box_width().0 + node.margin.right.0
}

fn outer_height(node: &LayoutBox) -> i32 {
    node.margin.top.0 + node.border_box_height().0 + node.margin.bottom.0
}

fn translate_layout(node: &mut LayoutBox, delta_x: i32, delta_y: i32) {
    node.content.x = CssPx(node.content.x.0.saturating_add(delta_x));
    node.content.y = CssPx(node.content.y.0.saturating_add(delta_y));
    for child in &mut node.children {
        translate_layout(child, delta_x, delta_y);
    }
}

fn replaced_image_height(
    node: &LayoutBox,
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
) -> CssPx {
    let style = node.source.and_then(|source| styles.style_for(source));
    if let Some(style) = style
        && let ComputedValue::LengthOrAuto(LengthOrAuto::Length(length)) =
            style.typed(PropertyId::Height)
    {
        return resolve_length(*length, node.content.width).max_zero();
    }
    let Some((width, height)) = boxes
        .box_by_id(node.box_id)
        .and_then(|box_node| box_node.intrinsic_size)
    else {
        return CssPx(0);
    };
    if width == 0 {
        return CssPx(0);
    }
    let used_width = i64::from(node.content.width.0.max(0));
    CssPx(
        used_width
            .saturating_mul(i64::from(height))
            .checked_div(i64::from(width))
            .unwrap_or(0)
            .clamp(0, i64::from(i32::MAX)) as i32,
    )
}

fn specified_content_height(
    style: &ComputedStyle,
    containing_height: Option<CssPx>,
    non_content: i32,
) -> Option<CssPx> {
    let specified = match style.typed(PropertyId::Height) {
        ComputedValue::LengthOrAuto(LengthOrAuto::Auto) => return None,
        ComputedValue::LengthOrAuto(LengthOrAuto::Length(length)) => {
            if length.unit == meow_css::LengthUnit::Percent {
                resolve_length(*length, containing_height?)
            } else {
                resolve_length(*length, CssPx(0))
            }
        }
        _ => unreachable!("height has a length-or-auto typed value"),
    };
    Some(match style.typed(PropertyId::BoxSizing) {
        ComputedValue::BoxSizing(BoxSizingValue::ContentBox) => specified.max_zero(),
        ComputedValue::BoxSizing(BoxSizingValue::BorderBox) => {
            CssPx(specified.0 - non_content).max_zero()
        }
        _ => unreachable!("box-sizing has a box-sizing typed value"),
    })
}

fn resolve_used_height(
    style: &ComputedStyle,
    auto_extent: CssPx,
    containing_height: Option<CssPx>,
    non_content: i32,
    specified_content: Option<CssPx>,
) -> CssPx {
    let box_sizing = match style.typed(PropertyId::BoxSizing) {
        ComputedValue::BoxSizing(value) => *value,
        _ => unreachable!("box-sizing has a box-sizing typed value"),
    };
    let content = specified_content.unwrap_or(auto_extent);
    let sizing_box = match box_sizing {
        BoxSizingValue::ContentBox => content,
        BoxSizingValue::BorderBox => CssPx(content.0 + non_content),
    };
    let min_height = resolve_constraint_length(style, PropertyId::MinHeight, containing_height)
        .unwrap_or(CssPx(0));
    let max_height = match style.typed(PropertyId::MaxHeight) {
        ComputedValue::LengthOrNone(LengthOrNone::None) => None,
        ComputedValue::LengthOrNone(LengthOrNone::Length(length)) => {
            if length.unit == meow_css::LengthUnit::Percent {
                containing_height.map(|basis| resolve_length(*length, basis))
            } else {
                Some(resolve_length(*length, CssPx(0)))
            }
        }
        _ => unreachable!("max-height has a length-or-none typed value"),
    };
    let constrained = CssPx(
        max_height
            .map_or(sizing_box.0, |maximum| sizing_box.0.min(maximum.0))
            .max(min_height.0),
    );
    match box_sizing {
        BoxSizingValue::ContentBox => constrained.max_zero(),
        BoxSizingValue::BorderBox => CssPx(constrained.0 - non_content).max_zero(),
    }
}

fn resolve_constraint_length(
    style: &ComputedStyle,
    property: PropertyId,
    containing_height: Option<CssPx>,
) -> Option<CssPx> {
    match style.typed(property) {
        ComputedValue::Length(length) if length.unit == meow_css::LengthUnit::Percent => {
            containing_height.map(|basis| resolve_length(*length, basis))
        }
        ComputedValue::Length(length) => Some(resolve_length(*length, CssPx(0))),
        _ => unreachable!("constraint has a length typed value"),
    }
}

fn update_overflow(node: &mut LayoutBox, auto_extent: CssPx) {
    let mut scroll_width = node.content.width;
    let mut scroll_height = auto_extent.max(node.content.height);
    for child in &node.children {
        let rect = child.border_box_rect();
        scroll_width = scroll_width.max(CssPx(rect.x.0 + rect.width.0 - node.content.x.0));
        scroll_height = scroll_height.max(CssPx(rect.y.0 + rect.height.0 - node.content.y.0));
    }
    node.overflow.scroll_width = scroll_width.max_zero();
    node.overflow.scroll_height = scroll_height.max_zero();
    node.overflow.horizontal = node.overflow.scroll_width > node.content.width;
    node.overflow.vertical = node.overflow.scroll_height > node.content.height;
}

/// Collapses two adjacent block sibling margins for the W15 subset.
#[must_use]
pub const fn collapse_margins(first: CssPx, second: CssPx) -> CssPx {
    if first.0 >= 0 && second.0 >= 0 {
        if first.0 >= second.0 { first } else { second }
    } else if first.0 <= 0 && second.0 <= 0 {
        if first.0 <= second.0 { first } else { second }
    } else {
        CssPx(first.0 + second.0)
    }
}
