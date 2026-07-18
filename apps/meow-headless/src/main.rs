mod cli;

use std::{env, error::Error};

use cli::{Options, print_help, write_output};
use meow_embedder_api::BrowserEngine;
use meow_renderer::{ReferenceRenderer, Renderer};

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args_os().skip(1))?;
    if options.help {
        print_help();
        return Ok(());
    }

    let frame = BrowserEngine::new().render_frame(options.width, options.height)?;
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
