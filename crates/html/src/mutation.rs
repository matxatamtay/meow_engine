//! Explicit DOM mutation APIs and records consumed by style invalidation.

use std::{error::Error, fmt, rc::Rc};

use html5ever::{Attribute, LocalName, QualName, ns};

use super::dom::{Document, NodeHandle, NodeId, NodeKind, node, node_mut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomMutationKind {
    Attributes,
    CharacterData,
    ChildList,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomMutation {
    pub kind: DomMutationKind,
    pub target: NodeId,
    pub attribute_name: Option<String>,
    pub added_nodes: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomMutationError {
    ExpectedElement,
    ExpectedText,
    DetachedNode,
    CannotRemoveDocumentElement,
    InvalidName,
}

impl fmt::Display for DomMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExpectedElement => "DOM mutation target must be an element",
            Self::ExpectedText => "DOM mutation target must be a text node",
            Self::DetachedNode => "DOM mutation target is detached",
            Self::CannotRemoveDocumentElement => "the document element cannot be removed",
            Self::InvalidName => "DOM name must not be empty or contain ASCII whitespace",
        })
    }
}

impl Error for DomMutationError {}

impl Document {
    pub fn set_element_attribute(
        &self,
        element: &NodeHandle,
        local_name: &str,
        value: &str,
    ) -> Result<Option<DomMutation>, DomMutationError> {
        self.assert_same_document(element);
        let local_name = validate_name(local_name)?;
        let mut state = self.inner.state.borrow_mut();
        let NodeKind::Element { name, attrs, .. } = &mut node_mut(&mut state, element).kind else {
            return Err(DomMutationError::ExpectedElement);
        };
        let html = name.ns.as_ref() == "http://www.w3.org/1999/xhtml";
        let canonical = if html {
            local_name.to_ascii_lowercase()
        } else {
            local_name.to_owned()
        };
        if let Some(attribute) = attrs.iter_mut().find(|attribute| {
            attribute
                .name
                .local
                .as_ref()
                .eq_ignore_ascii_case(&canonical)
        }) {
            if attribute.value.as_ref() == value {
                return Ok(None);
            }
            attribute.value = value.into();
        } else {
            attrs.push(Attribute {
                name: QualName::new(None, ns!(), LocalName::from(canonical.as_str())),
                value: value.into(),
            });
        }
        Ok(Some(attribute_mutation(element.id, canonical)))
    }

    pub fn remove_element_attribute(
        &self,
        element: &NodeHandle,
        local_name: &str,
    ) -> Result<Option<DomMutation>, DomMutationError> {
        self.assert_same_document(element);
        let local_name = validate_name(local_name)?;
        let mut state = self.inner.state.borrow_mut();
        let NodeKind::Element { attrs, .. } = &mut node_mut(&mut state, element).kind else {
            return Err(DomMutationError::ExpectedElement);
        };
        let before = attrs.len();
        attrs.retain(|attribute| {
            !attribute
                .name
                .local
                .as_ref()
                .eq_ignore_ascii_case(local_name)
        });
        if attrs.len() == before {
            return Ok(None);
        }
        Ok(Some(attribute_mutation(
            element.id,
            local_name.to_ascii_lowercase(),
        )))
    }

    pub fn append_element(
        &self,
        parent: &NodeHandle,
        local_name: &str,
    ) -> Result<(NodeHandle, DomMutation), DomMutationError> {
        self.assert_same_document(parent);
        let local_name = validate_name(local_name)?.to_ascii_lowercase();
        {
            let state = self.inner.state.borrow();
            if !matches!(node(&state, parent).kind, NodeKind::Element { .. }) {
                return Err(DomMutationError::ExpectedElement);
            }
        }
        let name = Rc::new(QualName::new(
            None,
            ns!(html),
            LocalName::from(local_name.as_str()),
        ));
        let child = self.allocate(
            NodeKind::Element {
                name: Rc::clone(&name),
                attrs: Vec::new(),
                template_contents: None,
            },
            Some(name),
        );
        self.append_node(parent, &child);
        Ok((
            child.clone(),
            DomMutation {
                kind: DomMutationKind::ChildList,
                target: parent.id,
                attribute_name: None,
                added_nodes: vec![child.id],
                removed_nodes: Vec::new(),
            },
        ))
    }

    pub fn append_text_node(
        &self,
        parent: &NodeHandle,
        text: &str,
    ) -> Result<(NodeHandle, DomMutation), DomMutationError> {
        self.assert_same_document(parent);
        {
            let state = self.inner.state.borrow();
            if !matches!(node(&state, parent).kind, NodeKind::Element { .. }) {
                return Err(DomMutationError::ExpectedElement);
            }
        }
        let child = self.allocate(NodeKind::Text(text.to_owned()), None);
        self.append_node(parent, &child);
        Ok((
            child.clone(),
            DomMutation {
                kind: DomMutationKind::ChildList,
                target: parent.id,
                attribute_name: None,
                added_nodes: vec![child.id],
                removed_nodes: Vec::new(),
            },
        ))
    }

    pub fn set_text(
        &self,
        text_node: &NodeHandle,
        text: &str,
    ) -> Result<Option<DomMutation>, DomMutationError> {
        self.assert_same_document(text_node);
        let mut state = self.inner.state.borrow_mut();
        let parent = node(&state, text_node)
            .parent
            .clone()
            .ok_or(DomMutationError::DetachedNode)?;
        let NodeKind::Text(existing) = &mut node_mut(&mut state, text_node).kind else {
            return Err(DomMutationError::ExpectedText);
        };
        if existing == text {
            return Ok(None);
        }
        *existing = text.to_owned();
        let target = nearest_element(&state, &parent).ok_or(DomMutationError::DetachedNode)?;
        Ok(Some(DomMutation {
            kind: DomMutationKind::CharacterData,
            target: target.id,
            attribute_name: None,
            added_nodes: Vec::new(),
            removed_nodes: Vec::new(),
        }))
    }

    pub fn remove_subtree(&self, target: &NodeHandle) -> Result<DomMutation, DomMutationError> {
        self.assert_same_document(target);
        let (parent, removed_nodes) = {
            let state = self.inner.state.borrow();
            let parent = node(&state, target)
                .parent
                .clone()
                .ok_or(DomMutationError::DetachedNode)?;
            let parent = nearest_element(&state, &parent)
                .ok_or(DomMutationError::CannotRemoveDocumentElement)?;
            let mut removed = Vec::new();
            collect_subtree_ids(&state, target, &mut removed);
            (parent, removed)
        };
        self.remove_from_parent(target);
        Ok(DomMutation {
            kind: DomMutationKind::ChildList,
            target: parent.id,
            attribute_name: None,
            added_nodes: Vec::new(),
            removed_nodes,
        })
    }
}

fn attribute_mutation(target: NodeId, name: String) -> DomMutation {
    DomMutation {
        kind: DomMutationKind::Attributes,
        target,
        attribute_name: Some(name),
        added_nodes: Vec::new(),
        removed_nodes: Vec::new(),
    }
}

fn validate_name(name: &str) -> Result<&str, DomMutationError> {
    let name = name.trim();
    if name.is_empty() || name.bytes().any(|byte| byte.is_ascii_whitespace()) {
        Err(DomMutationError::InvalidName)
    } else {
        Ok(name)
    }
}

fn nearest_element(state: &super::dom::DomState, start: &NodeHandle) -> Option<NodeHandle> {
    let mut current = Some(start.clone());
    while let Some(candidate) = current {
        if matches!(node(state, &candidate).kind, NodeKind::Element { .. }) {
            return Some(candidate);
        }
        current = node(state, &candidate).parent.clone();
    }
    None
}

fn collect_subtree_ids(
    state: &super::dom::DomState,
    handle: &NodeHandle,
    output: &mut Vec<NodeId>,
) {
    output.push(handle.id);
    for child in &node(state, handle).children {
        collect_subtree_ids(state, child, output);
    }
}
