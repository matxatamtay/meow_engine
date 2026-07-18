use std::{env, error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).ok_or("usage: dump_css PATH")?;
    let source = fs::read_to_string(path)?;
    print!("{}", meow_css::parse_stylesheet(&source).dump());
    Ok(())
}
