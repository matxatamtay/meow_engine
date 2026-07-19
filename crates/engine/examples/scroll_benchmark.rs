use std::{error::Error, time::Instant};

use meow_css::parse_stylesheet;
use meow_display_list::Viewport;
use meow_engine::{
    BrowserUrl, CharsetSource, DocumentState, DocumentStylesheet, DocumentView, InteractionState,
    StylesheetSource,
};
use meow_html::{StylesheetCandidateKind, parse_utf8};

fn main() -> Result<(), Box<dyn Error>> {
    let mut html = String::from(
        "<!doctype html><title>scroll benchmark</title><style>body{margin:16px}p{display:block;height:20px;margin-bottom:4px}</style><main>",
    );
    for index in 0..1_000 {
        html.push_str(&format!(
            "<p>benchmark line {index}: cached layout, translated paint</p>"
        ));
    }
    html.push_str("</main>");

    let parsed = parse_utf8(html.as_bytes());
    let candidate = parsed
        .document
        .stylesheet_candidates()
        .into_iter()
        .next()
        .expect("inline benchmark stylesheet");
    let StylesheetCandidateKind::Inline(css) = candidate.kind else {
        unreachable!("benchmark stylesheet is inline");
    };
    let url = BrowserUrl::parse("https://benchmark.invalid/scroll.html")?;
    let document = DocumentState {
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
    };
    let viewport = Viewport::new(1_280, 800)?;
    let view = DocumentView::new(&document, viewport);
    let mut interaction = InteractionState::default();
    interaction.reconcile(&view);

    const FRAMES: u32 = 600;
    let started = Instant::now();
    let mut direction = 1;
    let mut command_count = 0_usize;
    for _ in 0..FRAMES {
        if !interaction.scroll_by(&view, 0, direction * 24) {
            direction = -direction;
            interaction.scroll_by(&view, 0, direction * 24);
        }
        command_count += view.display_list(&interaction)?.commands().len();
    }
    let elapsed = started.elapsed();
    let average_ms = elapsed.as_secs_f64() * 1_000.0 / f64::from(FRAMES);
    let fps = 1_000.0 / average_ms;
    println!(
        "frames={FRAMES} average_ms={average_ms:.3} fps={fps:.1} commands={command_count} content_height={}",
        view.scroll_tree().content_height()
    );
    if average_ms > 16.667 {
        return Err(format!("60 FPS budget missed: {average_ms:.3} ms/frame").into());
    }
    Ok(())
}
