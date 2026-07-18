use std::fmt::Write as _;

use meow_html::NodeId;

use crate::{BoxId, BoxKind};

/// Integer CSS pixel used by deterministic layout snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CssPx(pub i32);

impl CssPx {
    #[must_use]
    pub const fn max_zero(self) -> Self {
        Self(if self.0 < 0 { 0 } else { self.0 })
    }
}

/// Four physical edges in top/right/bottom/left order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeSizes {
    pub top: CssPx,
    pub right: CssPx,
    pub bottom: CssPx,
    pub left: CssPx,
}

/// Pixel-aligned layout rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutRect {
    pub x: CssPx,
    pub y: CssPx,
    pub width: CssPx,
    pub height: CssPx,
}

/// Viewport passed to the layout stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutViewport {
    pub width: CssPx,
    pub height: CssPx,
}

impl LayoutViewport {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: CssPx(i32::try_from(width).expect("viewport width exceeds i32")),
            height: CssPx(i32::try_from(height).expect("viewport height exceeds i32")),
        }
    }
}

/// Overflow facts produced by W15 and initialized by W14.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverflowMetadata {
    pub horizontal: bool,
    pub vertical: bool,
    pub scroll_width: CssPx,
    pub scroll_height: CssPx,
}

/// One box with resolved geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutBox {
    pub box_id: BoxId,
    pub kind: BoxKind,
    pub source: Option<NodeId>,
    pub containing_block_width: CssPx,
    pub content: LayoutRect,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
    pub margin: EdgeSizes,
    pub overflow: OverflowMetadata,
    pub children: Vec<LayoutBox>,
}

impl LayoutBox {
    #[must_use]
    pub const fn border_box_width(&self) -> CssPx {
        CssPx(
            self.content.width.0
                + self.padding.left.0
                + self.padding.right.0
                + self.border.left.0
                + self.border.right.0,
        )
    }

    #[must_use]
    pub const fn border_box_height(&self) -> CssPx {
        CssPx(
            self.content.height.0
                + self.padding.top.0
                + self.padding.bottom.0
                + self.border.top.0
                + self.border.bottom.0,
        )
    }

    #[must_use]
    pub const fn border_box_rect(&self) -> LayoutRect {
        LayoutRect {
            x: CssPx(self.content.x.0 - self.padding.left.0 - self.border.left.0),
            y: CssPx(self.content.y.0 - self.padding.top.0 - self.border.top.0),
            width: self.border_box_width(),
            height: self.border_box_height(),
        }
    }
}

/// Fully resolved layout tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutTree {
    viewport: LayoutViewport,
    roots: Vec<LayoutBox>,
}

impl LayoutTree {
    pub(super) fn new(viewport: LayoutViewport, roots: Vec<LayoutBox>) -> Self {
        Self { viewport, roots }
    }

    #[must_use]
    pub const fn viewport(&self) -> LayoutViewport {
        self.viewport
    }

    #[must_use]
    pub fn roots(&self) -> &[LayoutBox] {
        &self.roots
    }

    pub(super) fn roots_mut(&mut self) -> &mut [LayoutBox] {
        &mut self.roots
    }

    #[must_use]
    pub fn box_by_id(&self, id: BoxId) -> Option<&LayoutBox> {
        self.roots.iter().find_map(|root| find_box(root, id))
    }

    #[must_use]
    pub fn find_source(&self, source: NodeId) -> Option<&LayoutBox> {
        self.roots.iter().find_map(|root| find_source(root, source))
    }

    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::new();
        writeln!(
            output,
            "#layout-tree viewport={}x{}",
            self.viewport.width.0, self.viewport.height.0
        )
        .expect("writing to String cannot fail");
        for root in &self.roots {
            dump_box(root, 0, &mut output);
        }
        output
    }
}

fn find_box(node: &LayoutBox, id: BoxId) -> Option<&LayoutBox> {
    if node.box_id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_box(child, id))
}

fn find_source(node: &LayoutBox, source: NodeId) -> Option<&LayoutBox> {
    if node.source == Some(source) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_source(child, source))
}

fn dump_box(node: &LayoutBox, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    let source = node
        .source
        .map(|source| source.slot.to_string())
        .unwrap_or_else(|| "-".to_owned());
    writeln!(
        output,
        "{indent}{} box={} source={} cb={} content=({},{} {}x{}) margin=({},{},{},{}) padding=({},{},{},{}) border=({},{},{},{}) border-box={}x{} overflow=({},{} {}x{})",
        node.kind.name(),
        node.box_id.0,
        source,
        node.containing_block_width.0,
        node.content.x.0,
        node.content.y.0,
        node.content.width.0,
        node.content.height.0,
        node.margin.top.0,
        node.margin.right.0,
        node.margin.bottom.0,
        node.margin.left.0,
        node.padding.top.0,
        node.padding.right.0,
        node.padding.bottom.0,
        node.padding.left.0,
        node.border.top.0,
        node.border.right.0,
        node.border.bottom.0,
        node.border.left.0,
        node.border_box_width().0,
        node.border_box_height().0,
        node.overflow.horizontal,
        node.overflow.vertical,
        node.overflow.scroll_width.0,
        node.overflow.scroll_height.0,
    )
    .expect("writing to String cannot fail");
    for child in &node.children {
        dump_box(child, depth + 1, output);
    }
}
