use std::{fmt::Write as _, fs, path::Path};

use meow_css::{PropertyId, parse_selector_list, parse_stylesheet};
use meow_engine::{CascadeOrigin, CascadeStylesheet, StyleEngine, compute_styles};
use meow_html::{Document, DomMutation, parse_utf8};

#[test]
fn typed_value_snapshot_is_byte_stable() {
    let fixture = fixture_root().join("typed-values");
    let document = parse_document(&fixture);
    let author_source = fs::read_to_string(fixture.join("author.css"))
        .expect("typed-value CSS fixture should exist");
    let author = parse_stylesheet(&author_source);
    let actual = compute_styles(
        &document,
        &[CascadeStylesheet::new(CascadeOrigin::Author, &author)],
    )
    .dump_typed();
    assert_snapshot(&fixture.join("expected.dump"), &actual);
}

#[test]
fn invalidation_snapshot_is_byte_stable_and_subtree_scoped() {
    let fixture = fixture_root().join("invalidation");
    let document = parse_document(&fixture);
    let author_source = fs::read_to_string(fixture.join("author.css"))
        .expect("invalidation CSS fixture should exist");
    let author = parse_stylesheet(&author_source);
    let sheets = [CascadeStylesheet::new(CascadeOrigin::Author, &author)];
    let mut engine = StyleEngine::new(&document, &sheets);
    let left = element(&document, "#left");
    let tail = element(&document, "#tail");
    let tail_generation = engine.style_generation(tail.id()).unwrap();

    let mutation = document
        .set_element_attribute(&left, "class", "active")
        .expect("attribute mutation should succeed")
        .expect("attribute mutation should change the DOM");
    let invalidation = engine.invalidate(&mutation);
    let restyle = engine.restyle_dirty();

    assert!(!restyle.restyled_nodes.contains(&tail.id()));
    assert_eq!(engine.style_generation(tail.id()), Some(tail_generation));

    let actual = dump_invalidation(&document, &engine, &mutation, &invalidation, &restyle);
    assert_snapshot(&fixture.join("expected.dump"), &actual);
}

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/w12")
}

fn parse_document(fixture: &Path) -> Document {
    let html = fs::read(fixture.join("document.html")).expect("fixture HTML should exist");
    parse_utf8(&html).document
}

fn element(document: &Document, selector: &str) -> meow_html::NodeHandle {
    document
        .query_selector(&parse_selector_list(selector).expect("fixture selector should parse"))
        .expect("fixture element should exist")
}

fn assert_snapshot(path: &Path, actual: &str) {
    if std::env::var_os("UPDATE_W12_SNAPSHOTS").is_some() {
        fs::write(path, actual).expect("W12 snapshot should update");
    }
    let expected = fs::read_to_string(path).expect("W12 expected dump should exist");
    assert_eq!(actual, expected, "fixture {} changed", path.display());
}

fn dump_invalidation(
    document: &Document,
    engine: &StyleEngine<'_>,
    mutation: &DomMutation,
    invalidation: &meow_engine::InvalidationReport,
    restyle: &meow_engine::RestyleReport,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "mutation kind={:?} target={} attribute={:?} added={:?} removed={:?}",
        mutation.kind,
        mutation.target.slot,
        mutation.attribute_name,
        slots(&mutation.added_nodes),
        slots(&mutation.removed_nodes)
    )
    .unwrap();
    writeln!(
        output,
        "invalidation roots={:?}",
        slots(&invalidation.roots)
    )
    .unwrap();
    writeln!(
        output,
        "invalidation dirty={:?}",
        slots(&invalidation.dirty_nodes)
    )
    .unwrap();
    writeln!(
        output,
        "restyle generation={} restyled={:?} changed={:?}",
        restyle.generation,
        slots(&restyle.restyled_nodes),
        slots(&restyle.changed_nodes)
    )
    .unwrap();

    for selector in [
        "#root",
        "#left",
        "#left-child",
        "#right",
        "#right-child",
        "#tail",
    ] {
        let element = element(document, selector);
        let style = engine.style_for(element.id()).unwrap();
        writeln!(
            output,
            "cache selector={selector:?} slot={} generation={} color={:?} width={:?} margin-top={:?}",
            element.id().slot,
            engine.style_generation(element.id()).unwrap(),
            style.get(PropertyId::Color),
            style.get(PropertyId::Width),
            style.get(PropertyId::MarginTop)
        )
        .unwrap();
    }
    output
}

fn slots(nodes: &[meow_html::NodeId]) -> Vec<u32> {
    nodes.iter().map(|node| node.slot).collect()
}
