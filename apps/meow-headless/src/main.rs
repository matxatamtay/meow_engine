mod cli;

use std::{env, error::Error};

use cli::{Options, print_help, write_output};
use meow_embedder_api::{BrowserEngine, CancellationToken};
use meow_renderer::{ReferenceRenderer, Renderer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args_os().skip(1))?;
    if options.help {
        print_help();
        return Ok(());
    }

    let mut engine = BrowserEngine::new();
    if let Some(url) = options.css_url.as_deref() {
        let state = engine.navigate(url, &CancellationToken::new()).await?;
        let dump = state.dump_stylesheets();
        if options.output_explicit {
            write_output(&options.output, dump.as_bytes())?;
            println!(
                "wrote CSS dump for {} to {} ({} stylesheets, {} load errors)",
                state.url,
                options.output.display(),
                state.stylesheets.len(),
                state.stylesheet_errors.len()
            );
        } else {
            print!("{dump}");
        }
        return Ok(());
    }
    if let Some(url) = options.dom_url.as_deref() {
        let state = engine.navigate(url, &CancellationToken::new()).await?;
        let dump = state.document.dump();
        if options.output_explicit {
            write_output(&options.output, dump.as_bytes())?;
            println!(
                "wrote DOM dump for {} to {} ({} nodes, {})",
                state.url,
                options.output.display(),
                state.document.node_count(),
                state.encoding
            );
        } else {
            print!("{dump}");
        }
        return Ok(());
    }

    let frame = engine.render_frame(options.width, options.height)?;
    let framebuffer = ReferenceRenderer::new().render(frame.viewport(), frame.display_list())?;
    let png = framebuffer.encode_png()?;
    write_output(&options.output, &png)?;

    println!(
        "wrote deterministic {}x{} reference PNG to {} ({} bytes)",
        options.width,
        options.height,
        options.output.display(),
        png.len()
    );
    Ok(())
}
