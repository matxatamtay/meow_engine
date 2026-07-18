use std::collections::BTreeMap;

use meow_css::{
    ALL_PROPERTIES, Combinator, ComputedValue, CssWideKeyword, Declaration, PropertyId,
    PseudoClass, Rule, SelectorList, SimpleSelector, Specificity, SpecifiedValue,
    parse_computed_value, parse_css_wide_keyword, parse_property_declarations,
};
use meow_html::{Document, NodeHandle, NodeId};

use super::{
    model::{CascadeOrigin, CascadeStylesheet, ComputedStyle, StyleDiagnostic, ValueDiagnostic},
    variables::{resolve_custom_properties, substitute_vars},
};

pub(super) struct PreparedRule<'a> {
    origin: CascadeOrigin,
    stylesheet_order: usize,
    rule_source_order: usize,
    selectors: SelectorList,
    declarations: &'a [Declaration],
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SelectorDependencies {
    pub ancestor_combinator: bool,
    pub next_sibling_combinator: bool,
    pub subsequent_sibling_combinator: bool,
    pub empty_pseudo: bool,
    pub structural_pseudo: bool,
}

pub(super) struct PreparedStyleSet<'a> {
    pub rules: Vec<PreparedRule<'a>>,
    pub diagnostics: Vec<StyleDiagnostic>,
    pub dependencies: SelectorDependencies,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CascadePriority {
    origin_and_importance: u8,
    specificity: Specificity,
    stylesheet_order: usize,
    rule_source_order: usize,
    declaration_order: usize,
}

struct Winner<T> {
    priority: CascadePriority,
    value: T,
}

struct ElementWinners {
    properties: BTreeMap<PropertyId, Winner<SpecifiedValue>>,
    custom_properties: BTreeMap<String, Winner<String>>,
}

pub(super) struct ElementComputation {
    pub style: ComputedStyle,
    pub diagnostics: Vec<ValueDiagnostic>,
}

pub(super) fn prepare_styles<'a>(stylesheets: &'a [CascadeStylesheet<'a>]) -> PreparedStyleSet<'a> {
    let mut rules = Vec::new();
    let mut diagnostics = Vec::new();
    let mut dependencies = SelectorDependencies::default();
    for (stylesheet_order, input) in stylesheets.iter().enumerate() {
        for rule in &input.stylesheet.rules {
            let Rule::Style(rule) = rule else {
                continue;
            };
            match rule.selector_list() {
                Ok(selectors) => {
                    update_dependencies(&selectors, &mut dependencies);
                    rules.push(PreparedRule {
                        origin: input.origin,
                        stylesheet_order,
                        rule_source_order: rule.source_order,
                        selectors,
                        declarations: &rule.declarations,
                    });
                }
                Err(error) => diagnostics.push(StyleDiagnostic {
                    stylesheet_index: stylesheet_order,
                    rule_source_order: rule.source_order,
                    message: error.to_string(),
                }),
            }
        }
    }
    PreparedStyleSet {
        rules,
        diagnostics,
        dependencies,
    }
}

pub(super) fn compute_element_style(
    document: &Document,
    element: &NodeHandle,
    parent: Option<&ComputedStyle>,
    prepared: &PreparedStyleSet<'_>,
) -> ElementComputation {
    let winners = cascade_for_element(document, element, &prepared.rules);
    resolve_computed_style(element.id(), parent, winners)
}

fn cascade_for_element(
    document: &Document,
    element: &NodeHandle,
    rules: &[PreparedRule<'_>],
) -> ElementWinners {
    let mut properties = BTreeMap::<PropertyId, Winner<SpecifiedValue>>::new();
    let mut custom_properties = BTreeMap::<String, Winner<String>>::new();
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
            if declaration.name.starts_with("--") {
                let value = declaration.value.trim();
                if !value.is_empty() {
                    insert_winner(
                        &mut custom_properties,
                        declaration.name.clone(),
                        Winner {
                            priority,
                            value: value.to_owned(),
                        },
                    );
                }
                continue;
            }
            for declaration in parse_property_declarations(declaration) {
                insert_winner(
                    &mut properties,
                    declaration.property,
                    Winner {
                        priority: priority.clone(),
                        value: declaration.value,
                    },
                );
            }
        }
    }
    ElementWinners {
        properties,
        custom_properties,
    }
}

fn insert_winner<K: Ord, T>(map: &mut BTreeMap<K, Winner<T>>, key: K, candidate: Winner<T>) {
    let replace = map
        .get(&key)
        .is_none_or(|winner| candidate.priority >= winner.priority);
    if replace {
        map.insert(key, candidate);
    }
}

fn resolve_computed_style(
    node: NodeId,
    parent: Option<&ComputedStyle>,
    winners: ElementWinners,
) -> ElementComputation {
    let custom_winners = winners
        .custom_properties
        .into_iter()
        .map(|(name, winner)| (name, winner.value))
        .collect::<BTreeMap<_, _>>();
    let (custom_properties, custom_errors) = resolve_custom_properties(
        parent.map(|style| &style.custom_properties),
        &custom_winners,
    );
    let mut diagnostics = custom_errors
        .into_iter()
        .map(|(name, error)| ValueDiagnostic {
            node,
            property: None,
            custom_property: Some(name),
            message: error.to_string(),
        })
        .collect::<Vec<_>>();
    let mut values = BTreeMap::new();
    let mut typed_values = BTreeMap::new();

    for property in ALL_PROPERTIES {
        let resolved = match winners
            .properties
            .get(&property)
            .map(|winner| &winner.value)
        {
            Some(SpecifiedValue::CssWide(keyword)) => resolve_css_wide(property, *keyword, parent),
            Some(SpecifiedValue::Value(source)) => {
                match substitute_vars(source, &custom_properties) {
                    Ok(source) => {
                        if let Some(keyword) = parse_css_wide_keyword(&source) {
                            resolve_css_wide(property, keyword, parent)
                        } else if let Some(value) = parse_computed_value(property, &source) {
                            resolve_current_color(property, value, parent, &typed_values)
                        } else {
                            diagnostics.push(ValueDiagnostic {
                                node,
                                property: Some(property),
                                custom_property: None,
                                message: format!("invalid computed value {source:?}"),
                            });
                            resolve_unset(property, parent)
                        }
                    }
                    Err(error) => {
                        diagnostics.push(ValueDiagnostic {
                            node,
                            property: Some(property),
                            custom_property: None,
                            message: error.to_string(),
                        });
                        resolve_unset(property, parent)
                    }
                }
            }
            None => resolve_unset(property, parent),
        };
        values.insert(property, resolved.to_css());
        typed_values.insert(property, resolved);
    }

    ElementComputation {
        style: ComputedStyle {
            values,
            typed_values,
            custom_properties,
        },
        diagnostics,
    }
}

fn resolve_css_wide(
    property: PropertyId,
    keyword: CssWideKeyword,
    parent: Option<&ComputedStyle>,
) -> ComputedValue {
    match keyword {
        CssWideKeyword::Initial => initial_value(property),
        CssWideKeyword::Inherit => inherited_or_initial(property, parent),
        CssWideKeyword::Unset => resolve_unset(property, parent),
    }
}

fn resolve_unset(property: PropertyId, parent: Option<&ComputedStyle>) -> ComputedValue {
    if property.inherited() {
        inherited_or_initial(property, parent)
    } else {
        initial_value(property)
    }
}

fn inherited_or_initial(property: PropertyId, parent: Option<&ComputedStyle>) -> ComputedValue {
    parent
        .map(|style| style.typed(property).clone())
        .unwrap_or_else(|| initial_value(property))
}

fn initial_value(property: PropertyId) -> ComputedValue {
    parse_computed_value(property, property.initial_value())
        .expect("every property initial value must parse")
}

fn resolve_current_color(
    property: PropertyId,
    value: ComputedValue,
    parent: Option<&ComputedStyle>,
    current_values: &BTreeMap<PropertyId, ComputedValue>,
) -> ComputedValue {
    if !matches!(
        value,
        ComputedValue::Color(meow_css::ColorValue::CurrentColor)
    ) {
        return value;
    }
    if property == PropertyId::Color {
        inherited_or_initial(PropertyId::Color, parent)
    } else {
        current_values
            .get(&PropertyId::Color)
            .cloned()
            .unwrap_or_else(|| initial_value(PropertyId::Color))
    }
}

fn update_dependencies(selectors: &SelectorList, dependencies: &mut SelectorDependencies) {
    for selector in &selectors.selectors {
        for segment in &selector.segments {
            match segment.combinator {
                Some(Combinator::Child | Combinator::Descendant) => {
                    dependencies.ancestor_combinator = true;
                }
                Some(Combinator::NextSibling) => {
                    dependencies.next_sibling_combinator = true;
                }
                Some(Combinator::SubsequentSibling) => {
                    dependencies.subsequent_sibling_combinator = true;
                }
                None => {}
            }
            for simple in &segment.compound.simple_selectors {
                let SimpleSelector::PseudoClass(pseudo) = simple else {
                    continue;
                };
                match pseudo {
                    PseudoClass::Empty => dependencies.empty_pseudo = true,
                    PseudoClass::FirstChild
                    | PseudoClass::LastChild
                    | PseudoClass::OnlyChild
                    | PseudoClass::NthChild(_)
                    | PseudoClass::NthLastChild(_)
                    | PseudoClass::FirstOfType
                    | PseudoClass::LastOfType
                    | PseudoClass::OnlyOfType
                    | PseudoClass::NthOfType(_)
                    | PseudoClass::NthLastOfType(_) => dependencies.structural_pseudo = true,
                    PseudoClass::Root => {}
                }
            }
        }
    }
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
