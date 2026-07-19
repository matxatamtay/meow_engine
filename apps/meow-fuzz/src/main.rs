use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use encoding_rs::UTF_8;
use meow_css::{parse_selector_list, parse_stylesheet};
use meow_html::parse_bytes;
use meow_ipc::decode_envelope;
use meow_url_policy::BrowserUrl;
use serde::Serialize;

const TARGETS: [&str; 5] = ["html", "css", "ipc", "image", "url"];

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args().skip(1))?;
    let started = Instant::now();
    let deadline = started + Duration::from_secs(options.duration_seconds);
    let mut rng = XorShift64::new(options.seed);
    let mut results = Vec::new();
    for target in TARGETS {
        let corpus = read_corpus(&options.corpus_root.join(target))?;
        if corpus.is_empty() {
            return Err(format!("fuzz corpus {target} is empty").into());
        }
        let target_started = Instant::now();
        let mut iterations = 0_u64;
        let mut panics = Vec::new();
        while iterations < options.iterations && Instant::now() < deadline {
            let seed = &corpus[rng.index(corpus.len())];
            let input = mutate(seed, &mut rng, options.max_input_bytes);
            let outcome = std::panic::catch_unwind(|| execute(target, &input));
            if let Err(payload) = outcome {
                let crash = options
                    .output
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(format!("crash-{target}-{}-{iterations}.bin", options.seed));
                fs::write(&crash, &input)?;
                panics.push(format!(
                    "{}: {}",
                    crash.display(),
                    panic_message(payload.as_ref())
                ));
                break;
            }
            iterations = iterations.saturating_add(1);
        }
        results.push(TargetResult {
            target: target.to_owned(),
            corpus_entries: corpus.len(),
            iterations,
            elapsed_ms: elapsed_ms(target_started),
            new_crashes: panics,
        });
        if Instant::now() >= deadline {
            break;
        }
    }
    let total_iterations = results.iter().map(|result| result.iterations).sum();
    let new_crashes = results
        .iter()
        .map(|result| result.new_crashes.len())
        .sum::<usize>();
    let report = CampaignReport {
        schema_version: 1,
        seed: options.seed,
        duration_seconds: options.duration_seconds,
        requested_iterations_per_target: options.iterations,
        total_iterations,
        elapsed_ms: elapsed_ms(started),
        sanitizer: env::var("MEOW_SANITIZER").unwrap_or_else(|_| "none".to_owned()),
        new_crashes,
        targets: results,
    };
    write_json(&options.output, &report)?;
    println!(
        "fuzz campaign: {} iterations, {} new crashes, report {}",
        report.total_iterations,
        report.new_crashes,
        options.output.display()
    );
    if report.new_crashes > 0 {
        return Err("fuzz campaign found a new crash".into());
    }
    Ok(())
}

fn execute(target: &str, input: &[u8]) {
    match target {
        "html" => {
            let _ = parse_bytes(input, UTF_8);
        }
        "css" => {
            let source = String::from_utf8_lossy(input);
            let _ = parse_stylesheet(&source);
            let _ = parse_selector_list(&source);
        }
        "ipc" => {
            let _ = decode_envelope::<serde_json::Value>(input);
        }
        "image" => {
            let _ = image::load_from_memory(input);
        }
        "url" => {
            let source = String::from_utf8_lossy(input);
            let base =
                BrowserUrl::parse("https://example.test/base/path/").expect("static URL parses");
            let _ = BrowserUrl::parse(&source);
            let _ = base.resolve(&source);
        }
        _ => unreachable!("fixed fuzz target"),
    }
}

fn read_corpus(path: &Path) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let mut entries = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    entries
        .into_iter()
        .map(|entry| fs::read(entry.path()).map_err(Into::into))
        .collect()
}

fn mutate(seed: &[u8], rng: &mut XorShift64, max_bytes: usize) -> Vec<u8> {
    let mut output = seed[..seed.len().min(max_bytes)].to_vec();
    let mutations = 1 + rng.index(16);
    for _ in 0..mutations {
        match rng.next() % 4 {
            0 if !output.is_empty() => {
                let index = rng.index(output.len());
                output[index] ^= rng.next() as u8;
            }
            1 if !output.is_empty() => {
                let index = rng.index(output.len());
                output.remove(index);
            }
            2 if output.len() < max_bytes => {
                let index = if output.is_empty() {
                    0
                } else {
                    rng.index(output.len() + 1)
                };
                output.insert(index, rng.next() as u8);
            }
            _ if output.len() < max_bytes => {
                let remaining = max_bytes - output.len();
                let count = (1 + rng.index(32)).min(remaining);
                output.extend((0..count).map(|_| rng.next() as u8));
            }
            _ => {}
        }
    }
    output
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "non-string panic payload".to_owned()
    }
}

struct Options {
    duration_seconds: u64,
    iterations: u64,
    seed: u64,
    max_input_bytes: usize,
    corpus_root: PathBuf,
    output: PathBuf,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut options = Self {
            duration_seconds: 5,
            iterations: 10_000,
            seed: 0x4d45_4f57_2027,
            max_input_bytes: 64 * 1024,
            corpus_root: PathBuf::from("fuzz/corpora"),
            output: PathBuf::from("release/fuzz-report.json"),
        };
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let (name, inline) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            let value = |arguments: &mut std::iter::Peekable<_>| -> Result<String, Box<dyn Error>> {
                inline
                    .map(str::to_owned)
                    .or_else(|| arguments.next())
                    .ok_or_else(|| format!("{name} requires a value").into())
            };
            match name {
                "--duration-seconds" => {
                    options.duration_seconds = value(&mut arguments)?.parse()?
                }
                "--iterations" => options.iterations = value(&mut arguments)?.parse()?,
                "--seed" => options.seed = value(&mut arguments)?.parse()?,
                "--max-input-bytes" => options.max_input_bytes = value(&mut arguments)?.parse()?,
                "--corpus-root" => options.corpus_root = PathBuf::from(value(&mut arguments)?),
                "--output" => options.output = PathBuf::from(value(&mut arguments)?),
                _ => return Err(format!("unknown fuzz option {argument}").into()),
            }
        }
        if options.duration_seconds == 0 || options.iterations == 0 || options.max_input_bytes == 0
        {
            return Err("fuzz duration, iterations, and max input size must be positive".into());
        }
        Ok(options)
    }
}

#[derive(Serialize)]
struct CampaignReport {
    schema_version: u32,
    seed: u64,
    duration_seconds: u64,
    requested_iterations_per_target: u64,
    total_iterations: u64,
    elapsed_ms: u64,
    sanitizer: String,
    new_crashes: usize,
    targets: Vec<TargetResult>,
}

#[derive(Serialize)]
struct TargetResult {
    target: String,
    corpus_entries: usize,
    iterations: u64,
    elapsed_ms: u64,
    new_crashes: Vec<String>,
}

struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn index(&mut self, length: usize) -> usize {
        if length == 0 {
            0
        } else {
            (self.next() as usize) % length
        }
    }
}
