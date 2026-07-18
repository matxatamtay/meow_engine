use meow_css::{PropertyId, parse_selector_list, parse_stylesheet};
use meow_html::{Document, parse_utf8};

use super::*;

fn document(source: &str) -> Document {
    parse_utf8(source.as_bytes()).document
}

fn element(document: &Document, selector: &str) -> meow_html::NodeHandle {
    document
        .query_selector(&parse_selector_list(selector).expect("test selector should parse"))
        .expect("test element should exist")
}

#[test]
fn origin_and_important_order_follow_the_w11_ladder() {
    let document = document("<div id='target' class='card'></div>");
    let ua = parse_stylesheet("#target { color: navy !important; display: block; }");
    let user = parse_stylesheet(".card { color: green !important; display: grid; }");
    let author = parse_stylesheet("div { color: red !important; display: flex; }");
    let snapshot = compute_styles(
        &document,
        &[
            CascadeStylesheet::new(CascadeOrigin::UserAgent, &ua),
            CascadeStylesheet::new(CascadeOrigin::User, &user),
            CascadeStylesheet::new(CascadeOrigin::Author, &author),
        ],
    );
    let target = element(&document, "#target");
    let style = snapshot.style_for(target.id()).unwrap();

    assert_eq!(style.get(PropertyId::Color), "navy");
    assert_eq!(style.get(PropertyId::Display), "flex");
}

#[test]
fn specificity_stylesheet_rule_and_declaration_order_break_ties() {
    let document = document("<div id='target' class='card'></div>");
    let first =
        parse_stylesheet(".card { width: 1px; } #target { width: 2px; width: 3px; height: 1px; }");
    let second = parse_stylesheet("#target { width: 4px; } #target { height: 2px; }");
    let snapshot = compute_styles(
        &document,
        &[
            CascadeStylesheet::new(CascadeOrigin::Author, &first),
            CascadeStylesheet::new(CascadeOrigin::Author, &second),
        ],
    );
    let style = snapshot
        .style_for(element(&document, "#target").id())
        .unwrap();

    assert_eq!(style.get(PropertyId::Width), "4px");
    assert_eq!(style.get(PropertyId::Height), "2px");
}

#[test]
fn inheritance_initial_and_unset_resolve_parent_first() {
    let document = document("<main id='parent'><span id='child'><em id='leaf'></em></span></main>");
    let author = parse_stylesheet(
        "#parent { color: red; font-size: 20px; width: 10px; display: block; }
         #child { color: unset; font-size: initial; width: inherit; display: unset; }
         #leaf { color: initial; visibility: hidden; opacity: .5; }",
    );
    let snapshot = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &author)],
    );
    let child = snapshot
        .style_for(element(&document, "#child").id())
        .unwrap();
    let leaf = snapshot
        .style_for(element(&document, "#leaf").id())
        .unwrap();

    assert_eq!(child.get(PropertyId::Color), "red");
    assert_eq!(child.get(PropertyId::FontSize), "medium");
    assert_eq!(child.get(PropertyId::Width), "10px");
    assert_eq!(child.get(PropertyId::Display), "inline");
    assert_eq!(leaf.get(PropertyId::Color), "black");
    assert_eq!(leaf.get(PropertyId::FontSize), "medium");
    assert_eq!(leaf.get(PropertyId::Visibility), "hidden");
    assert_eq!(leaf.get(PropertyId::Opacity), ".5");
}

#[test]
fn unsupported_selectors_are_ignored_with_stable_diagnostics() {
    let document = document("<div id='target'></div>");
    let author = parse_stylesheet("div:hover { color: red; } #target { color: blue; }");
    let snapshot = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &author)],
    );

    assert_eq!(snapshot.diagnostics().len(), 1);
    assert!(
        snapshot.diagnostics()[0]
            .message
            .contains("unsupported pseudo-class")
    );
    assert_eq!(
        snapshot
            .style_for(element(&document, "#target").id())
            .unwrap()
            .get(PropertyId::Color),
        "blue"
    );
}

#[test]
fn typed_box_values_and_custom_properties_resolve_with_fallbacks() {
    let document = document("<main id='parent'><p id='child'></p><p id='cycle'></p></main>");
    let author = parse_stylesheet(
        "#parent {
            --space: 8px;
            --tone: #AbC;
            --family: serif;
            color: var(--tone);
            font-family: mèo var(--family);
            margin: var(--space);
            padding: 1px 2px 3px 4px;
            border-width: thin 2px;
            box-sizing: border-box;
         }
         #child {
            width: var(--missing, 12.500px);
            color: var(--tone);
         }
         #cycle {
            --a: var(--b);
            --b: var(--a);
            height: var(--a, 5px);
         }",
    );
    let snapshot = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &author)],
    );
    let parent = snapshot
        .style_for(element(&document, "#parent").id())
        .unwrap();
    let child = snapshot
        .style_for(element(&document, "#child").id())
        .unwrap();
    let cycle = snapshot
        .style_for(element(&document, "#cycle").id())
        .unwrap();

    assert_eq!(parent.get(PropertyId::Color), "#aabbcc");
    assert_eq!(parent.get(PropertyId::FontFamily), "mèo serif");
    assert_eq!(parent.get(PropertyId::MarginTop), "8px");
    assert_eq!(parent.get(PropertyId::MarginLeft), "8px");
    assert_eq!(parent.get(PropertyId::PaddingRight), "2px");
    assert_eq!(parent.get(PropertyId::BorderTopWidth), "thin");
    assert_eq!(parent.get(PropertyId::BorderRightWidth), "2px");
    assert_eq!(parent.get(PropertyId::BoxSizing), "border-box");
    assert_eq!(parent.custom_property("--space"), Some("8px"));
    assert_eq!(child.get(PropertyId::Width), "12.5px");
    assert_eq!(child.get(PropertyId::Color), "#aabbcc");
    assert_eq!(cycle.get(PropertyId::Height), "5px");
    assert!(
        snapshot
            .value_diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("cycle"))
    );
}

#[test]
fn attribute_mutation_restyles_only_changed_inheritance_branch() {
    let document = document(
        "<main><section id='left'><span id='left-child'></span></section><section id='right'><span id='right-child'></span></section></main>",
    );
    let author = parse_stylesheet("#left.active { color: red; } #right { color: blue; }");
    let sheets = [CascadeStylesheet::new(CascadeOrigin::Author, &author)];
    let mut engine = StyleEngine::new(&document, &sheets);
    let left = element(&document, "#left");
    let left_child = element(&document, "#left-child");
    let right = element(&document, "#right");
    let right_child = element(&document, "#right-child");
    let right_generation = engine.style_generation(right.id()).unwrap();
    let right_child_generation = engine.style_generation(right_child.id()).unwrap();

    let mutation = document
        .set_element_attribute(&left, "class", "active")
        .unwrap()
        .unwrap();
    let invalidation = engine.invalidate(&mutation);
    assert_eq!(invalidation.roots, vec![left.id()]);
    assert_eq!(engine.dirty_flag(left.id()), DirtyFlag::SelfOnly);
    assert_eq!(engine.dirty_flag(left_child.id()), DirtyFlag::Clean);

    let restyle = engine.restyle_dirty();
    assert!(restyle.restyled_nodes.contains(&left.id()));
    assert!(restyle.restyled_nodes.contains(&left_child.id()));
    assert!(!restyle.restyled_nodes.contains(&right.id()));
    assert!(!restyle.restyled_nodes.contains(&right_child.id()));
    assert_eq!(
        engine.style_for(left.id()).unwrap().get(PropertyId::Color),
        "red"
    );
    assert_eq!(
        engine
            .style_for(left_child.id())
            .unwrap()
            .get(PropertyId::Color),
        "red"
    );
    assert_eq!(engine.style_generation(right.id()), Some(right_generation));
    assert_eq!(
        engine.style_generation(right_child.id()),
        Some(right_child_generation)
    );
}

#[test]
fn ancestor_and_sibling_dependencies_expand_only_required_roots() {
    let document = document(
        "<main><section id='left'><span id='inside'></span></section><section id='right'><span id='right-child'></span></section><aside id='tail'></aside></main>",
    );
    let author =
        parse_stylesheet(".active span { width: 10px; } #left.active + #right { color: green; }");
    let sheets = [CascadeStylesheet::new(CascadeOrigin::Author, &author)];
    let mut engine = StyleEngine::new(&document, &sheets);
    let left = element(&document, "#left");
    let inside = element(&document, "#inside");
    let right = element(&document, "#right");
    let right_child = element(&document, "#right-child");
    let tail = element(&document, "#tail");
    let tail_generation = engine.style_generation(tail.id()).unwrap();

    let mutation = document
        .set_element_attribute(&left, "class", "active")
        .unwrap()
        .unwrap();
    let invalidation = engine.invalidate(&mutation);
    assert!(invalidation.roots.contains(&left.id()));
    assert!(invalidation.roots.contains(&right.id()));
    assert!(!invalidation.roots.contains(&tail.id()));
    assert_eq!(engine.dirty_flag(left.id()), DirtyFlag::Subtree);
    assert_eq!(engine.dirty_flag(inside.id()), DirtyFlag::SelfOnly);
    assert_eq!(engine.dirty_flag(right.id()), DirtyFlag::Subtree);
    assert_eq!(engine.dirty_flag(right_child.id()), DirtyFlag::SelfOnly);

    let restyle = engine.restyle_dirty();
    assert_eq!(
        engine
            .style_for(inside.id())
            .unwrap()
            .get(PropertyId::Width),
        "10px"
    );
    assert_eq!(
        engine.style_for(right.id()).unwrap().get(PropertyId::Color),
        "green"
    );
    assert!(!restyle.restyled_nodes.contains(&tail.id()));
    assert_eq!(engine.style_generation(tail.id()), Some(tail_generation));
}

#[test]
fn child_list_mutation_restyles_parent_subtree_but_not_siblings() {
    let document = document(
        "<main><section id='left'></section><section id='right'><span id='right-child'></span></section></main>",
    );
    let author = parse_stylesheet("#left:empty { display: none; } #left > span { color: red; }");
    let sheets = [CascadeStylesheet::new(CascadeOrigin::Author, &author)];
    let mut engine = StyleEngine::new(&document, &sheets);
    let left = element(&document, "#left");
    let right = element(&document, "#right");
    let right_generation = engine.style_generation(right.id()).unwrap();

    let (added, mutation) = document.append_element(&left, "span").unwrap();
    let invalidation = engine.invalidate(&mutation);
    assert_eq!(invalidation.roots, vec![left.id(), added.id()]);
    assert_eq!(engine.dirty_flag(left.id()), DirtyFlag::SelfOnly);
    assert_eq!(engine.dirty_flag(added.id()), DirtyFlag::Subtree);
    let restyle = engine.restyle_dirty();
    assert!(restyle.restyled_nodes.contains(&left.id()));
    assert!(restyle.restyled_nodes.contains(&added.id()));
    assert!(!restyle.restyled_nodes.contains(&right.id()));
    assert_eq!(
        engine
            .style_for(left.id())
            .unwrap()
            .get(PropertyId::Display),
        "inline"
    );
    assert_eq!(
        engine.style_for(added.id()).unwrap().get(PropertyId::Color),
        "red"
    );
    assert_eq!(engine.style_generation(right.id()), Some(right_generation));
}
