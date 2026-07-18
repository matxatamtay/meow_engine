//! Generational DOM arena, stable node handles, tree mutation, and deterministic dumps.

use std::{
    cell::RefCell,
    fmt,
    hash::{Hash, Hasher},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use html5ever::{Attribute, QualName, interface::NodeOrText};

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable identity for a node in one document arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    /// Unique document identity.
    pub document: u64,
    /// Arena slot index.
    pub slot: u32,
    /// Slot generation.
    pub generation: u32,
}

/// Parser-facing node reference. Equality and hashing use only generational identity.
#[derive(Clone)]
pub struct NodeHandle {
    pub(super) id: NodeId,
    pub(super) element_name: Option<Rc<QualName>>,
}

impl NodeHandle {
    /// Returns the stable node identity.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
    }
}

impl PartialEq for NodeHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for NodeHandle {}

impl Hash for NodeHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl fmt::Debug for NodeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("NodeHandle").field(&self.id).finish()
    }
}

/// Parsed document sharing its arena with lightweight node handles.
#[derive(Clone)]
pub struct Document {
    pub(super) inner: Rc<DocumentInner>,
    pub(super) root: NodeHandle,
}

impl fmt::Debug for Document {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Document")
            .field("id", &self.root.id.document)
            .field("nodes", &self.node_count())
            .field("quirks_mode", &self.quirks_mode())
            .finish()
    }
}

pub(super) struct DocumentInner {
    pub(super) state: RefCell<DomState>,
}

pub(super) struct DomState {
    pub(super) slots: Vec<Slot>,
    pub(super) quirks_mode: DocumentQuirksMode,
    pub(super) parse_errors: Vec<String>,
}

pub(super) struct Slot {
    pub(super) generation: u32,
    pub(super) node: Node,
}

pub(super) struct Node {
    pub(super) parent: Option<NodeHandle>,
    pub(super) children: Vec<NodeHandle>,
    pub(super) kind: NodeKind,
}

pub(super) enum NodeKind {
    Document,
    DocumentFragment,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element {
        name: Rc<QualName>,
        attrs: Vec<Attribute>,
        template_contents: Option<NodeHandle>,
    },
    Text(String),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
}

/// HTML document quirks mode selected by the tree builder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentQuirksMode {
    /// Standards mode.
    #[default]
    NoQuirks,
    /// Limited quirks mode.
    LimitedQuirks,
    /// Full quirks mode.
    Quirks,
}

impl Document {
    pub(super) fn new() -> Self {
        let document_id = NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed);
        let root = NodeHandle {
            id: NodeId {
                document: document_id,
                slot: 0,
                generation: 0,
            },
            element_name: None,
        };
        let state = DomState {
            slots: vec![Slot {
                generation: 0,
                node: Node {
                    parent: None,
                    children: Vec::new(),
                    kind: NodeKind::Document,
                },
            }],
            quirks_mode: DocumentQuirksMode::NoQuirks,
            parse_errors: Vec::new(),
        };
        Self {
            inner: Rc::new(DocumentInner {
                state: RefCell::new(state),
            }),
            root,
        }
    }

    /// Returns the document node handle.
    #[must_use]
    pub fn root(&self) -> NodeHandle {
        self.root.clone()
    }

    /// Returns the number of allocated arena nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.inner.state.borrow().slots.len()
    }

    /// Returns tree-builder parse diagnostics.
    #[must_use]
    pub fn parse_errors(&self) -> Vec<String> {
        self.inner.state.borrow().parse_errors.clone()
    }

    /// Returns the selected quirks mode.
    #[must_use]
    pub fn quirks_mode(&self) -> DocumentQuirksMode {
        self.inner.state.borrow().quirks_mode
    }

    /// Produces a deterministic, indentation-based DOM dump.
    #[must_use]
    pub fn dump(&self) -> String {
        let state = self.inner.state.borrow();
        let mut output = String::new();
        dump_node(&state, &self.root, 0, &mut output);
        output
    }

    pub(super) fn allocate(
        &self,
        kind: NodeKind,
        element_name: Option<Rc<QualName>>,
    ) -> NodeHandle {
        let mut state = self.inner.state.borrow_mut();
        let slot = u32::try_from(state.slots.len()).expect("document arena exceeded u32 slots");
        let handle = NodeHandle {
            id: NodeId {
                document: self.root.id.document,
                slot,
                generation: 0,
            },
            element_name,
        };
        state.slots.push(Slot {
            generation: 0,
            node: Node {
                parent: None,
                children: Vec::new(),
                kind,
            },
        });
        handle
    }

    pub(super) fn append(&self, parent: &NodeHandle, child: NodeOrText<NodeHandle>) {
        match child {
            NodeOrText::AppendNode(child) => self.append_node(parent, &child),
            NodeOrText::AppendText(text) => self.append_text(parent, text.as_ref()),
        }
    }

    pub(super) fn append_node(&self, parent: &NodeHandle, child: &NodeHandle) {
        self.assert_same_document(parent);
        self.assert_same_document(child);
        self.remove_from_parent(child);
        {
            let mut state = self.inner.state.borrow_mut();
            node_mut(&mut state, child).parent = Some(parent.clone());
        }
        self.inner
            .state
            .borrow_mut()
            .slots
            .get_mut(parent.id.slot as usize)
            .expect("valid parent slot")
            .node
            .children
            .push(child.clone());
    }

    fn append_text(&self, parent: &NodeHandle, text: &str) {
        if text.is_empty() {
            return;
        }
        let last_child = {
            let state = self.inner.state.borrow();
            node(&state, parent).children.last().cloned()
        };
        if let Some(last_child) = last_child {
            let mut state = self.inner.state.borrow_mut();
            if let NodeKind::Text(existing) = &mut node_mut(&mut state, &last_child).kind {
                existing.push_str(text);
                return;
            }
        }
        let child = self.allocate(NodeKind::Text(text.to_owned()), None);
        self.append_node(parent, &child);
    }

    pub(super) fn append_before_sibling(
        &self,
        sibling: &NodeHandle,
        child: NodeOrText<NodeHandle>,
    ) {
        let parent = {
            let state = self.inner.state.borrow();
            node(&state, sibling)
                .parent
                .clone()
                .expect("sibling must have a parent")
        };
        match child {
            NodeOrText::AppendText(text) => {
                let previous = {
                    let state = self.inner.state.borrow();
                    let parent_node = node(&state, &parent);
                    let index = parent_node
                        .children
                        .iter()
                        .position(|candidate| candidate == sibling)
                        .expect("sibling must be present in parent");
                    index
                        .checked_sub(1)
                        .map(|index| parent_node.children[index].clone())
                };
                if let Some(previous) = previous {
                    let mut state = self.inner.state.borrow_mut();
                    if let NodeKind::Text(existing) = &mut node_mut(&mut state, &previous).kind {
                        existing.push_str(text.as_ref());
                        return;
                    }
                }
                let text_node = self.allocate(NodeKind::Text(text.to_string()), None);
                self.insert_node_before(&parent, sibling, &text_node);
            }
            NodeOrText::AppendNode(child) => self.insert_node_before(&parent, sibling, &child),
        }
    }

    fn insert_node_before(&self, parent: &NodeHandle, sibling: &NodeHandle, child: &NodeHandle) {
        self.remove_from_parent(child);
        {
            let mut state = self.inner.state.borrow_mut();
            node_mut(&mut state, child).parent = Some(parent.clone());
        }
        let mut state = self.inner.state.borrow_mut();
        let parent_node = node_mut(&mut state, parent);
        let index = parent_node
            .children
            .iter()
            .position(|candidate| candidate == sibling)
            .expect("sibling must be present in parent");
        parent_node.children.insert(index, child.clone());
    }

    pub(super) fn remove_from_parent(&self, target: &NodeHandle) {
        let parent = {
            let state = self.inner.state.borrow();
            node(&state, target).parent.clone()
        };
        let Some(parent) = parent else {
            return;
        };
        {
            let mut state = self.inner.state.borrow_mut();
            node_mut(&mut state, &parent)
                .children
                .retain(|candidate| candidate != target);
        }
        node_mut(&mut self.inner.state.borrow_mut(), target).parent = None;
    }

    pub(super) fn reparent_children(&self, source: &NodeHandle, destination: &NodeHandle) {
        let children = {
            let mut state = self.inner.state.borrow_mut();
            std::mem::take(&mut node_mut(&mut state, source).children)
        };
        for child in children {
            node_mut(&mut self.inner.state.borrow_mut(), &child).parent = None;
            self.append_node(destination, &child);
        }
    }

    pub(super) fn assert_same_document(&self, handle: &NodeHandle) {
        assert_eq!(
            handle.id.document, self.root.id.document,
            "node handle belongs to a different document"
        );
    }
}

pub(super) fn node<'a>(state: &'a DomState, handle: &NodeHandle) -> &'a Node {
    let slot = state
        .slots
        .get(handle.id.slot as usize)
        .expect("node slot must exist");
    assert_eq!(slot.generation, handle.id.generation, "stale node handle");
    &slot.node
}

pub(super) fn node_mut<'a>(state: &'a mut DomState, handle: &NodeHandle) -> &'a mut Node {
    let slot = state
        .slots
        .get_mut(handle.id.slot as usize)
        .expect("node slot must exist");
    assert_eq!(slot.generation, handle.id.generation, "stale node handle");
    &mut slot.node
}

fn dump_node(state: &DomState, handle: &NodeHandle, depth: usize, output: &mut String) {
    let current = node(state, handle);
    let indent = "  ".repeat(depth);
    match &current.kind {
        NodeKind::Document => output.push_str("#document\n"),
        NodeKind::DocumentFragment => output.push_str(&format!("{indent}#document-fragment\n")),
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => {
            if public_id.is_empty() && system_id.is_empty() {
                output.push_str(&format!("{indent}<!DOCTYPE {name}>\n"));
            } else {
                output.push_str(&format!(
                    "{indent}<!DOCTYPE {name} PUBLIC {public_id:?} {system_id:?}>\n"
                ));
            }
        }
        NodeKind::Element { name, attrs, .. } => {
            let qualified = if name.ns.as_ref() == "http://www.w3.org/1999/xhtml" {
                name.local.to_string()
            } else {
                format!("{{{}}}{}", name.ns, name.local)
            };
            output.push_str(&format!("{indent}<{qualified}"));
            let mut attrs = attrs.iter().collect::<Vec<_>>();
            attrs.sort_by(|left, right| {
                (&left.name.ns, &left.name.local).cmp(&(&right.name.ns, &right.name.local))
            });
            for attribute in attrs {
                output.push_str(&format!(
                    " {}={:?}",
                    attribute.name.local,
                    attribute.value.as_ref()
                ));
            }
            output.push_str(">\n");
        }
        NodeKind::Text(text) => output.push_str(&format!("{indent}{text:?}\n")),
        NodeKind::Comment(text) => output.push_str(&format!("{indent}<!--{text}-->\n")),
        NodeKind::ProcessingInstruction { target, data } => {
            output.push_str(&format!("{indent}<?{target} {data}>\n"));
        }
    }
    for child in &current.children {
        dump_node(state, child, depth + 1, output);
    }
}

pub(super) fn attribute_value(attrs: &[Attribute], local_name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|attribute| {
            attribute
                .name
                .local
                .as_ref()
                .eq_ignore_ascii_case(local_name)
        })
        .map(|attribute| attribute.value.to_string())
}
