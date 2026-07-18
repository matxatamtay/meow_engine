//! Selector matching and query APIs for the HTML DOM arena.

use html5ever::Attribute;
use meow_css::{
    AttributeCaseSensitivity, AttributeMatcher, AttributeSelector, Combinator, ComplexSelector,
    CompoundSelector, PseudoClass, SelectorList, SimpleSelector, TypeSelector,
};

use super::{Document, DomState, NodeHandle, NodeKind, attribute_value, node};

impl Document {
    /// Returns one attribute value from an element, using HTML ASCII-insensitive names.
    #[must_use]
    pub fn element_attribute(&self, element: &NodeHandle, local_name: &str) -> Option<String> {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        let NodeKind::Element { attrs, .. } = &node(&state, element).kind else {
            return None;
        };
        attribute_value(attrs, local_name)
    }

    /// Returns whether an element matches one complex selector.
    #[must_use]
    pub fn matches_selector(&self, element: &NodeHandle, selector: &ComplexSelector) -> bool {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        if selector.segments.is_empty() || !is_element(&state, element) {
            return false;
        }
        matches_complex_selector(&state, element, selector, selector.segments.len() - 1)
    }

    /// Returns whether an element matches any selector in a selector list.
    #[must_use]
    pub fn matches_selector_list(&self, element: &NodeHandle, selectors: &SelectorList) -> bool {
        self.assert_same_document(element);
        let state = self.inner.state.borrow();
        is_element(&state, element)
            && selectors.selectors.iter().any(|selector| {
                !selector.segments.is_empty()
                    && matches_complex_selector(
                        &state,
                        element,
                        selector,
                        selector.segments.len() - 1,
                    )
            })
    }

    /// Returns the first matching element in document tree order.
    #[must_use]
    pub fn query_selector(&self, selectors: &SelectorList) -> Option<NodeHandle> {
        let state = self.inner.state.borrow();
        find_first_matching_element(&state, &self.root, selectors)
    }

    /// Returns all matching elements in document tree order without duplicates.
    #[must_use]
    pub fn query_selector_all(&self, selectors: &SelectorList) -> Vec<NodeHandle> {
        let state = self.inner.state.borrow();
        let mut matches = Vec::new();
        collect_matching_elements(&state, &self.root, selectors, &mut matches);
        matches
    }
}

fn find_first_matching_element(
    state: &DomState,
    handle: &NodeHandle,
    selectors: &SelectorList,
) -> Option<NodeHandle> {
    if is_element(state, handle)
        && selectors.selectors.iter().any(|selector| {
            !selector.segments.is_empty()
                && matches_complex_selector(state, handle, selector, selector.segments.len() - 1)
        })
    {
        return Some(handle.clone());
    }
    node(state, handle)
        .children
        .iter()
        .find_map(|child| find_first_matching_element(state, child, selectors))
}

fn collect_matching_elements(
    state: &DomState,
    handle: &NodeHandle,
    selectors: &SelectorList,
    output: &mut Vec<NodeHandle>,
) {
    if is_element(state, handle)
        && selectors.selectors.iter().any(|selector| {
            !selector.segments.is_empty()
                && matches_complex_selector(state, handle, selector, selector.segments.len() - 1)
        })
    {
        output.push(handle.clone());
    }
    for child in &node(state, handle).children {
        collect_matching_elements(state, child, selectors, output);
    }
}

fn matches_complex_selector(
    state: &DomState,
    element: &NodeHandle,
    selector: &ComplexSelector,
    segment_index: usize,
) -> bool {
    let segment = &selector.segments[segment_index];
    if !matches_compound_selector(state, element, &segment.compound) {
        return false;
    }
    if segment_index == 0 {
        return true;
    }

    match segment
        .combinator
        .expect("non-first selector segments have a combinator")
    {
        Combinator::Child => parent_element(state, element).is_some_and(|parent| {
            matches_complex_selector(state, &parent, selector, segment_index - 1)
        }),
        Combinator::Descendant => {
            let mut current = node(state, element).parent.clone();
            while let Some(candidate) = current {
                if is_element(state, &candidate)
                    && matches_complex_selector(state, &candidate, selector, segment_index - 1)
                {
                    return true;
                }
                current = node(state, &candidate).parent.clone();
            }
            false
        }
        Combinator::NextSibling => {
            previous_element_sibling(state, element).is_some_and(|sibling| {
                matches_complex_selector(state, &sibling, selector, segment_index - 1)
            })
        }
        Combinator::SubsequentSibling => {
            let mut current = previous_element_sibling(state, element);
            while let Some(sibling) = current {
                if matches_complex_selector(state, &sibling, selector, segment_index - 1) {
                    return true;
                }
                current = previous_element_sibling(state, &sibling);
            }
            false
        }
    }
}

fn matches_compound_selector(
    state: &DomState,
    element: &NodeHandle,
    selector: &CompoundSelector,
) -> bool {
    let NodeKind::Element { name, attrs, .. } = &node(state, element).kind else {
        return false;
    };

    if let Some(type_selector) = &selector.type_selector {
        match type_selector {
            TypeSelector::Universal => {}
            TypeSelector::Type(expected) => {
                let matches = if name.ns.as_ref() == "http://www.w3.org/1999/xhtml" {
                    name.local.as_ref().eq_ignore_ascii_case(expected)
                } else {
                    name.local.as_ref() == expected
                };
                if !matches {
                    return false;
                }
            }
        }
    }

    selector.simple_selectors.iter().all(|simple| match simple {
        SimpleSelector::Id(expected) => attribute_value(attrs, "id").as_deref() == Some(expected),
        SimpleSelector::Class(expected) => attribute_value(attrs, "class").is_some_and(|classes| {
            classes
                .split_ascii_whitespace()
                .any(|class| class == expected)
        }),
        SimpleSelector::Attribute(attribute) => matches_attribute_selector(attrs, attribute),
        SimpleSelector::PseudoClass(pseudo) => matches_pseudo_class(state, element, pseudo),
    })
}

fn matches_attribute_selector(attrs: &[Attribute], selector: &AttributeSelector) -> bool {
    let Some(value) = attribute_value(attrs, &selector.name) else {
        return false;
    };
    let compare = |left: &str, right: &str| match selector.case_sensitivity {
        AttributeCaseSensitivity::AsciiInsensitive => left.eq_ignore_ascii_case(right),
        AttributeCaseSensitivity::Default | AttributeCaseSensitivity::CaseSensitive => {
            left == right
        }
    };

    match &selector.matcher {
        AttributeMatcher::Exists => true,
        AttributeMatcher::Equals(expected) => compare(&value, expected),
        AttributeMatcher::Includes(expected) => {
            !expected.is_empty()
                && value
                    .split_ascii_whitespace()
                    .any(|word| compare(word, expected))
        }
        AttributeMatcher::DashMatch(expected) => {
            compare(&value, expected)
                || starts_with(&value, expected, selector.case_sensitivity)
                    && value.as_bytes().get(expected.len()) == Some(&b'-')
        }
        AttributeMatcher::Prefix(expected) => {
            !expected.is_empty() && starts_with(&value, expected, selector.case_sensitivity)
        }
        AttributeMatcher::Suffix(expected) => {
            !expected.is_empty() && ends_with(&value, expected, selector.case_sensitivity)
        }
        AttributeMatcher::Substring(expected) => {
            !expected.is_empty() && contains(&value, expected, selector.case_sensitivity)
        }
    }
}

fn starts_with(value: &str, expected: &str, sensitivity: AttributeCaseSensitivity) -> bool {
    match sensitivity {
        AttributeCaseSensitivity::AsciiInsensitive => value
            .get(..expected.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected)),
        AttributeCaseSensitivity::Default | AttributeCaseSensitivity::CaseSensitive => {
            value.starts_with(expected)
        }
    }
}

fn ends_with(value: &str, expected: &str, sensitivity: AttributeCaseSensitivity) -> bool {
    match sensitivity {
        AttributeCaseSensitivity::AsciiInsensitive => value
            .get(value.len().saturating_sub(expected.len())..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(expected)),
        AttributeCaseSensitivity::Default | AttributeCaseSensitivity::CaseSensitive => {
            value.ends_with(expected)
        }
    }
}

fn contains(value: &str, expected: &str, sensitivity: AttributeCaseSensitivity) -> bool {
    match sensitivity {
        AttributeCaseSensitivity::AsciiInsensitive => value
            .as_bytes()
            .windows(expected.len())
            .any(|window| window.eq_ignore_ascii_case(expected.as_bytes())),
        AttributeCaseSensitivity::Default | AttributeCaseSensitivity::CaseSensitive => {
            value.contains(expected)
        }
    }
}

fn matches_pseudo_class(state: &DomState, element: &NodeHandle, pseudo: &PseudoClass) -> bool {
    match pseudo {
        PseudoClass::Root => node(state, element)
            .parent
            .as_ref()
            .is_some_and(|parent| matches!(node(state, parent).kind, NodeKind::Document)),
        PseudoClass::Empty => node(state, element).children.iter().all(|child| {
            !matches!(
                node(state, child).kind,
                NodeKind::Element { .. } | NodeKind::Text(_)
            )
        }),
        PseudoClass::FirstChild => element_index(state, element, false, false) == Some(1),
        PseudoClass::LastChild => element_index(state, element, true, false) == Some(1),
        PseudoClass::OnlyChild => {
            element_index(state, element, false, false) == Some(1)
                && element_index(state, element, true, false) == Some(1)
        }
        PseudoClass::NthChild(expression) => element_index(state, element, false, false)
            .is_some_and(|index| expression.matches(index)),
        PseudoClass::NthLastChild(expression) => element_index(state, element, true, false)
            .is_some_and(|index| expression.matches(index)),
        PseudoClass::FirstOfType => element_index(state, element, false, true) == Some(1),
        PseudoClass::LastOfType => element_index(state, element, true, true) == Some(1),
        PseudoClass::OnlyOfType => {
            element_index(state, element, false, true) == Some(1)
                && element_index(state, element, true, true) == Some(1)
        }
        PseudoClass::NthOfType(expression) => element_index(state, element, false, true)
            .is_some_and(|index| expression.matches(index)),
        PseudoClass::NthLastOfType(expression) => {
            element_index(state, element, true, true).is_some_and(|index| expression.matches(index))
        }
    }
}

fn element_index(
    state: &DomState,
    element: &NodeHandle,
    from_end: bool,
    of_type: bool,
) -> Option<usize> {
    let parent = node(state, element).parent.as_ref()?;
    let children = &node(state, parent).children;
    let mut index = 0;
    let iterator: Box<dyn Iterator<Item = &NodeHandle> + '_> = if from_end {
        Box::new(children.iter().rev())
    } else {
        Box::new(children.iter())
    };
    for child in iterator {
        if !is_element(state, child) {
            continue;
        }
        if of_type && !same_element_type(state, element, child) {
            continue;
        }
        index += 1;
        if child == element {
            return Some(index);
        }
    }
    None
}

fn same_element_type(state: &DomState, left: &NodeHandle, right: &NodeHandle) -> bool {
    let NodeKind::Element {
        name: left_name, ..
    } = &node(state, left).kind
    else {
        return false;
    };
    let NodeKind::Element {
        name: right_name, ..
    } = &node(state, right).kind
    else {
        return false;
    };
    left_name == right_name
}

fn parent_element(state: &DomState, element: &NodeHandle) -> Option<NodeHandle> {
    node(state, element)
        .parent
        .as_ref()
        .filter(|parent| is_element(state, parent))
        .cloned()
}

fn previous_element_sibling(state: &DomState, element: &NodeHandle) -> Option<NodeHandle> {
    let parent = node(state, element).parent.as_ref()?;
    let children = &node(state, parent).children;
    let index = children.iter().position(|candidate| candidate == element)?;
    children[..index]
        .iter()
        .rev()
        .find(|candidate| is_element(state, candidate))
        .cloned()
}

fn is_element(state: &DomState, handle: &NodeHandle) -> bool {
    matches!(node(state, handle).kind, NodeKind::Element { .. })
}
