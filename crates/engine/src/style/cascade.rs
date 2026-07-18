use std::collections::{BTreeMap, HashMap};

use meow_css::{
    ALL_PROPERTIES, CssWideKeyword, Declaration, PropertyId, Rule, SelectorList, Specificity,
    SpecifiedValue, parse_property_declaration,
};
use meow_html::{Document, NodeHandle, NodeId};

use super::model::{
    CascadeOrigin, CascadeStylesheet, ComputedElementStyle, ComputedStyle, ComputedStyleSnapshot,
    StyleDiagnostic,
};

struct PreparedRule<'a> {
    origin: CascadeOrigin,
    stylesheet_order: usize,
    rule_source_order: usize,
    selectors: SelectorList,
    declarations: &'a [Declaration],
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CascadePriority {
    origin_and_importance: u8,
    specificity: Specificity,
    stylesheet_order: usize,
    rule_source_order: usize,
    declaration_order: usize,
}

struct Winner {
    priority: CascadePriority,
    value: SpecifiedValue,
}

/// Computes styles for every element in tree order.
#[must_use]
pub fn compute_styles(
    document: &Document,
    stylesheets: &[CascadeStylesheet<'_>],
) -> ComputedStyleSnapshot {
    let (rules, diagnostics) = prepare_rules(stylesheets);
    let mut computed_by_node = HashMap::<NodeId, ComputedStyle>::new();
    let mut elements = Vec::new();

    for element in document.elements_in_tree_order() {
        let parent_style = document
            .parent_element(&element)
            .and_then(|parent| computed_by_node.get(&parent.id()));
        let winners = cascade_for_element(document, &element, &rules);
        let style = resolve_computed_style(parent_style, &winners);
        computed_by_node.insert(element.id(), style.clone());
        elements.push(ComputedElementStyle {
            node: element.id(),
            local_name: document
                .element_local_name(&element)
                .expect("tree-order traversal returns only elements"),
            element_id: document.element_attribute(&element, "id"),
            style,
        });
    }

    ComputedStyleSnapshot {
        elements,
        diagnostics,
    }
}

fn prepare_rules<'a>(
    stylesheets: &'a [CascadeStylesheet<'a>],
) -> (Vec<PreparedRule<'a>>, Vec<StyleDiagnostic>) {
    let mut rules = Vec::new();
    let mut diagnostics = Vec::new();

    for (stylesheet_order, input) in stylesheets.iter().enumerate() {
        for rule in &input.stylesheet.rules {
            let Rule::Style(rule) = rule else {
                continue;
            };
            match rule.selector_list() {
                Ok(selectors) => rules.push(PreparedRule {
                    origin: input.origin,
                    stylesheet_order,
                    rule_source_order: rule.source_order,
                    selectors,
                    declarations: &rule.declarations,
                }),
                Err(error) => diagnostics.push(StyleDiagnostic {
                    stylesheet_index: stylesheet_order,
                    rule_source_order: rule.source_order,
                    message: error.to_string(),
                }),
            }
        }
    }

    (rules, diagnostics)
}

fn cascade_for_element(
    document: &Document,
    element: &NodeHandle,
    rules: &[PreparedRule<'_>],
) -> BTreeMap<PropertyId, Winner> {
    let mut winners = BTreeMap::<PropertyId, Winner>::new();

    for rule in rules {
        let Some(specificity) = rule
            .selectors
            .selectors
            .iter()
            .filter(|selector| document.matches_selector(element, selector))
            .map(|selector| selector.specificity())
            .max()
        else {
            continue;
        };

        for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
            let Some(property_declaration) = parse_property_declaration(declaration) else {
                continue;
            };
            let priority = CascadePriority {
                origin_and_importance: origin_and_importance_rank(
                    rule.origin,
                    declaration.important,
                ),
                specificity,
                stylesheet_order: rule.stylesheet_order,
                rule_source_order: rule.rule_source_order,
                declaration_order,
            };
            let candidate = Winner {
                priority,
                value: property_declaration.value,
            };
            let replace = winners
                .get(&property_declaration.property)
                .is_none_or(|winner| candidate.priority >= winner.priority);
            if replace {
                winners.insert(property_declaration.property, candidate);
            }
        }
    }

    winners
}

fn resolve_computed_style(
    parent: Option<&ComputedStyle>,
    winners: &BTreeMap<PropertyId, Winner>,
) -> ComputedStyle {
    let mut values = BTreeMap::new();
    for property in ALL_PROPERTIES {
        let value = match winners.get(&property).map(|winner| &winner.value) {
            Some(SpecifiedValue::Value(value)) => value.clone(),
            Some(SpecifiedValue::CssWide(CssWideKeyword::Initial)) => {
                property.initial_value().to_owned()
            }
            Some(SpecifiedValue::CssWide(CssWideKeyword::Inherit)) => {
                inherited_or_initial(property, parent)
            }
            Some(SpecifiedValue::CssWide(CssWideKeyword::Unset)) | None if property.inherited() => {
                inherited_or_initial(property, parent)
            }
            Some(SpecifiedValue::CssWide(CssWideKeyword::Unset)) | None => {
                property.initial_value().to_owned()
            }
        };
        values.insert(property, value);
    }
    ComputedStyle { values }
}

fn inherited_or_initial(property: PropertyId, parent: Option<&ComputedStyle>) -> String {
    parent
        .map(|style| style.get(property).to_owned())
        .unwrap_or_else(|| property.initial_value().to_owned())
}

const fn origin_and_importance_rank(origin: CascadeOrigin, important: bool) -> u8 {
    match (important, origin) {
        (false, CascadeOrigin::UserAgent) => 0,
        (false, CascadeOrigin::User) => 1,
        (false, CascadeOrigin::Author) => 2,
        (true, CascadeOrigin::Author) => 3,
        (true, CascadeOrigin::User) => 4,
        (true, CascadeOrigin::UserAgent) => 5,
    }
}
