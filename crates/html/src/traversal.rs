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

    /// Returns an element subtree in tree order, including the root.
    #[must_use]
    pub fn element_subtree(&self, root: &NodeHandle) -> Vec<NodeHandle> {
        self.assert_same_document(root);
        let state = self.inner.state.borrow();
        let mut elements = Vec::new();
        collect_elements(&state, root, &mut elements);
        elements
    }

    /// Returns the next element sibling.
    #[must_use]
    pub fn next_element_sibling(&self, element: &NodeHandle) -> Option<NodeHandle> {
        self.following_element_siblings(element).into_iter().next()
    }

    /// Returns following element siblings in tree order.
    #[must_use]
    pub fn following_element_siblings(&self, element: &NodeHandle) -> Vec<NodeHandle> {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        let Some(parent) = node(&state, element).parent.as_ref() else {
            return Vec::new();
        };
        let children = &node(&state, parent).children;
        let Some(index) = children.iter().position(|candidate| candidate == element) else {
            return Vec::new();
        };
        children[index + 1..]
            .iter()
            .filter(|child| matches!(node(&state, child).kind, NodeKind::Element { .. }))
            .cloned()
            .collect()
    }

    /// Resolves a connected element by stable node identity.
    #[must_use]
    pub fn element_by_id(&self, id: super::NodeId) -> Option<NodeHandle> {
        if id.document != self.root.id.document {
            return None;
        }
        self.elements_in_tree_order()
            .into_iter()
            .find(|element| element.id == id)
    }

    /// Returns concatenated descendant text in DOM order.
    #[must_use]
    pub fn text_content(&self, root: &NodeHandle) -> String {
        self.assert_same_document(root);
        let state = self.inner.state.borrow();
        let mut output = String::new();
        collect_text(&state, root, &mut output);
        output
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

fn collect_text(state: &DomState, handle: &NodeHandle, output: &mut String) {
    let current = node(state, handle);
    if let NodeKind::Text(text) = &current.kind {
        output.push_str(text);
    }
    for child in &current.children {
        collect_text(state, child, output);
    }
}
