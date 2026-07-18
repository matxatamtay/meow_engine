use meow_css::{ComputedValue, DisplayValue, PropertyId};
use meow_html::{Document, NodeHandle, RenderChild};

use crate::ComputedStyleSnapshot;

use super::model::{BoxId, BoxKind, BoxNode, BoxTree};

/// Generates an independent formatting box tree from DOM and computed styles.
#[must_use]
pub fn build_box_tree(document: &Document, styles: &ComputedStyleSnapshot) -> BoxTree {
    let mut builder = Builder {
        document,
        styles,
        next_id: 0,
    };
    let roots = document
        .render_roots()
        .into_iter()
        .filter_map(|root| builder.build_element(&root))
        .collect();
    BoxTree::new(roots)
}

struct Builder<'a> {
    document: &'a Document,
    styles: &'a ComputedStyleSnapshot,
    next_id: u32,
}

impl Builder<'_> {
    fn build_element(&mut self, element: &NodeHandle) -> Option<BoxNode> {
        let style = self.styles.style_for(element.id())?;
        let kind = match style.typed(PropertyId::Display) {
            ComputedValue::Display(DisplayValue::None) => return None,
            ComputedValue::Display(DisplayValue::Inline | DisplayValue::InlineBlock) => {
                BoxKind::PrincipalInline
            }
            ComputedValue::Display(
                DisplayValue::Block | DisplayValue::Flex | DisplayValue::Grid,
            ) => BoxKind::PrincipalBlock,
            _ => unreachable!("display always has a display typed value"),
        };
        let id = self.allocate_id();
        let raw_children = self
            .document
            .render_children(element)
            .into_iter()
            .filter_map(|child| self.build_child(child))
            .collect::<Vec<_>>();
        let children = if kind == BoxKind::PrincipalBlock {
            self.wrap_mixed_inline_runs(raw_children)
        } else {
            raw_children
        };
        Some(BoxNode {
            id,
            kind,
            source: Some(element.id()),
            local_name: self.document.element_local_name(element),
            element_id: self.document.element_attribute(element, "id"),
            text: None,
            children,
        })
    }

    fn build_child(&mut self, child: RenderChild) -> Option<BoxNode> {
        match child {
            RenderChild::Element(element) => self.build_element(&element),
            RenderChild::Text { node, text } => {
                let text = normalize_text(&text)?;
                Some(BoxNode {
                    id: self.allocate_id(),
                    kind: BoxKind::TextRun,
                    source: Some(node),
                    local_name: None,
                    element_id: None,
                    text: Some(text),
                    children: Vec::new(),
                })
            }
        }
    }

    fn wrap_mixed_inline_runs(&mut self, children: Vec<BoxNode>) -> Vec<BoxNode> {
        let has_block = children.iter().any(|child| !child.kind.is_inline_level());
        let has_inline = children.iter().any(|child| child.kind.is_inline_level());
        if !has_block || !has_inline {
            return children;
        }

        let mut output = Vec::new();
        let mut inline_run = Vec::new();
        for child in children {
            if child.kind.is_inline_level() {
                inline_run.push(child);
            } else {
                self.flush_inline_run(&mut inline_run, &mut output);
                output.push(child);
            }
        }
        self.flush_inline_run(&mut inline_run, &mut output);
        output
    }

    fn flush_inline_run(&mut self, run: &mut Vec<BoxNode>, output: &mut Vec<BoxNode>) {
        if run.is_empty() {
            return;
        }
        output.push(BoxNode {
            id: self.allocate_id(),
            kind: BoxKind::AnonymousBlock,
            source: None,
            local_name: None,
            element_id: None,
            text: None,
            children: std::mem::take(run),
        });
    }

    fn allocate_id(&mut self) -> BoxId {
        let id = BoxId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("box tree exceeded u32 identities");
        id
    }
}

fn normalize_text(text: &str) -> Option<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}
