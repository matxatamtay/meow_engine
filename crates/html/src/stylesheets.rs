//! DOM discovery for base URLs and stylesheet-bearing elements.

use super::dom::{Document, DomState, NodeHandle, NodeId, NodeKind, attribute_value, node};

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

impl Document {
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

fn collect_text_content(state: &DomState, handle: &NodeHandle, output: &mut String) {
    let current = node(state, handle);
    if let NodeKind::Text(text) = &current.kind {
        output.push_str(text);
    }
    for child in &current.children {
        collect_text_content(state, child, output);
    }
}
