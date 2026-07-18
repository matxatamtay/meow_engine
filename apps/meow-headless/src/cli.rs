use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

use meow_embedder_api::{MIN_REFERENCE_DIMENSION, REFERENCE_HEIGHT, REFERENCE_WIDTH};

const DEFAULT_OUTPUT: &str = "meow-reference.png";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub output: PathBuf,
    pub output_explicit: bool,
    pub width: u32,
    pub height: u32,
    pub dom_url: Option<String>,
    pub css_url: Option<String>,
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            output: PathBuf::from(DEFAULT_OUTPUT),
            output_explicit: false,
            width: REFERENCE_WIDTH,
            height: REFERENCE_HEIGHT,
            dom_url: None,
            css_url: None,
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
                options.output_explicit = true;
            } else if argument == OsStr::new("--dump-dom") {
                options.dom_url = Some(
                    next_value(&mut arguments, "--dump-dom")?
                        .into_string()
                        .map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "--dump-dom URL must be valid UTF-8",
                            )
                        })?,
                );
            } else if argument == OsStr::new("--dump-css") {
                options.css_url = Some(
                    next_value(&mut arguments, "--dump-css")?
                        .into_string()
                        .map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "--dump-css URL must be valid UTF-8",
                            )
                        })?,
                );
            } else if argument == OsStr::new("--width") {
                options.width = parse_dimension(next_value(&mut arguments, "--width")?, "width")?;
            } else if argument == OsStr::new("--height") {
                options.height =
                    parse_dimension(next_value(&mut arguments, "--height")?, "height")?;
            } else if let Some(value) = argument.to_string_lossy().strip_prefix("--output=") {
                options.output = PathBuf::from(value);
                options.output_explicit = true;
            } else if let Some(value) = argument.to_string_lossy().strip_prefix("--dump-dom=") {
                if value.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--dump-dom requires a URL",
                    ));
                }
                options.dom_url = Some(value.to_owned());
            } else if let Some(value) = argument.to_string_lossy().strip_prefix("--dump-css=") {
                if value.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--dump-css requires a URL",
                    ));
                }
                options.css_url = Some(value.to_owned());
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

        if options.dom_url.is_some() && options.css_url.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--dump-dom and --dump-css cannot be used together",
            ));
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
         Render the reference scene or load a URL and dump its DOM or CSS.\n\n\
         Usage:\n  {name} [--output PATH] [--width PIXELS] [--height PIXELS]\n  {name} --dump-dom URL [--output PATH]\n  {name} --dump-css URL [--output PATH]\n\n\
         Options:\n  --dump-dom URL   Load HTTP(S)/about:blank and emit a deterministic DOM dump\n  --dump-css URL   Load a document and emit parsed stylesheets and diagnostics\n  --output PATH    Output PNG, DOM, or CSS dump [default PNG: {default_output}]\n  --width PIXELS   Framebuffer width [default: {default_width}]\n  --height PIXELS  Framebuffer height [default: {default_height}]\n  -h, --help       Print this help\n",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
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
        assert!(options.output_explicit);
        assert!(options.dom_url.is_none());
        assert!(options.css_url.is_none());
        assert!(!options.help);
    }

    #[test]
    fn parses_dom_dump_mode() {
        let options = Options::parse([
            OsString::from("--dump-dom"),
            OsString::from("https://example.test/"),
        ])
        .expect("DOM options should parse");

        assert_eq!(options.dom_url.as_deref(), Some("https://example.test/"));
        assert!(!options.output_explicit);
    }

    #[test]
    fn parses_css_dump_mode() {
        let options = Options::parse([OsString::from("--dump-css=https://example.test/")])
            .expect("CSS options should parse");

        assert_eq!(options.css_url.as_deref(), Some("https://example.test/"));
        assert!(options.dom_url.is_none());
    }

    #[test]
    fn rejects_conflicting_dump_modes_small_dimensions_and_unknown_options() {
        assert!(
            Options::parse([
                OsString::from("--dump-dom=https://example.test/"),
                OsString::from("--dump-css=https://example.test/"),
            ])
            .is_err()
        );
        assert!(Options::parse([OsString::from("--width=1")]).is_err());
        assert!(Options::parse([OsString::from("--cat")]).is_err());
    }
}
