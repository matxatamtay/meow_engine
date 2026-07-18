//! html5ever `TreeSink` adapter backed by the DOM arena.

use std::{borrow::Cow, rc::Rc};

use html5ever::{
    Attribute, ExpandedName, QualName, expanded_name,
    interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink},
    local_name, ns,
    tendril::StrTendril,
};

use super::dom::{Document, DocumentQuirksMode, NodeHandle, NodeKind, node, node_mut};

#[derive(Clone)]
pub(super) struct DomSink {
    document: Document,
}

impl DomSink {
    pub(super) fn new() -> Self {
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
