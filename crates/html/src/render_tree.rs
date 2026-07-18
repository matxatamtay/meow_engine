//! DOM child snapshots for box-tree construction without exposing arena internals.

use super::dom::{Document, NodeHandle, NodeId, NodeKind, node};

/// Render-relevant child data copied out of the DOM arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderChild {
    /// Element child represented by its stable handle.
    Element(NodeHandle),
    /// Text child represented by stable identity and source text.
    Text { node: NodeId, text: String },
}

impl Document {
    /// Returns direct render-relevant children in DOM tree order.
    #[must_use]
    pub fn render_children(&self, parent: &NodeHandle) -> Vec<RenderChild> {
        self.assert_same_document(parent);
        let state = self.inner.state.borrow();
        node(&state, parent)
            .children
            .iter()
            .filter_map(|child| match &node(&state, child).kind {
                NodeKind::Element { .. } => Some(RenderChild::Element(child.clone())),
                NodeKind::Text(text) => Some(RenderChild::Text {
                    node: child.id,
                    text: text.clone(),
                }),
                NodeKind::Document
                | NodeKind::DocumentFragment
                | NodeKind::Doctype { .. }
                | NodeKind::Comment(_)
                | NodeKind::ProcessingInstruction { .. } => None,
            })
            .collect()
    }

    /// Returns top-level element roots in document tree order.
    #[must_use]
    pub fn render_roots(&self) -> Vec<NodeHandle> {
        let state = self.inner.state.borrow();
        node(&state, &self.root)
            .children
            .iter()
            .filter(|child| matches!(node(&state, child).kind, NodeKind::Element { .. }))
            .cloned()
            .collect()
    }
}
