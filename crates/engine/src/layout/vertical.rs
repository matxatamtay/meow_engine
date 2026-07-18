use std::collections::BTreeMap;

use meow_css::{BoxSizingValue, ComputedValue, LengthOrAuto, LengthOrNone, PropertyId};

use crate::{BoxId, BoxKind, BoxTree, ComputedStyle, ComputedStyleSnapshot};

use super::{
    horizontal::{layout_box_tree, resolve_length},
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
        styles,
        CssPx(0),
        Some(viewport.height),
        inline_heights,
    );
    tree
}

fn flow_siblings(
    siblings: &mut [LayoutBox],
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

    let auto_extent = if node.children.is_empty() {
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
            .unwrap_or_else(|| flow_inline_children(node, styles, specified, inline_heights))
    } else {
        let end = flow_siblings(
            &mut node.children,
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
    styles: &ComputedStyleSnapshot,
    containing_height: Option<CssPx>,
    inline_heights: &BTreeMap<BoxId, CssPx>,
) -> CssPx {
    let mut height = CssPx(0);
    for child in &mut node.children {
        flow_box(
            child,
            styles,
            node.content.y,
            containing_height,
            inline_heights,
        );
        height = height.max(child.border_box_height());
    }
    height
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
