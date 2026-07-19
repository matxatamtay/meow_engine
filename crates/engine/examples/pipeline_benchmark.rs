use std::{error::Error, time::Instant};

use meow_css::parse_stylesheet;
use meow_display_list::Viewport;
use meow_engine::{
    BrowserUrl, CharsetSource, DocumentState, DocumentStylesheet, DocumentView, InteractionState,
    StylesheetSource, glyph_cache_metrics,
};
use meow_html::{StylesheetCandidateKind, parse_utf8};

fn main() -> Result<(), Box<dyn Error>> {
    let cards = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(200);
    let document = benchmark_document(cards)?;
    let viewport = Viewport::new(1_280, 900)?;
    let started = Instant::now();
    let view = DocumentView::new(&document, viewport);
    let build_elapsed = started.elapsed();
    let interaction = InteractionState::default();
    let glyph_before = glyph_cache_metrics();
    let first = view.display_list(&interaction)?;
    let glyph_middle = glyph_cache_metrics();
    let second = view.display_list(&interaction)?;
    let glyph_after = glyph_cache_metrics();
    let metrics = view.metrics();

    println!(
        "cards={cards} build_ms={:.3} style_us={} box_us={} layout_us={} interaction_us={} styles={} unique_styles={} shared_elements={} boxes={} layout_boxes={} paragraphs={} glyphs={} commands={} glyph_first_misses={} glyph_second_hits={}",
        build_elapsed.as_secs_f64() * 1_000.0,
        metrics.style_micros,
        metrics.box_tree_micros,
        metrics.fragment_layout_micros,
        metrics.interaction_micros,
        metrics.style_elements,
        metrics.style_sharing.unique_styles,
        metrics.style_sharing.shared_elements,
        metrics.box_nodes,
        metrics.layout_boxes,
        metrics.paragraphs,
        metrics.glyphs,
        first.commands().len(),
        glyph_middle.misses.saturating_sub(glyph_before.misses),
        glyph_after.hits.saturating_sub(glyph_middle.hits),
    );
    if first.commands() != second.commands() {
        return Err(
            "display-list command stream changed between identical benchmark frames".into(),
        );
    }
    Ok(())
}

fn benchmark_document(cards: usize) -> Result<DocumentState, Box<dyn Error>> {
    let mut html = String::from(
        "<!doctype html><title>pipeline benchmark</title><style>html,body,main,article,h2,p{display:block;margin:0}body{padding:16px}main{display:flex;width:1200px;gap:8px}article{flex:1 1 180px;padding:8px;border-width:1px;background-color:white}h2{font-size:16px}p{font-size:14px}</style><main>",
    );
    for index in 0..cards {
        html.push_str(&format!(
            "<article><h2>Card {index}</h2><p>Shared styles and cached glyphs for benchmark corpus.</p></article>"
        ));
    }
    html.push_str("</main>");
    let parsed = parse_utf8(html.as_bytes());
    let candidate = parsed
        .document
        .stylesheet_candidates()
        .into_iter()
        .next()
        .expect("benchmark stylesheet");
    let StylesheetCandidateKind::Inline(css) = candidate.kind else {
        unreachable!("benchmark stylesheet is inline");
    };
    let url = BrowserUrl::parse("https://benchmark.invalid/pipeline.html")?;
    Ok(DocumentState {
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
    })
}
