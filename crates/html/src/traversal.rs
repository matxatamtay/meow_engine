//! Read-only DOM traversal helpers for style and later layout stages.

use super::dom::{Document, DomState, NodeHandle, NodeKind, node};

impl Document {
    /// Returns every element in document tree order.
    #[must_use]
    pub fn elements_in_tree_order(&self) -> Vec<NodeHandle> {
        let state = self.inner.state.borrow();
        let mut elements = Vec::new();
        collect_elements(&state, &self.root, &mut elements);
        elements
    }

    /// Returns the nearest element parent, skipping non-element ancestors.
    #[must_use]
    pub fn parent_element(&self, element: &NodeHandle) -> Option<NodeHandle> {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        let mut current = node(&state, element).parent.clone();
        while let Some(candidate) = current {
            if matches!(node(&state, &candidate).kind, NodeKind::Element { .. }) {
                return Some(candidate);
            }
            current = node(&state, &candidate).parent.clone();
        }
        None
    }

    /// Returns direct element children in tree order.
    #[must_use]
    pub fn element_children(&self, element: &NodeHandle) -> Vec<NodeHandle> {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        node(&state, element)
            .children
            .iter()
            .filter(|child| matches!(node(&state, child).kind, NodeKind::Element { .. }))
            .cloned()
            .collect()
    }

    /// Returns the local element name, or `None` for a non-element handle.
    #[must_use]
    pub fn element_local_name(&self, element: &NodeHandle) -> Option<String> {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        let NodeKind::Element { name, .. } = &node(&state, element).kind else {
            return None;
        };
        Some(name.local.to_string())
    }
}

fn collect_elements(state: &DomState, handle: &NodeHandle, output: &mut Vec<NodeHandle>) {
    if matches!(node(state, handle).kind, NodeKind::Element { .. }) {
        output.push(handle.clone());
    }
    for child in &node(state, handle).children {
        collect_elements(state, child, output);
    }
}
