//! Streaming HTML decoding and an html5ever `TreeSink` backed by a generational arena.

use std::{
    borrow::Cow,
    cell::RefCell,
    fmt,
    hash::{Hash, Hasher},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use encoding_rs::{CoderResult, Encoding, UTF_8};
use html5ever::{
    Attribute, ExpandedName, QualName, expanded_name,
    interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink},
    local_name, ns, parse_document,
    tendril::{StrTendril, TendrilSink},
};

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
    id: NodeId,
    element_name: Option<Rc<QualName>>,
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
    inner: Rc<DocumentInner>,
    root: NodeHandle,
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

struct DocumentInner {
    state: RefCell<DomState>,
}

struct DomState {
    slots: Vec<Slot>,
    quirks_mode: DocumentQuirksMode,
    parse_errors: Vec<String>,
}

struct Slot {
    generation: u32,
    node: Node,
}

struct Node {
    parent: Option<NodeHandle>,
    children: Vec<NodeHandle>,
    kind: NodeKind,
}

enum NodeKind {
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

/// One stylesheet-bearing DOM node discovered in tree order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StylesheetCandidate {
    /// Element node that owns this candidate.
    pub node: NodeId,
    /// Inline text or external href.
    pub kind: StylesheetCandidateKind,
    /// Optional media query text retained for a later semantic stage.
    pub media: Option<String>,
}

/// Source form of a discovered stylesheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StylesheetCandidateKind {
    /// Text content of an HTML `<style>` element.
    Inline(String),
    /// `href` value of an HTML `<link rel="stylesheet">` element.
    Linked(String),
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
    fn new() -> Self {
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

    /// Returns the first HTML `<base href>` value in tree order.
    #[must_use]
    pub fn first_base_href(&self) -> Option<String> {
        let state = self.inner.state.borrow();
        find_base_href(&state, &self.root)
    }

    /// Returns CSS-bearing `<style>` and `<link rel="stylesheet">` nodes in tree order.
    #[must_use]
    pub fn stylesheet_candidates(&self) -> Vec<StylesheetCandidate> {
        let state = self.inner.state.borrow();
        let mut candidates = Vec::new();
        collect_stylesheet_candidates(&state, &self.root, &mut candidates);
        candidates
    }

    /// Produces a deterministic, indentation-based DOM dump.
    #[must_use]
    pub fn dump(&self) -> String {
        let state = self.inner.state.borrow();
        let mut output = String::new();
        dump_node(&state, &self.root, 0, &mut output);
        output
    }

    fn allocate(&self, kind: NodeKind, element_name: Option<Rc<QualName>>) -> NodeHandle {
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

    fn append(&self, parent: &NodeHandle, child: NodeOrText<NodeHandle>) {
        match child {
            NodeOrText::AppendNode(child) => self.append_node(parent, &child),
            NodeOrText::AppendText(text) => self.append_text(parent, text.as_ref()),
        }
    }

    fn append_node(&self, parent: &NodeHandle, child: &NodeHandle) {
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

    fn append_before_sibling(&self, sibling: &NodeHandle, child: NodeOrText<NodeHandle>) {
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

    fn remove_from_parent(&self, target: &NodeHandle) {
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

    fn reparent_children(&self, source: &NodeHandle, destination: &NodeHandle) {
        let children = {
            let mut state = self.inner.state.borrow_mut();
            std::mem::take(&mut node_mut(&mut state, source).children)
        };
        for child in children {
            node_mut(&mut self.inner.state.borrow_mut(), &child).parent = None;
            self.append_node(destination, &child);
        }
    }

    fn assert_same_document(&self, handle: &NodeHandle) {
        assert_eq!(
            handle.id.document, self.root.id.document,
            "node handle belongs to a different document"
        );
    }
}

fn node<'a>(state: &'a DomState, handle: &NodeHandle) -> &'a Node {
    let slot = state
        .slots
        .get(handle.id.slot as usize)
        .expect("node slot must exist");
    assert_eq!(slot.generation, handle.id.generation, "stale node handle");
    &slot.node
}

fn node_mut<'a>(state: &'a mut DomState, handle: &NodeHandle) -> &'a mut Node {
    let slot = state
        .slots
        .get_mut(handle.id.slot as usize)
        .expect("node slot must exist");
    assert_eq!(slot.generation, handle.id.generation, "stale node handle");
    &mut slot.node
}

fn find_base_href(state: &DomState, handle: &NodeHandle) -> Option<String> {
    let current = node(state, handle);
    if let NodeKind::Element { name, attrs, .. } = &current.kind
        && name.ns.as_ref() == "http://www.w3.org/1999/xhtml"
        && name.local.as_ref() == "base"
    {
        return attrs
            .iter()
            .find(|attribute| attribute.name.local.as_ref() == "href")
            .map(|attribute| attribute.value.to_string());
    }
    current
        .children
        .iter()
        .find_map(|child| find_base_href(state, child))
}

fn collect_stylesheet_candidates(
    state: &DomState,
    handle: &NodeHandle,
    output: &mut Vec<StylesheetCandidate>,
) {
    let current = node(state, handle);
    if let NodeKind::Element { name, attrs, .. } = &current.kind
        && name.ns.as_ref() == "http://www.w3.org/1999/xhtml"
    {
        if name.local.as_ref() == "style" {
            if css_type_is_supported(attribute_value(attrs, "type").as_deref()) {
                let mut css = String::new();
                collect_text_content(state, handle, &mut css);
                output.push(StylesheetCandidate {
                    node: handle.id,
                    kind: StylesheetCandidateKind::Inline(css),
                    media: attribute_value(attrs, "media"),
                });
            }
        } else if name.local.as_ref() == "link" {
            let is_stylesheet = attribute_value(attrs, "rel").is_some_and(|rel| {
                rel.split_ascii_whitespace()
                    .any(|token| token.eq_ignore_ascii_case("stylesheet"))
            });
            let is_css = css_type_is_supported(attribute_value(attrs, "type").as_deref());
            if is_stylesheet
                && is_css
                && let Some(href) = attribute_value(attrs, "href")
            {
                output.push(StylesheetCandidate {
                    node: handle.id,
                    kind: StylesheetCandidateKind::Linked(href),
                    media: attribute_value(attrs, "media"),
                });
            }
        }
    }
    for child in &current.children {
        collect_stylesheet_candidates(state, child, output);
    }
}

fn css_type_is_supported(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        let value = value.trim();
        value.is_empty() || value.eq_ignore_ascii_case("text/css")
    })
}

fn attribute_value(attrs: &[Attribute], local_name: &str) -> Option<String> {
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

fn collect_text_content(state: &DomState, handle: &NodeHandle, output: &mut String) {
    let current = node(state, handle);
    if let NodeKind::Text(text) = &current.kind {
        output.push_str(text);
    }
    for child in &current.children {
        collect_text_content(state, child, output);
    }
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

#[derive(Clone)]
struct DomSink {
    document: Document,
}

impl DomSink {
    fn new() -> Self {
        Self {
            document: Document::new(),
        }
    }
}

impl TreeSink for DomSink {
    type Handle = NodeHandle;
    type Output = Document;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self::Output {
        self.document
    }

    fn parse_error(&self, message: Cow<'static, str>) {
        self.document
            .inner
            .state
            .borrow_mut()
            .parse_errors
            .push(message.into_owned());
    }

    fn get_document(&self) -> Self::Handle {
        self.document.root()
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        target
            .element_name
            .as_deref()
            .expect("elem_name called for a non-element")
            .expanded()
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> Self::Handle {
        let name = Rc::new(name);
        let template_contents = if name.expanded() == expanded_name!(html "template") {
            Some(self.document.allocate(NodeKind::DocumentFragment, None))
        } else {
            None
        };
        self.document.allocate(
            NodeKind::Element {
                name: Rc::clone(&name),
                attrs,
                template_contents,
            },
            Some(name),
        )
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        self.document
            .allocate(NodeKind::Comment(text.to_string()), None)
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        self.document.allocate(
            NodeKind::ProcessingInstruction {
                target: target.to_string(),
                data: data.to_string(),
            },
            None,
        )
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.document.append(parent, child);
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        previous_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let has_parent = {
            let state = self.document.inner.state.borrow();
            node(&state, element).parent.is_some()
        };
        if has_parent {
            self.document.append_before_sibling(element, child);
        } else {
            self.document.append(previous_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let doctype = self.document.allocate(
            NodeKind::Doctype {
                name: name.to_string(),
                public_id: public_id.to_string(),
                system_id: system_id.to_string(),
            },
            None,
        );
        self.document.append_node(&self.document.root, &doctype);
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let state = self.document.inner.state.borrow();
        match &node(&state, target).kind {
            NodeKind::Element {
                template_contents: Some(contents),
                ..
            } => contents.clone(),
            _ => panic!("get_template_contents called for a non-template"),
        }
    }

    fn same_node(&self, left: &Self::Handle, right: &Self::Handle) -> bool {
        left == right
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.document.inner.state.borrow_mut().quirks_mode = match mode {
            QuirksMode::NoQuirks => DocumentQuirksMode::NoQuirks,
            QuirksMode::LimitedQuirks => DocumentQuirksMode::LimitedQuirks,
            QuirksMode::Quirks => DocumentQuirksMode::Quirks,
        };
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.document.append_before_sibling(sibling, child);
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        let mut state = self.document.inner.state.borrow_mut();
        let NodeKind::Element {
            attrs: existing, ..
        } = &mut node_mut(&mut state, target).kind
        else {
            panic!("add_attrs_if_missing called for a non-element");
        };
        for attribute in attrs {
            if existing
                .iter()
                .all(|candidate| candidate.name != attribute.name)
            {
                existing.push(attribute);
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.document.remove_from_parent(target);
    }

    fn reparent_children(&self, source: &Self::Handle, destination: &Self::Handle) {
        self.document.reparent_children(source, destination);
    }
}

/// Incremental decoder feeding UTF-8 tendrils into html5ever.
pub struct StreamingParser {
    parser: html5ever::driver::Parser<DomSink>,
    decoder: encoding_rs::Decoder,
    had_replacements: bool,
}

impl StreamingParser {
    /// Creates a parser for a chosen Encoding Standard decoder.
    #[must_use]
    pub fn new(encoding: &'static Encoding) -> Self {
        Self {
            parser: parse_document(DomSink::new(), Default::default()),
            decoder: encoding.new_decoder(),
            had_replacements: false,
        }
    }

    /// Feeds another network byte chunk.
    pub fn feed(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let capacity = self
                .decoder
                .max_utf8_buffer_length(bytes.len())
                .unwrap_or(bytes.len().saturating_mul(3).saturating_add(8));
            let mut decoded = String::with_capacity(capacity.max(8));
            let (result, read, replacements) =
                self.decoder.decode_to_string(bytes, &mut decoded, false);
            self.had_replacements |= replacements;
            if !decoded.is_empty() {
                self.parser.process(decoded.into());
            }
            bytes = &bytes[read..];
            if result == CoderResult::InputEmpty {
                break;
            }
            assert!(
                read > 0,
                "decoder made no progress with a full output buffer"
            );
        }
    }

    /// Completes decoding and tree construction.
    #[must_use]
    pub fn finish(mut self) -> ParsedHtml {
        let capacity = self.decoder.max_utf8_buffer_length(0).unwrap_or(8);
        let mut decoded = String::with_capacity(capacity.max(8));
        let (_, _, replacements) = self.decoder.decode_to_string(b"", &mut decoded, true);
        self.had_replacements |= replacements;
        if !decoded.is_empty() {
            self.parser.process(decoded.into());
        }
        ParsedHtml {
            document: self.parser.finish(),
            had_replacements: self.had_replacements,
        }
    }
}

/// Result of HTML byte decoding and tree construction.
#[derive(Clone, Debug)]
pub struct ParsedHtml {
    /// Parsed document.
    pub document: Document,
    /// Whether malformed byte sequences were replaced.
    pub had_replacements: bool,
}

/// Parses a complete byte slice while preserving the streaming implementation path.
#[must_use]
pub fn parse_bytes(bytes: &[u8], encoding: &'static Encoding) -> ParsedHtml {
    let mut parser = StreamingParser::new(encoding);
    parser.feed(bytes);
    parser.finish()
}

/// Parses UTF-8 HTML bytes.
#[must_use]
pub fn parse_utf8(bytes: &[u8]) -> ParsedHtml {
    parse_bytes(bytes, UTF_8)
}

#[cfg(test)]
mod tests {
    use encoding_rs::WINDOWS_1252;

    use super::*;

    #[test]
    fn creates_document_skeleton_for_empty_input() {
        let parsed = parse_utf8(b"");
        assert_eq!(
            parsed.document.dump(),
            "#document\n  <html>\n    <head>\n    <body>\n"
        );
    }

    #[test]
    fn parses_malformed_html_into_a_stable_tree() {
        let parsed = parse_utf8(b"<!doctype html><title>x</title><p id=p>one<div>two</p>three");
        assert_eq!(
            parsed.document.dump(),
            concat!(
                "#document\n",
                "  <!DOCTYPE html>\n",
                "  <html>\n",
                "    <head>\n",
                "      <title>\n",
                "        \"x\"\n",
                "    <body>\n",
                "      <p id=\"p\">\n",
                "        \"one\"\n",
                "      <div>\n",
                "        \"two\"\n",
                "        <p>\n",
                "        \"three\"\n",
            )
        );
    }

    #[test]
    fn streaming_decoder_preserves_split_multibyte_sequences() {
        let source = "<p>mèo 🐈</p>".as_bytes();
        let mut parser = StreamingParser::new(UTF_8);
        for byte in source {
            parser.feed(std::slice::from_ref(byte));
        }
        let parsed = parser.finish();

        assert!(!parsed.had_replacements);
        assert!(parsed.document.dump().contains("mèo 🐈"));
    }

    #[test]
    fn decodes_legacy_encoding_and_reports_replacements() {
        let parsed = parse_bytes(b"<p>caf\xe9</p>", WINDOWS_1252);
        assert!(parsed.document.dump().contains("café"));
        assert!(!parsed.had_replacements);

        let malformed = parse_utf8(b"<p>\xff</p>");
        assert!(malformed.had_replacements);
        assert!(malformed.document.dump().contains('�'));
    }

    #[test]
    fn exposes_first_base_href() {
        let parsed = parse_utf8(b"<base href='../assets/'><base href='/ignored/'>");
        assert_eq!(
            parsed.document.first_base_href().as_deref(),
            Some("../assets/")
        );
    }

    #[test]
    fn discovers_inline_and_linked_stylesheets_in_tree_order() {
        let parsed = parse_utf8(
            br#"<style type=" TEXT/CSS " media="screen">a { color: red }</style>
                <link rel="preload StyleSheet" type=" text/css " href="theme.css" media="print">
                <style type="text/less">ignored</style>
                <link rel="stylesheet" type="text/less" href="ignored.less">
                <link rel="stylesheet">"#,
        );
        let candidates = parsed.document.stylesheet_candidates();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].media.as_deref(), Some("screen"));
        assert!(matches!(
            &candidates[0].kind,
            StylesheetCandidateKind::Inline(css) if css == "a { color: red }"
        ));
        assert_eq!(candidates[1].media.as_deref(), Some("print"));
        assert!(matches!(
            &candidates[1].kind,
            StylesheetCandidateKind::Linked(href) if href == "theme.css"
        ));
    }
}
