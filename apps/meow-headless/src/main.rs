mod cli;

use std::{env, error::Error};

use cli::{Options, print_help, write_output};
use meow_engine::reference_renderer::render_reference_png;

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args_os().skip(1))?;
    if options.help {
        print_help();
        return Ok(());
    }

    let png = render_reference_png(options.width, options.height)?;
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
