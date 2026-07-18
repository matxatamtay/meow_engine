use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

use meow_engine::reference_renderer::{MIN_REFERENCE_DIMENSION, REFERENCE_HEIGHT, REFERENCE_WIDTH};

const DEFAULT_OUTPUT: &str = "meow-reference.png";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            output: PathBuf::from(DEFAULT_OUTPUT),
            width: REFERENCE_WIDTH,
            height: REFERENCE_HEIGHT,
            help: false,
        }
    }
}

impl Options {
    pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> io::Result<Self> {
        let mut options = Self::default();
        let mut arguments = arguments.into_iter();

        while let Some(argument) = arguments.next() {
            if argument == OsStr::new("-h") || argument == OsStr::new("--help") {
                options.help = true;
            } else if argument == OsStr::new("--output") {
                options.output = PathBuf::from(next_value(&mut arguments, "--output")?);
            } else if argument == OsStr::new("--width") {
                options.width = parse_dimension(next_value(&mut arguments, "--width")?, "width")?;
            } else if argument == OsStr::new("--height") {
                options.height =
                    parse_dimension(next_value(&mut arguments, "--height")?, "height")?;
            } else if let Some(value) = argument.to_string_lossy().strip_prefix("--output=") {
                options.output = PathBuf::from(value);
            } else if let Some(value) = argument.to_string_lossy().strip_prefix("--width=") {
                options.width = parse_dimension(OsString::from(value), "width")?;
            } else if let Some(value) = argument.to_string_lossy().strip_prefix("--height=") {
                options.height = parse_dimension(OsString::from(value), "height")?;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument {:?}; use --help for usage", argument),
                ));
            }
        }

        Ok(options)
    }
}

pub fn write_output(path: &Path, png: &[u8]) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, png)
}

pub fn print_help() {
    println!(
        "{name} {version}\n\n\
         Render the deterministic MeowEngine W3 reference scene.\n\n\
         Usage:\n  {name} [--output PATH] [--width PIXELS] [--height PIXELS]\n\n\
         Options:\n  --output PATH    Output PNG path [default: {default_output}]\n  --width PIXELS   Framebuffer width [default: {default_width}]\n  --height PIXELS  Framebuffer height [default: {default_height}]\n  -h, --help       Print this help\n",
        name = env!("CARGO_PKG_NAME"),
        version = meow_engine::version(),
        default_output = DEFAULT_OUTPUT,
        default_width = REFERENCE_WIDTH,
        default_height = REFERENCE_HEIGHT,
    );
}

fn next_value(
    arguments: &mut impl Iterator<Item = OsString>,
    option: &str,
) -> io::Result<OsString> {
    arguments.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{option} requires a value"),
        )
    })
}

fn parse_dimension(value: OsString, name: &str) -> io::Result<u32> {
    let value = value.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be valid UTF-8 digits"),
        )
    })?;
    let dimension = value.parse::<u32>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid {name} {value:?}; expected a positive integer"),
        )
    })?;

    if dimension < MIN_REFERENCE_DIMENSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be at least {MIN_REFERENCE_DIMENSION}"),
        ));
    }

    Ok(dimension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_output_and_dimensions() {
        let options = Options::parse([
            OsString::from("--output"),
            OsString::from("artifacts/frame.png"),
            OsString::from("--width=320"),
            OsString::from("--height"),
            OsString::from("200"),
        ])
        .expect("options should parse");

        assert_eq!(options.output, PathBuf::from("artifacts/frame.png"));
        assert_eq!(options.width, 320);
        assert_eq!(options.height, 200);
        assert!(!options.help);
    }

    #[test]
    fn rejects_small_dimensions_and_unknown_options() {
        assert!(Options::parse([OsString::from("--width=1")]).is_err());
        assert!(Options::parse([OsString::from("--cat")]).is_err());
    }
}
