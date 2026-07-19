use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use meow_css::{
    ALL_PROPERTIES, ComputedValue, PropertyId, Stylesheet, W11_SNAPSHOT_PROPERTIES,
    W12_SNAPSHOT_PROPERTIES,
};
use meow_html::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CascadeOrigin {
    UserAgent,
    User,
    Author,
}

#[derive(Clone, Copy, Debug)]
pub struct CascadeStylesheet<'a> {
    pub origin: CascadeOrigin,
    pub stylesheet: &'a Stylesheet,
}

impl<'a> CascadeStylesheet<'a> {
    #[must_use]
    pub const fn new(origin: CascadeOrigin, stylesheet: &'a Stylesheet) -> Self {
        Self { origin, stylesheet }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComputedStyle {
    pub(super) values: BTreeMap<PropertyId, String>,
    pub(super) typed_values: BTreeMap<PropertyId, ComputedValue>,
    pub(super) custom_properties: BTreeMap<String, String>,
}

impl ComputedStyle {
    #[must_use]
    pub fn get(&self, property: PropertyId) -> &str {
        self.values
            .get(&property)
            .map(String::as_str)
            .expect("every supported property has a computed value")
    }

    #[must_use]
    pub fn typed(&self, property: PropertyId) -> &ComputedValue {
        self.typed_values
            .get(&property)
            .expect("every supported property has a typed computed value")
    }

    #[must_use]
    pub fn custom_property(&self, name: &str) -> Option<&str> {
        self.custom_properties.get(name).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (PropertyId, &str)> {
        ALL_PROPERTIES
            .into_iter()
            .map(|property| (property, self.get(property)))
    }

    pub fn custom_properties(&self) -> impl Iterator<Item = (&str, &str)> {
        self.custom_properties
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub(super) fn inherited_inputs_equal(&self, other: &Self) -> bool {
        self.custom_properties == other.custom_properties
            && ALL_PROPERTIES
                .into_iter()
                .filter(|property| property.inherited())
                .all(|property| self.typed(property) == other.typed(property))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputedElementStyle {
    pub node: NodeId,
    pub local_name: String,
    pub element_id: Option<String>,
    pub generation: u64,
    pub style: Arc<ComputedStyle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleDiagnostic {
    pub stylesheet_index: usize,
    pub rule_source_order: usize,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDiagnostic {
    pub node: NodeId,
    pub property: Option<PropertyId>,
    pub custom_property: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleSharingMetrics {
    pub hits: u64,
    pub misses: u64,
    pub unique_styles: usize,
    pub shared_elements: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputedStyleSnapshot {
    pub(super) elements: Vec<ComputedElementStyle>,
    pub(super) diagnostics: Vec<StyleDiagnostic>,
    pub(super) value_diagnostics: Vec<ValueDiagnostic>,
    pub(super) sharing_metrics: StyleSharingMetrics,
}

impl ComputedStyleSnapshot {
    #[must_use]
    pub fn elements(&self) -> &[ComputedElementStyle] {
        &self.elements
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[StyleDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn value_diagnostics(&self) -> &[ValueDiagnostic] {
        &self.value_diagnostics
    }

    #[must_use]
    pub const fn sharing_metrics(&self) -> StyleSharingMetrics {
        self.sharing_metrics
    }

    #[must_use]
    pub fn style_for(&self, node: NodeId) -> Option<&ComputedStyle> {
        self.elements
            .iter()
            .find(|entry| entry.node == node)
            .map(|entry| entry.style.as_ref())
    }

    /// Legacy W11 snapshot, intentionally limited to the original 13 properties.
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
            for property in W11_SNAPSHOT_PROPERTIES {
                writeln!(
                    output,
                    "  {}={:?}",
                    property.name(),
                    element.style.get(property)
                )
                .expect("writing to String cannot fail");
            }
        }
        write_diagnostics(&mut output, &self.diagnostics, &self.value_diagnostics);
        output
    }

    /// W12 snapshot with typed kinds, box properties, custom properties, and generations.
    #[must_use]
    pub fn dump_typed(&self) -> String {
        let mut output = String::new();
        for element in &self.elements {
            writeln!(
                output,
                "element slot={} name={:?} id={:?} generation={}",
                element.node.slot, element.local_name, element.element_id, element.generation
            )
            .expect("writing to String cannot fail");
            for property in W12_SNAPSHOT_PROPERTIES {
                writeln!(
                    output,
                    "  {} kind={} value={:?}",
                    property.name(),
                    element.style.typed(property).kind_name(),
                    element.style.get(property)
                )
                .expect("writing to String cannot fail");
            }
            for (name, value) in element.style.custom_properties() {
                writeln!(output, "  custom {name:?}={value:?}")
                    .expect("writing to String cannot fail");
            }
        }
        write_diagnostics(&mut output, &self.diagnostics, &self.value_diagnostics);
        output
    }
}

fn write_diagnostics(
    output: &mut String,
    diagnostics: &[StyleDiagnostic],
    value_diagnostics: &[ValueDiagnostic],
) {
    for diagnostic in diagnostics {
        writeln!(
            output,
            "style-error sheet={} rule={} message={:?}",
            diagnostic.stylesheet_index, diagnostic.rule_source_order, diagnostic.message
        )
        .expect("writing to String cannot fail");
    }
    for diagnostic in value_diagnostics {
        writeln!(
            output,
            "value-error slot={} property={:?} custom={:?} message={:?}",
            diagnostic.node.slot,
            diagnostic.property.map(PropertyId::name),
            diagnostic.custom_property,
            diagnostic.message
        )
        .expect("writing to String cannot fail");
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirtyFlag {
    #[default]
    Clean,
    SelfOnly,
    Subtree,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InvalidationReport {
    pub roots: Vec<NodeId>,
    pub dirty_nodes: Vec<NodeId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestyleReport {
    pub generation: u64,
    pub restyled_nodes: Vec<NodeId>,
    pub changed_nodes: Vec<NodeId>,
}
