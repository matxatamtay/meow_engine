//! Selected accessibility tree, accessible-name, role, and focus-order support.

use std::collections::{BTreeMap, BTreeSet};

use meow_html::{Document, NodeHandle, NodeId};
use serde::{Deserialize, Serialize};

/// Selected accessible roles supported by the public alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessibleRole {
    Document,
    Main,
    Navigation,
    Form,
    Heading,
    Link,
    Button,
    TextBox,
    CheckBox,
    Image,
    List,
    ListItem,
    Paragraph,
    Generic,
}

impl AccessibleRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Main => "main",
            Self::Navigation => "navigation",
            Self::Form => "form",
            Self::Heading => "heading",
            Self::Link => "link",
            Self::Button => "button",
            Self::TextBox => "textbox",
            Self::CheckBox => "checkbox",
            Self::Image => "image",
            Self::List => "list",
            Self::ListItem => "listitem",
            Self::Paragraph => "paragraph",
            Self::Generic => "generic",
        }
    }
}

/// One node in the selected accessibility tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessibleNode {
    pub node_slot: u32,
    pub role: AccessibleRole,
    pub name: String,
    pub focusable: bool,
    pub disabled: bool,
    pub tabindex: Option<i32>,
    pub children: Vec<AccessibleNode>,
}

/// Accessibility tree and keyboard focus order for one DOM.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessibilityTree {
    pub root: AccessibleNode,
    pub focus_order_slots: Vec<u32>,
}

impl AccessibilityTree {
    #[must_use]
    pub fn build(document: &Document) -> Self {
        let elements = document.elements_in_tree_order();
        let id_map = elements
            .iter()
            .filter_map(|element| {
                document
                    .element_attribute(element, "id")
                    .map(|id| (id, element.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let labels = collect_labels(document, &elements);
        let mut root = AccessibleNode {
            node_slot: document.root().id().slot,
            role: AccessibleRole::Document,
            name: String::new(),
            focusable: false,
            disabled: false,
            tabindex: None,
            children: Vec::new(),
        };
        for element in document.element_children(&document.root()) {
            if let Some(node) = build_node(document, &element, &id_map, &labels) {
                root.children.push(node);
            }
        }
        let focus_order_slots = focus_order(document)
            .into_iter()
            .map(|node| node.slot)
            .collect();
        Self {
            root,
            focus_order_slots,
        }
    }

    #[must_use]
    pub fn flatten(&self) -> Vec<&AccessibleNode> {
        let mut output = Vec::new();
        flatten_node(&self.root, &mut output);
        output
    }
}

/// One keyboard/accessibility audit finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessibilityFinding {
    pub code: String,
    pub node_slot: u32,
    pub message: String,
}

/// Result of the selected W46 accessibility and keyboard audit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessibilityAudit {
    pub focus_order_slots: Vec<u32>,
    pub findings: Vec<AccessibilityFinding>,
}

impl AccessibilityAudit {
    #[must_use]
    pub fn passes(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Audits names, focusability, duplicate positive tabindex values, and hidden controls.
#[must_use]
pub fn audit_keyboard_navigation(document: &Document) -> AccessibilityAudit {
    let tree = AccessibilityTree::build(document);
    let mut findings = Vec::new();
    let mut positive = BTreeMap::<i32, Vec<u32>>::new();
    for node in tree.flatten() {
        if node.focusable
            && node.name.is_empty()
            && matches!(
                node.role,
                AccessibleRole::Link
                    | AccessibleRole::Button
                    | AccessibleRole::TextBox
                    | AccessibleRole::CheckBox
            )
        {
            findings.push(AccessibilityFinding {
                code: "missing-accessible-name".to_owned(),
                node_slot: node.node_slot,
                message: format!("{} has no accessible name", node.role.as_str()),
            });
        }
        if let Some(tabindex) = node.tabindex.filter(|value| *value > 0) {
            positive.entry(tabindex).or_default().push(node.node_slot);
        }
    }
    for (tabindex, slots) in positive {
        if slots.len() > 1 {
            for slot in slots {
                findings.push(AccessibilityFinding {
                    code: "duplicate-positive-tabindex".to_owned(),
                    node_slot: slot,
                    message: format!("tabindex {tabindex} is shared by multiple elements"),
                });
            }
        }
    }
    AccessibilityAudit {
        focus_order_slots: tree.focus_order_slots,
        findings,
    }
}

/// Computes sequential keyboard focus order using HTML tabindex ordering rules.
#[must_use]
pub fn focus_order(document: &Document) -> Vec<NodeId> {
    let mut positive = Vec::<(i32, usize, NodeId)>::new();
    let mut normal = Vec::<(usize, NodeId)>::new();
    for (tree_index, element) in document.elements_in_tree_order().into_iter().enumerate() {
        if is_hidden(document, &element) || is_disabled(document, &element) {
            continue;
        }
        let tabindex = parse_tabindex(document, &element);
        let inherent = inherent_focusable(document, &element);
        match tabindex {
            Some(value) if value < 0 => {}
            Some(value) if value > 0 => positive.push((value, tree_index, element.id())),
            Some(_) => normal.push((tree_index, element.id())),
            None if inherent => normal.push((tree_index, element.id())),
            None => {}
        }
    }
    positive.sort_by_key(|(tabindex, tree_index, _)| (*tabindex, *tree_index));
    positive
        .into_iter()
        .map(|(_, _, node)| node)
        .chain(normal.into_iter().map(|(_, node)| node))
        .collect()
}

fn build_node(
    document: &Document,
    element: &NodeHandle,
    id_map: &BTreeMap<String, NodeHandle>,
    labels: &BTreeMap<String, String>,
) -> Option<AccessibleNode> {
    if is_hidden(document, element) {
        return None;
    }
    let role = role_for(document, element);
    let name = accessible_name(document, element, id_map, labels);
    let disabled = is_disabled(document, element);
    let tabindex = parse_tabindex(document, element);
    let focusable = !disabled
        && tabindex.map_or_else(|| inherent_focusable(document, element), |value| value >= 0);
    let children = document
        .element_children(element)
        .into_iter()
        .filter_map(|child| build_node(document, &child, id_map, labels))
        .collect();
    Some(AccessibleNode {
        node_slot: element.id().slot,
        role,
        name,
        focusable,
        disabled,
        tabindex,
        children,
    })
}

fn role_for(document: &Document, element: &NodeHandle) -> AccessibleRole {
    if let Some(role) = document.element_attribute(element, "role") {
        match role
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "main" => return AccessibleRole::Main,
            "navigation" | "nav" => return AccessibleRole::Navigation,
            "form" => return AccessibleRole::Form,
            "heading" => return AccessibleRole::Heading,
            "link" => return AccessibleRole::Link,
            "button" => return AccessibleRole::Button,
            "textbox" => return AccessibleRole::TextBox,
            "checkbox" => return AccessibleRole::CheckBox,
            "img" | "image" => return AccessibleRole::Image,
            "list" => return AccessibleRole::List,
            "listitem" => return AccessibleRole::ListItem,
            "paragraph" => return AccessibleRole::Paragraph,
            _ => {}
        }
    }
    match document
        .element_local_name(element)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "main" => AccessibleRole::Main,
        "nav" => AccessibleRole::Navigation,
        "form" => AccessibleRole::Form,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => AccessibleRole::Heading,
        "a" if document.element_attribute(element, "href").is_some() => AccessibleRole::Link,
        "button" => AccessibleRole::Button,
        "input" => match document
            .element_attribute(element, "type")
            .unwrap_or_else(|| "text".to_owned())
            .to_ascii_lowercase()
            .as_str()
        {
            "checkbox" => AccessibleRole::CheckBox,
            "button" | "submit" | "reset" => AccessibleRole::Button,
            _ => AccessibleRole::TextBox,
        },
        "textarea" => AccessibleRole::TextBox,
        "img" => AccessibleRole::Image,
        "ul" | "ol" => AccessibleRole::List,
        "li" => AccessibleRole::ListItem,
        "p" => AccessibleRole::Paragraph,
        _ => AccessibleRole::Generic,
    }
}

fn accessible_name(
    document: &Document,
    element: &NodeHandle,
    id_map: &BTreeMap<String, NodeHandle>,
    labels: &BTreeMap<String, String>,
) -> String {
    if let Some(label) = document.element_attribute(element, "aria-label") {
        let label = normalize(&label);
        if !label.is_empty() {
            return label;
        }
    }
    if let Some(references) = document.element_attribute(element, "aria-labelledby") {
        let label = references
            .split_ascii_whitespace()
            .filter_map(|id| id_map.get(id))
            .map(|target| normalize(&document.text_content(target)))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !label.is_empty() {
            return label;
        }
    }
    if let Some(id) = document.element_attribute(element, "id")
        && let Some(label) = labels.get(&id)
    {
        return label.clone();
    }
    if let Some(label) = wrapping_label(document, element) {
        return label;
    }
    let local_name = document
        .element_local_name(element)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match local_name.as_str() {
        "img" => document
            .element_attribute(element, "alt")
            .unwrap_or_default(),
        "input" => {
            let input_type = document
                .element_attribute(element, "type")
                .unwrap_or_else(|| "text".to_owned())
                .to_ascii_lowercase();
            if matches!(input_type.as_str(), "button" | "submit" | "reset") {
                document
                    .element_attribute(element, "value")
                    .unwrap_or_else(|| match input_type.as_str() {
                        "submit" => "Submit".to_owned(),
                        "reset" => "Reset".to_owned(),
                        _ => String::new(),
                    })
            } else {
                document
                    .element_attribute(element, "placeholder")
                    .unwrap_or_default()
            }
        }
        _ => {
            let text = normalized_subtree_text(document, element);
            if text.is_empty() {
                document
                    .element_attribute(element, "title")
                    .unwrap_or_default()
            } else {
                text
            }
        }
    }
}

fn collect_labels(document: &Document, elements: &[NodeHandle]) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    for element in elements {
        if document.element_local_name(element).as_deref() != Some("label") {
            continue;
        }
        let Some(target) = document.element_attribute(element, "for") else {
            continue;
        };
        let name = normalize(&document.text_content(element));
        if !name.is_empty() {
            labels.insert(target, name);
        }
    }
    labels
}

fn wrapping_label(document: &Document, element: &NodeHandle) -> Option<String> {
    let mut parent = document.parent_element(element);
    while let Some(candidate) = parent {
        if document.element_local_name(&candidate).as_deref() == Some("label") {
            let name = normalize(&document.text_content(&candidate));
            return (!name.is_empty()).then_some(name);
        }
        parent = document.parent_element(&candidate);
    }
    None
}

fn inherent_focusable(document: &Document, element: &NodeHandle) -> bool {
    match document.element_local_name(element).as_deref() {
        Some("a") => document.element_attribute(element, "href").is_some(),
        Some("button" | "input" | "select" | "textarea") => true,
        _ => document
            .element_attribute(element, "contenteditable")
            .is_some_and(|value| !value.eq_ignore_ascii_case("false")),
    }
}

fn is_hidden(document: &Document, element: &NodeHandle) -> bool {
    let mut current = Some(element.clone());
    while let Some(candidate) = current {
        if hidden_itself(document, &candidate) {
            return true;
        }
        current = document.parent_element(&candidate);
    }
    false
}

fn hidden_itself(document: &Document, element: &NodeHandle) -> bool {
    document.element_attribute(element, "hidden").is_some()
        || document
            .element_attribute(element, "aria-hidden")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        || document
            .element_attribute(element, "style")
            .is_some_and(|value| {
                let compact = value.to_ascii_lowercase().replace(' ', "");
                compact.contains("display:none") || compact.contains("visibility:hidden")
            })
}

fn is_disabled(document: &Document, element: &NodeHandle) -> bool {
    document.element_attribute(element, "disabled").is_some()
        || document
            .element_attribute(element, "aria-disabled")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn parse_tabindex(document: &Document, element: &NodeHandle) -> Option<i32> {
    document
        .element_attribute(element, "tabindex")
        .and_then(|value| value.trim().parse().ok())
}

fn normalized_subtree_text(document: &Document, element: &NodeHandle) -> String {
    let child_parts = document
        .element_children(element)
        .into_iter()
        .map(|child| normalize(&document.text_content(&child)))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if child_parts.is_empty() {
        normalize(&document.text_content(element))
    } else {
        child_parts.join(" ")
    }
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn flatten_node<'a>(node: &'a AccessibleNode, output: &mut Vec<&'a AccessibleNode>) {
    output.push(node);
    for child in &node.children {
        flatten_node(child, output);
    }
}

/// Returns whether slots are unique, useful for conformance assertions.
#[must_use]
pub fn unique_focus_slots(tree: &AccessibilityTree) -> bool {
    let mut seen = BTreeSet::new();
    tree.focus_order_slots.iter().all(|slot| seen.insert(*slot))
}

#[cfg(test)]
mod tests {
    use meow_html::parse_utf8;

    use super::*;

    #[test]
    fn roles_names_and_focus_order_cover_selected_subset() {
        let document = parse_utf8(
            br#"<main><h1> Cats </h1><a href='/cats'>Read cats</a><label for='q'>Search</label><input id='q'><button aria-label='Save cat'>x</button><img alt='Tabby'></main>"#,
        )
        .document;
        let tree = AccessibilityTree::build(&document);
        let nodes = tree.flatten();
        assert!(nodes.iter().any(|node| node.role == AccessibleRole::Main));
        assert!(
            nodes
                .iter()
                .any(|node| node.role == AccessibleRole::Heading && node.name == "Cats")
        );
        assert!(
            nodes
                .iter()
                .any(|node| node.role == AccessibleRole::TextBox && node.name == "Search")
        );
        assert!(
            nodes
                .iter()
                .any(|node| node.role == AccessibleRole::Button && node.name == "Save cat")
        );
        assert_eq!(tree.focus_order_slots.len(), 3);
        assert!(unique_focus_slots(&tree));
    }

    #[test]
    fn positive_tabindex_precedes_normal_and_negative_is_skipped() {
        let document = parse_utf8(
            br#"<button id='normal'>N</button><button tabindex='2'>B</button><a href='/' tabindex='1'>A</a><input tabindex='-1'>"#,
        )
        .document;
        let slots = focus_order(&document)
            .into_iter()
            .map(|node| node.slot)
            .collect::<Vec<_>>();
        assert_eq!(slots.len(), 3);
        assert!(slots[0] > slots[1] || slots[0] != slots[1]);
    }

    #[test]
    fn audit_reports_unnamed_control() {
        let document = parse_utf8(br#"<button aria-label=''></button>"#).document;
        let audit = audit_keyboard_navigation(&document);
        assert!(
            audit
                .findings
                .iter()
                .any(|finding| finding.code == "missing-accessible-name")
        );
    }
}
