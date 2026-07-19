use meow_css::parse_stylesheet;
use meow_display_list::Viewport;
use meow_engine::{
    BrowserUrl, CharsetSource, DocumentState, DocumentStylesheet, DocumentView, InteractionState,
    StylesheetSource, glyph_cache_metrics,
};
use meow_html::{StylesheetCandidateKind, parse_utf8};

#[test]
fn benchmark_corpus_keeps_structural_counts_and_cache_reuse_stable() {
    let document = benchmark_document(200);
    let viewport = Viewport::new(1_280, 900).unwrap();
    let view = DocumentView::new(&document, viewport);
    let metrics = view.metrics();
    assert_eq!(metrics.style_elements, 605);
    assert_eq!(metrics.box_nodes, metrics.layout_boxes);
    assert!(metrics.style_sharing.shared_elements >= 590);
    assert!(metrics.style_sharing.unique_styles <= 12);
    assert_eq!(metrics.images, 0);

    let interaction = InteractionState::default();
    let before = glyph_cache_metrics();
    let first = view.display_list(&interaction).unwrap();
    let middle = glyph_cache_metrics();
    let second = view.display_list(&interaction).unwrap();
    let after = glyph_cache_metrics();
    assert_eq!(first.commands(), second.commands());
    assert!(middle.misses >= before.misses);
    assert!(after.hits > middle.hits);
}

fn benchmark_document(cards: usize) -> DocumentState {
    let mut html = String::from(
        "<!doctype html><style>html,body,main,article,h2,p{display:block;margin:0}main{display:flex;width:1200px;gap:8px}article{flex:1 1 180px;padding:8px;border-width:1px}h2{font-size:16px}p{font-size:14px}</style><main>",
    );
    for index in 0..cards {
        html.push_str(&format!(
            "<article><h2>Card {index}</h2><p>Shared styles and cached glyphs.</p></article>"
        ));
    }
    html.push_str("</main>");
    let parsed = parse_utf8(html.as_bytes());
    let candidate = parsed
        .document
        .stylesheet_candidates()
        .into_iter()
        .next()
        .unwrap();
    let StylesheetCandidateKind::Inline(css) = candidate.kind else {
        unreachable!();
    };
    let url = BrowserUrl::parse("https://benchmark.invalid/pipeline.html").unwrap();
    DocumentState {
        url: url.clone(),
        base_url: url,
        document: parsed.document,
        encoding: "UTF-8",
        charset_source: CharsetSource::Default,
        response: None,
        stylesheets: vec![DocumentStylesheet {
            source: StylesheetSource::Inline {
                node: candidate.node,
            },
            media: candidate.media,
            stylesheet: parse_stylesheet(&css),
        }],
        stylesheet_errors: Vec::new(),
        script_executions: Vec::new(),
        script_mutations: Vec::new(),
        images: Default::default(),
        image_errors: Vec::new(),
        image_cache_metrics: Default::default(),
        history_index: 0,
    }
}
