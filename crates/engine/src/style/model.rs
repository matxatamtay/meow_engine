use std::{collections::BTreeMap, fmt::Write as _};

use meow_css::{ALL_PROPERTIES, PropertyId, Stylesheet};
use meow_html::NodeId;

/// Cascade origin supported by W11.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CascadeOrigin {
    /// Built-in user-agent rules.
    UserAgent,
    /// User preference rules.
    User,
    /// Document author rules.
    Author,
}

/// One stylesheet paired with its cascade origin.
#[derive(Clone, Copy, Debug)]
pub struct CascadeStylesheet<'a> {
    /// Sheet origin.
    pub origin: CascadeOrigin,
    /// Parsed stylesheet.
    pub stylesheet: &'a Stylesheet,
}

impl<'a> CascadeStylesheet<'a> {
    /// Creates one cascade input sheet.
    #[must_use]
    pub const fn new(origin: CascadeOrigin, stylesheet: &'a Stylesheet) -> Self {
        Self { origin, stylesheet }
    }
}

/// Computed values for the complete W11 property subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputedStyle {
    pub(super) values: BTreeMap<PropertyId, String>,
}

impl ComputedStyle {
    /// Returns one computed property value.
    #[must_use]
    pub fn get(&self, property: PropertyId) -> &str {
        self.values
            .get(&property)
            .map(String::as_str)
            .expect("every W11 property has a computed value")
    }

    /// Iterates computed values in deterministic property order.
    pub fn iter(&self) -> impl Iterator<Item = (PropertyId, &str)> {
        ALL_PROPERTIES
            .into_iter()
            .map(|property| (property, self.get(property)))
    }
}

/// One element and its computed style in document tree order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputedElementStyle {
    /// Stable node identity.
    pub node: NodeId,
    /// Element local name.
    pub local_name: String,
    /// HTML `id` value, when present.
    pub element_id: Option<String>,
    /// Complete computed style.
    pub style: ComputedStyle,
}

/// Non-fatal style preparation diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleDiagnostic {
    /// Input stylesheet index.
    pub stylesheet_index: usize,
    /// Source-order index within the stylesheet.
    pub rule_source_order: usize,
    /// Human-readable failure reason.
    pub message: String,
}

/// Tree-ordered computed styles and ignored-rule diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputedStyleSnapshot {
    pub(super) elements: Vec<ComputedElementStyle>,
    pub(super) diagnostics: Vec<StyleDiagnostic>,
}

impl ComputedStyleSnapshot {
    /// Returns computed element records in document tree order.
    #[must_use]
    pub fn elements(&self) -> &[ComputedElementStyle] {
        &self.elements
    }

    /// Returns non-fatal style diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[StyleDiagnostic] {
        &self.diagnostics
    }

    /// Finds one computed style by stable node identity.
    #[must_use]
    pub fn style_for(&self, node: NodeId) -> Option<&ComputedStyle> {
        self.elements
            .iter()
            .find(|entry| entry.node == node)
            .map(|entry| &entry.style)
    }

    /// Produces a deterministic snapshot independent of process-global document IDs.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::new();
        for element in &self.elements {
            writeln!(
                output,
                "element slot={} name={:?} id={:?}",
                element.node.slot, element.local_name, element.element_id
            )
            .expect("writing to String cannot fail");
            for (property, value) in element.style.iter() {
                writeln!(output, "  {}={value:?}", property.name())
                    .expect("writing to String cannot fail");
            }
        }
        for diagnostic in &self.diagnostics {
            writeln!(
                output,
                "style-error sheet={} rule={} message={:?}",
                diagnostic.stylesheet_index, diagnostic.rule_source_order, diagnostic.message
            )
            .expect("writing to String cannot fail");
        }
        output
    }
}
