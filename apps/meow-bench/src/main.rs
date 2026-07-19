use std::{
    env,
    error::Error,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    thread,
    time::Instant,
};

use meow_embedder_api::{BrowserEngine, CancellationToken};
use meow_net::{LoadConfig, Loader, Request};
use meow_url_policy::BrowserUrl;
use serde::{Deserialize, Serialize};

type LocalServer = thread::JoinHandle<std::io::Result<()>>;
type ServerResult = Result<(String, LocalServer), Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args().skip(1))?;
    let budgets: Budgets = read_json(&options.baseline)?;
    let profile = std::env::temp_dir().join(format!("meow-budget-{}", std::process::id()));

    let started = Instant::now();
    let mut engine = BrowserEngine::new_with_profile(&profile);
    let startup_ms = elapsed_ms(started);

    let (page_url, page_server) = page_server()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let page_started = Instant::now();
    runtime.block_on(engine.navigate(&page_url, &CancellationToken::new()))?;
    let page_load_ms = elapsed_ms(page_started);
    page_server.join().map_err(|_| "page server panicked")??;
    let _ = engine.render_document_frame(1280, 800)?;

    let mut scroll_samples = Vec::new();
    for _ in 0..200 {
        let started = Instant::now();
        let _ = engine.scroll_by(1280, 800, 0, 24)?;
        let _ = engine.render_document_frame(1280, 800)?;
        scroll_samples.push(elapsed_micros(started));
    }
    scroll_samples.sort_unstable();
    let scroll_p95_ms = scroll_samples[scroll_samples.len() * 95 / 100] as f64 / 1000.0;

    let (cache_url, cache_server) = cache_server()?;
    let loader = Loader::new(LoadConfig::default());
    let target = BrowserUrl::parse(&cache_url)?;
    runtime.block_on(loader.load(Request::get(target.clone()), &CancellationToken::new()))?;
    runtime.block_on(loader.load(Request::get(target), &CancellationToken::new()))?;
    cache_server.join().map_err(|_| "cache server panicked")??;
    let cache = loader
        .cache_metrics()
        .ok_or("direct cache metrics missing")?;
    let cache_hit_rate = cache.hits as f64 / (cache.hits + cache.misses).max(1) as f64;

    let browser_binary_bytes = file_size(&options.browser_bin);
    let headless_binary_bytes = file_size(&options.headless_bin);
    let binary_total_mb = (browser_binary_bytes + headless_binary_bytes) as f64 / 1_048_576.0;
    let peak_rss_mb = peak_rss_kib().unwrap_or(0) as f64 / 1024.0;
    let measurements = Measurements {
        startup_ms,
        peak_rss_mb,
        scroll_p95_ms,
        page_load_ms,
        browser_binary_bytes,
        headless_binary_bytes,
        binary_total_mb,
        cache_hits: cache.hits,
        cache_misses: cache.misses,
        cache_hit_rate,
    };
    let violations = budgets.check(&measurements);
    let report = BudgetReport {
        schema_version: 1,
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        environment: Environment {
            os: env::consts::OS.to_owned(),
            arch: env::consts::ARCH.to_owned(),
            rustc: command_output("rustc", &["--version"]),
            cpu_count: thread::available_parallelism().map_or(1, usize::from),
        },
        budgets,
        measurements,
        violations,
    };
    write_json(&options.output, &report)?;
    let _ = fs::remove_dir_all(profile);
    println!(
        "release budgets: startup={}ms rss={:.1}MiB scroll-p95={:.3}ms load={}ms binary={:.1}MiB cache-hit={:.1}%",
        report.measurements.startup_ms,
        report.measurements.peak_rss_mb,
        report.measurements.scroll_p95_ms,
        report.measurements.page_load_ms,
        report.measurements.binary_total_mb,
        report.measurements.cache_hit_rate * 100.0
    );
    if !report.violations.is_empty() {
        return Err(format!(
            "release budget violations: {}",
            report.violations.join("; ")
        )
        .into());
    }
    Ok(())
}

fn page_server() -> ServerResult {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request)?;
        let rows = (0..400)
            .map(|index| format!("<p>Budget row {index} tiếng Việt</p>"))
            .collect::<String>();
        let body = format!(
            "<!doctype html><title>Budget</title><style>body{{width:900px}}p{{height:20px;margin:2px}}</style><main>{rows}</main>"
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )?;
        stream.flush()
    });
    Ok((format!("http://{address}/page"), handle))
}

fn cache_server() -> ServerResult {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request)?;
        let body = b"cache-cat";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nCache-Control: public, max-age=3600\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        stream.write_all(body)?;
        stream.flush()
    });
    Ok((format!("http://{address}/cache"), handle))
}

fn peak_rss_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn command_output(command: &str, arguments: &[&str]) -> String {
    std::process::Command::new(command)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
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

struct Options {
    baseline: PathBuf,
    output: PathBuf,
    browser_bin: PathBuf,
    headless_bin: PathBuf,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut options = Self {
            baseline: PathBuf::from("release/budgets.json"),
            output: PathBuf::from("release/budget-report.json"),
            browser_bin: PathBuf::from("target/release/meow-browser"),
            headless_bin: PathBuf::from("target/release/meow-headless"),
        };
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let (name, inline) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            let value = inline
                .map(str::to_owned)
                .or_else(|| arguments.next())
                .ok_or_else(|| format!("{name} requires a value"))?;
            match name {
                "--baseline" => options.baseline = PathBuf::from(value),
                "--output" => options.output = PathBuf::from(value),
                "--browser-bin" => options.browser_bin = PathBuf::from(value),
                "--headless-bin" => options.headless_bin = PathBuf::from(value),
                _ => return Err(format!("unknown budget option {argument}").into()),
            }
        }
        Ok(options)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Budgets {
    max_startup_ms: u64,
    max_peak_rss_mb: f64,
    max_scroll_p95_ms: f64,
    max_page_load_ms: u64,
    max_binary_total_mb: f64,
    min_cache_hit_rate: f64,
}

impl Budgets {
    fn check(&self, measurements: &Measurements) -> Vec<String> {
        let mut violations = Vec::new();
        if measurements.startup_ms > self.max_startup_ms {
            violations.push(format!(
                "startup {} > {} ms",
                measurements.startup_ms, self.max_startup_ms
            ));
        }
        if measurements.peak_rss_mb > self.max_peak_rss_mb {
            violations.push(format!(
                "peak RSS {:.1} > {:.1} MiB",
                measurements.peak_rss_mb, self.max_peak_rss_mb
            ));
        }
        if measurements.scroll_p95_ms > self.max_scroll_p95_ms {
            violations.push(format!(
                "scroll p95 {:.3} > {:.3} ms",
                measurements.scroll_p95_ms, self.max_scroll_p95_ms
            ));
        }
        if measurements.page_load_ms > self.max_page_load_ms {
            violations.push(format!(
                "page load {} > {} ms",
                measurements.page_load_ms, self.max_page_load_ms
            ));
        }
        if measurements.binary_total_mb > self.max_binary_total_mb {
            violations.push(format!(
                "binary total {:.1} > {:.1} MiB",
                measurements.binary_total_mb, self.max_binary_total_mb
            ));
        }
        if measurements.cache_hit_rate < self.min_cache_hit_rate {
            violations.push(format!(
                "cache hit rate {:.3} < {:.3}",
                measurements.cache_hit_rate, self.min_cache_hit_rate
            ));
        }
        violations
    }
}

#[derive(Debug, Serialize)]
struct BudgetReport {
    schema_version: u32,
    engine_version: String,
    environment: Environment,
    budgets: Budgets,
    measurements: Measurements,
    violations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Environment {
    os: String,
    arch: String,
    rustc: String,
    cpu_count: usize,
}

#[derive(Debug, Serialize)]
struct Measurements {
    startup_ms: u64,
    peak_rss_mb: f64,
    scroll_p95_ms: f64,
    page_load_ms: u64,
    browser_binary_bytes: u64,
    headless_binary_bytes: u64,
    binary_total_mb: f64,
    cache_hits: u64,
    cache_misses: u64,
    cache_hit_rate: f64,
}
