use meow_css::{
    BorderWidthValue, BoxSizingValue, CSS_NUMBER_SCALE, ComputedValue, Length, LengthOrAuto,
    LengthOrNone, LengthUnit, PropertyId,
};

use crate::{BoxKind, BoxNode, BoxTree, ComputedStyle, ComputedStyleSnapshot};

use super::model::{
    CssPx, EdgeSizes, LayoutBox, LayoutRect, LayoutTree, LayoutViewport, OverflowMetadata,
};

const DEFAULT_FONT_SIZE: i32 = 16;

/// Resolves containing blocks and horizontal box geometry for the W14 subset.
#[must_use]
pub fn layout_box_tree(
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    viewport: LayoutViewport,
) -> LayoutTree {
    let roots = boxes
        .roots()
        .iter()
        .map(|root| layout_box(root, styles, CssPx(0), viewport.width))
        .collect();
    LayoutTree::new(viewport, roots)
}

fn layout_box(
    node: &BoxNode,
    styles: &ComputedStyleSnapshot,
    containing_x: CssPx,
    containing_width: CssPx,
) -> LayoutBox {
    let style = node.source.and_then(|source| styles.style_for(source));
    let horizontal = if node.kind == BoxKind::PrincipalBlock || node.kind == BoxKind::AnonymousBlock
    {
        resolve_horizontal(style, containing_width)
    } else {
        HorizontalUsed::default()
    };
    let content_x = CssPx(
        containing_x.0
            + horizontal.margin_left.0
            + horizontal.border.left.0
            + horizontal.padding.left.0,
    );
    let children = node
        .children
        .iter()
        .map(|child| layout_box(child, styles, content_x, horizontal.content_width))
        .collect();
    LayoutBox {
        box_id: node.id,
        kind: node.kind,
        source: node.source,
        containing_block_width: containing_width,
        content: LayoutRect {
            x: content_x,
            y: CssPx(horizontal.border.top.0 + horizontal.padding.top.0),
            width: horizontal.content_width,
            height: CssPx(0),
        },
        padding: horizontal.padding,
        border: horizontal.border,
        margin: EdgeSizes {
            top: horizontal.margin_top,
            right: horizontal.margin_right,
            bottom: horizontal.margin_bottom,
            left: horizontal.margin_left,
        },
        overflow: OverflowMetadata {
            scroll_width: horizontal.content_width,
            ..OverflowMetadata::default()
        },
        children,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct HorizontalUsed {
    content_width: CssPx,
    margin_top: CssPx,
    margin_right: CssPx,
    margin_bottom: CssPx,
    margin_left: CssPx,
    padding: EdgeSizes,
    border: EdgeSizes,
}

#[derive(Clone, Copy)]
enum AutoLength {
    Auto,
    Length(CssPx),
}

fn resolve_horizontal(style: Option<&ComputedStyle>, containing_width: CssPx) -> HorizontalUsed {
    let Some(style) = style else {
        return HorizontalUsed {
            content_width: containing_width,
            ..HorizontalUsed::default()
        };
    };
    let padding = EdgeSizes {
        top: length_property(style, PropertyId::PaddingTop, containing_width),
        right: length_property(style, PropertyId::PaddingRight, containing_width),
        bottom: length_property(style, PropertyId::PaddingBottom, containing_width),
        left: length_property(style, PropertyId::PaddingLeft, containing_width),
    };
    let border = EdgeSizes {
        top: border_property(style, PropertyId::BorderTopWidth, containing_width),
        right: border_property(style, PropertyId::BorderRightWidth, containing_width),
        bottom: border_property(style, PropertyId::BorderBottomWidth, containing_width),
        left: border_property(style, PropertyId::BorderLeftWidth, containing_width),
    };
    let margin_left = auto_length_property(style, PropertyId::MarginLeft, containing_width);
    let margin_right = auto_length_property(style, PropertyId::MarginRight, containing_width);
    let margin_top = auto_length_property(style, PropertyId::MarginTop, containing_width);
    let margin_bottom = auto_length_property(style, PropertyId::MarginBottom, containing_width);
    let box_sizing = match style.typed(PropertyId::BoxSizing) {
        ComputedValue::BoxSizing(value) => *value,
        _ => unreachable!("box-sizing has a box-sizing typed value"),
    };
    let specified = match style.typed(PropertyId::Width) {
        ComputedValue::LengthOrAuto(LengthOrAuto::Auto) => None,
        ComputedValue::LengthOrAuto(LengthOrAuto::Length(length)) => {
            Some(resolve_length(*length, containing_width))
        }
        _ => unreachable!("width has a length-or-auto typed value"),
    };
    let non_content = padding.left.0 + padding.right.0 + border.left.0 + border.right.0;
    let mut solved = solve_width(
        containing_width,
        specified,
        box_sizing,
        non_content,
        margin_left,
        margin_right,
    );
    let current_sizing_box = match box_sizing {
        BoxSizingValue::ContentBox => solved.content_width,
        BoxSizingValue::BorderBox => CssPx(solved.content_width.0 + non_content),
    };
    let min_width = length_property(style, PropertyId::MinWidth, containing_width);
    let max_width = match style.typed(PropertyId::MaxWidth) {
        ComputedValue::LengthOrNone(LengthOrNone::None) => None,
        ComputedValue::LengthOrNone(LengthOrNone::Length(length)) => {
            Some(resolve_length(*length, containing_width))
        }
        _ => unreachable!("max-width has a length-or-none typed value"),
    };
    let constrained = CssPx(
        max_width
            .map_or(current_sizing_box.0, |maximum| {
                current_sizing_box.0.min(maximum.0)
            })
            .max(min_width.0),
    );
    if constrained != current_sizing_box {
        solved = solve_width(
            containing_width,
            Some(constrained),
            box_sizing,
            non_content,
            margin_left,
            margin_right,
        );
    }
    HorizontalUsed {
        content_width: solved.content_width,
        margin_top: auto_to_zero(margin_top),
        margin_right: solved.margin_right,
        margin_bottom: auto_to_zero(margin_bottom),
        margin_left: solved.margin_left,
        padding,
        border,
    }
}

#[derive(Clone, Copy)]
struct WidthSolution {
    content_width: CssPx,
    margin_left: CssPx,
    margin_right: CssPx,
}

fn solve_width(
    containing_width: CssPx,
    specified_sizing_box: Option<CssPx>,
    box_sizing: BoxSizingValue,
    non_content: i32,
    margin_left: AutoLength,
    margin_right: AutoLength,
) -> WidthSolution {
    let fixed_left = auto_to_zero(margin_left);
    let fixed_right = auto_to_zero(margin_right);
    let content_width = match specified_sizing_box {
        None => CssPx(containing_width.0 - fixed_left.0 - fixed_right.0 - non_content).max_zero(),
        Some(width) => match box_sizing {
            BoxSizingValue::ContentBox => width.max_zero(),
            BoxSizingValue::BorderBox => CssPx(width.0 - non_content).max_zero(),
        },
    };
    if specified_sizing_box.is_none() {
        return WidthSolution {
            content_width,
            margin_left: fixed_left,
            margin_right: fixed_right,
        };
    }
    let remaining =
        containing_width.0 - non_content - content_width.0 - fixed_left.0 - fixed_right.0;
    let (used_left, used_right) = match (margin_left, margin_right) {
        (AutoLength::Auto, AutoLength::Auto) => {
            let left = remaining / 2;
            (CssPx(left), CssPx(remaining - left))
        }
        (AutoLength::Auto, AutoLength::Length(right)) => (CssPx(remaining), right),
        (AutoLength::Length(left), AutoLength::Auto) => (left, CssPx(remaining)),
        (AutoLength::Length(left), AutoLength::Length(right)) => (left, CssPx(right.0 + remaining)),
    };
    WidthSolution {
        content_width,
        margin_left: used_left,
        margin_right: used_right,
    }
}

fn auto_length_property(style: &ComputedStyle, property: PropertyId, basis: CssPx) -> AutoLength {
    match style.typed(property) {
        ComputedValue::LengthOrAuto(LengthOrAuto::Auto) => AutoLength::Auto,
        ComputedValue::LengthOrAuto(LengthOrAuto::Length(length)) => {
            AutoLength::Length(resolve_length(*length, basis))
        }
        _ => unreachable!("margin has a length-or-auto typed value"),
    }
}

fn length_property(style: &ComputedStyle, property: PropertyId, basis: CssPx) -> CssPx {
    match style.typed(property) {
        ComputedValue::Length(length) => resolve_length(*length, basis),
        _ => unreachable!("property has a length typed value"),
    }
}

fn border_property(style: &ComputedStyle, property: PropertyId, basis: CssPx) -> CssPx {
    match style.typed(property) {
        ComputedValue::BorderWidth(BorderWidthValue::Thin) => CssPx(1),
        ComputedValue::BorderWidth(BorderWidthValue::Medium) => CssPx(3),
        ComputedValue::BorderWidth(BorderWidthValue::Thick) => CssPx(5),
        ComputedValue::BorderWidth(BorderWidthValue::Length(length)) => {
            resolve_length(*length, basis)
        }
        _ => unreachable!("border width has a border-width typed value"),
    }
}

fn auto_to_zero(value: AutoLength) -> CssPx {
    match value {
        AutoLength::Auto => CssPx(0),
        AutoLength::Length(value) => value,
    }
}

pub(super) fn resolve_length(length: Length, percentage_basis: CssPx) -> CssPx {
    let unit_basis = match length.unit {
        LengthUnit::Px => 1_i64,
        LengthUnit::Em | LengthUnit::Rem => i64::from(DEFAULT_FONT_SIZE),
        LengthUnit::Percent => i64::from(percentage_basis.0),
    };
    let divisor = if length.unit == LengthUnit::Percent {
        CSS_NUMBER_SCALE * 100
    } else {
        CSS_NUMBER_SCALE
    };
    let value = length.number.scaled().saturating_mul(unit_basis) / divisor;
    CssPx(i32::try_from(value).unwrap_or_else(|_| {
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    }))
}
