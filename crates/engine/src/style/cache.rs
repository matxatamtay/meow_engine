use std::collections::{BTreeMap, BTreeSet};

use meow_html::{Document, DomMutation, DomMutationKind, NodeId};

use super::{
    cascade::{PreparedStyleSet, compute_element_style, prepare_styles},
    model::{
        CascadeStylesheet, ComputedElementStyle, ComputedStyle, ComputedStyleSnapshot, DirtyFlag,
        InvalidationReport, RestyleReport, ValueDiagnostic,
    },
};

struct CachedStyle {
    style: ComputedStyle,
    generation: u64,
}

pub struct StyleEngine<'a> {
    document: &'a Document,
    prepared: PreparedStyleSet<'a>,
    cache: BTreeMap<NodeId, CachedStyle>,
    dirty: BTreeMap<NodeId, DirtyFlag>,
    value_diagnostics: BTreeMap<NodeId, Vec<ValueDiagnostic>>,
    generation: u64,
}

impl<'a> StyleEngine<'a> {
    #[must_use]
    pub fn new(document: &'a Document, stylesheets: &'a [CascadeStylesheet<'a>]) -> Self {
        let mut engine = Self {
            document,
            prepared: prepare_styles(stylesheets),
            cache: BTreeMap::new(),
            dirty: BTreeMap::new(),
            value_diagnostics: BTreeMap::new(),
            generation: 1,
        };
        engine.compute_initial_styles();
        engine
    }

    #[must_use]
    pub fn style_for(&self, node: NodeId) -> Option<&ComputedStyle> {
        self.cache.get(&node).map(|entry| &entry.style)
    }

    #[must_use]
    pub fn style_generation(&self, node: NodeId) -> Option<u64> {
        self.cache.get(&node).map(|entry| entry.generation)
    }

    #[must_use]
    pub fn dirty_flag(&self, node: NodeId) -> DirtyFlag {
        self.dirty.get(&node).copied().unwrap_or_default()
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn snapshot(&self) -> ComputedStyleSnapshot {
        let elements = self
            .document
            .elements_in_tree_order()
            .into_iter()
            .filter_map(|element| {
                let cached = self.cache.get(&element.id())?;
                Some(ComputedElementStyle {
                    node: element.id(),
                    local_name: self.document.element_local_name(&element)?,
                    element_id: self.document.element_attribute(&element, "id"),
                    generation: cached.generation,
                    style: cached.style.clone(),
                })
            })
            .collect();
        let value_diagnostics = self
            .value_diagnostics
            .values()
            .flat_map(|items| items.iter().cloned())
            .collect();
        ComputedStyleSnapshot {
            elements,
            diagnostics: self.prepared.diagnostics.clone(),
            value_diagnostics,
        }
    }

    pub fn invalidate(&mut self, mutation: &DomMutation) -> InvalidationReport {
        for removed in &mutation.removed_nodes {
            self.cache.remove(removed);
            self.dirty.remove(removed);
            self.value_diagnostics.remove(removed);
        }
        let Some(target) = self.document.element_by_id(mutation.target) else {
            return InvalidationReport::default();
        };
        let mut roots = BTreeMap::new();
        match mutation.kind {
            DomMutationKind::Attributes => {
                let flag = if self.prepared.dependencies.ancestor_combinator {
                    DirtyFlag::Subtree
                } else {
                    DirtyFlag::SelfOnly
                };
                roots.insert(target.id(), flag);
                if self.prepared.dependencies.subsequent_sibling_combinator {
                    for sibling in self.document.following_element_siblings(&target) {
                        roots.insert(sibling.id(), DirtyFlag::Subtree);
                    }
                } else if self.prepared.dependencies.next_sibling_combinator
                    && let Some(sibling) = self.document.next_element_sibling(&target)
                {
                    roots.insert(sibling.id(), DirtyFlag::Subtree);
                }
            }
            DomMutationKind::CharacterData => {
                if self.prepared.dependencies.empty_pseudo {
                    roots.insert(target.id(), DirtyFlag::SelfOnly);
                }
            }
            DomMutationKind::ChildList => {
                let sibling_sensitive = self.prepared.dependencies.next_sibling_combinator
                    || self.prepared.dependencies.subsequent_sibling_combinator;
                if self.prepared.dependencies.structural_pseudo || sibling_sensitive {
                    roots.insert(target.id(), DirtyFlag::Subtree);
                } else {
                    if self.prepared.dependencies.empty_pseudo {
                        roots.insert(target.id(), DirtyFlag::SelfOnly);
                    }
                    for added in &mutation.added_nodes {
                        if let Some(element) = self.document.element_by_id(*added) {
                            roots.insert(element.id(), DirtyFlag::Subtree);
                        }
                    }
                }
            }
        }
        for (root, flag) in &roots {
            if let Some(element) = self.document.element_by_id(*root) {
                self.mark_dirty(&element, *flag);
            }
        }
        InvalidationReport {
            roots: roots.keys().copied().collect(),
            dirty_nodes: self.dirty.keys().copied().collect(),
        }
    }

    pub fn restyle_dirty(&mut self) -> RestyleReport {
        if self.dirty.is_empty() {
            return RestyleReport {
                generation: self.generation,
                ..RestyleReport::default()
            };
        }
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        let mut pending = std::mem::take(&mut self.dirty);
        let elements = self.document.elements_in_tree_order();
        let mut restyled_nodes = Vec::new();
        let mut changed_nodes = Vec::new();

        for element in &elements {
            if pending.get(&element.id()).copied().unwrap_or_default() == DirtyFlag::Clean {
                continue;
            }
            let parent_style = self
                .document
                .parent_element(element)
                .and_then(|parent| self.cache.get(&parent.id()))
                .map(|entry| &entry.style);
            let computation =
                compute_element_style(self.document, element, parent_style, &self.prepared);
            let old = self
                .cache
                .get(&element.id())
                .map(|entry| entry.style.clone());
            let inherited_changed = old
                .as_ref()
                .is_none_or(|style| !style.inherited_inputs_equal(&computation.style));
            if old.as_ref() != Some(&computation.style) {
                changed_nodes.push(element.id());
            }
            self.cache.insert(
                element.id(),
                CachedStyle {
                    style: computation.style,
                    generation,
                },
            );
            self.value_diagnostics
                .insert(element.id(), computation.diagnostics);
            restyled_nodes.push(element.id());
            if inherited_changed {
                for descendant in self.document.element_subtree(element).into_iter().skip(1) {
                    pending
                        .entry(descendant.id())
                        .and_modify(|flag| *flag = (*flag).max(DirtyFlag::SelfOnly))
                        .or_insert(DirtyFlag::SelfOnly);
                }
            }
        }

        let connected = elements
            .iter()
            .map(|element| element.id())
            .collect::<BTreeSet<_>>();
        self.cache.retain(|node, _| connected.contains(node));
        self.value_diagnostics
            .retain(|node, _| connected.contains(node));

        RestyleReport {
            generation,
            restyled_nodes,
            changed_nodes,
        }
    }

    fn compute_initial_styles(&mut self) {
        for element in self.document.elements_in_tree_order() {
            let parent_style = self
                .document
                .parent_element(&element)
                .and_then(|parent| self.cache.get(&parent.id()))
                .map(|entry| &entry.style);
            let computation =
                compute_element_style(self.document, &element, parent_style, &self.prepared);
            self.cache.insert(
                element.id(),
                CachedStyle {
                    style: computation.style,
                    generation: self.generation,
                },
            );
            self.value_diagnostics
                .insert(element.id(), computation.diagnostics);
        }
    }

    fn mark_dirty(&mut self, root: &meow_html::NodeHandle, flag: DirtyFlag) {
        self.dirty
            .entry(root.id())
            .and_modify(|current| *current = (*current).max(flag))
            .or_insert(flag);
        if flag == DirtyFlag::Subtree {
            for descendant in self.document.element_subtree(root).into_iter().skip(1) {
                self.dirty
                    .entry(descendant.id())
                    .and_modify(|current| *current = (*current).max(DirtyFlag::SelfOnly))
                    .or_insert(DirtyFlag::SelfOnly);
            }
        }
    }
}

#[must_use]
pub fn compute_styles(
    document: &Document,
    stylesheets: &[CascadeStylesheet<'_>],
) -> ComputedStyleSnapshot {
    StyleEngine::new(document, stylesheets).snapshot()
}
