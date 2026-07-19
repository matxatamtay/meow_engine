use std::{
    collections::BTreeMap,
    env,
    error::Error,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

use meow_accessibility::{AccessibilityTree, audit_keyboard_navigation};
use meow_css::{PropertyId, parse_selector_list, parse_stylesheet};
use meow_display_list::Viewport;
use meow_engine::{
    CascadeOrigin, CascadeStylesheet, CharsetSource, DocumentState, DocumentStylesheet,
    ImageCacheMetrics, StylesheetSource, compute_styles,
};
use meow_html::{NodeHandle, NodeId, parse_utf8};
use meow_renderer::{ReferenceRenderer, Renderer};
use meow_url_policy::BrowserUrl;
use serde::{Deserialize, Serialize};

const DEFAULT_MANIFEST: &str = "tests/wpt/manifest.json";
const DEFAULT_BASELINE: &str = "tests/wpt/baseline.json";
const DEFAULT_OUTPUT: &str = "artifacts/wpt";
const DEFAULT_TIMEOUT_MS: u64 = 2_000;

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<OsString>) -> Result<(), Box<dyn Error>> {
    if arguments.first().is_some_and(|value| value == "--worker") {
        return run_worker(&arguments);
    }
    let options = Options::parse(arguments)?;
    let manifest = read_json::<Manifest>(&options.manifest)?;
    validate_manifest(&manifest)?;
    let selected = manifest
        .cases
        .iter()
        .enumerate()
        .filter(|(_, case)| {
            options
                .suite
                .as_ref()
                .is_none_or(|suite| &case.suite == suite)
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("selected WPT suite has no cases".into());
    }

    fs::create_dir_all(&options.output)?;
    let executable = env::current_exe()?;
    let mut results = Vec::with_capacity(selected.len());
    for (index, case) in selected {
        results.push(run_case_process(
            &executable,
            &options.manifest,
            index,
            case,
            options.timeout_ms,
        ));
    }
    results.sort_by(|left, right| left.id.cmp(&right.id));
    let summary = Summary::from_results(&results);
    let report = Report {
        schema_version: 1,
        manifest_digest: digest_bytes(&fs::read(&options.manifest)?),
        selected_pass_rate_target: manifest.pass_rate_target,
        summary,
        results,
    };
    write_json(&options.output.join("report.json"), &report)?;
    fs::write(
        options.output.join("dashboard.html"),
        render_dashboard(&report),
    )?;

    let baseline = Baseline::from_report(&report);
    if options.update_baseline {
        write_json(&options.baseline, &baseline)?;
        println!("updated WPT baseline at {}", options.baseline.display());
    } else {
        let expected = read_json::<Baseline>(&options.baseline)?;
        compare_baseline(&expected, &baseline)?;
    }
    if report.summary.pass_rate + f64::EPSILON < manifest.pass_rate_target {
        return Err(format!(
            "selected WPT pass rate {:.2}% is below target {:.2}%",
            report.summary.pass_rate * 100.0,
            manifest.pass_rate_target * 100.0
        )
        .into());
    }
    if report.summary.failed > 0 || report.summary.timed_out > 0 {
        return Err(format!(
            "selected WPT run has {} failures and {} timeouts",
            report.summary.failed, report.summary.timed_out
        )
        .into());
    }
    println!(
        "selected WPT: {}/{} passed ({:.2}%), dashboard {}",
        report.summary.passed,
        report.summary.total,
        report.summary.pass_rate * 100.0,
        options.output.join("dashboard.html").display()
    );
    Ok(())
}

fn run_worker(arguments: &[OsString]) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 3 {
        return Err("worker usage: --worker MANIFEST INDEX".into());
    }
    let manifest_path = PathBuf::from(&arguments[1]);
    let index = arguments[2]
        .to_str()
        .ok_or("worker index must be UTF-8")?
        .parse::<usize>()?;
    let manifest = read_json::<Manifest>(&manifest_path)?;
    let case = manifest
        .cases
        .get(index)
        .ok_or("worker case index out of range")?;
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(|| execute_case(case));
    let (status, message) = match outcome {
        Ok(Ok(())) => (Status::Pass, None),
        Ok(Err(error)) => (Status::Fail, Some(error)),
        Err(payload) => (Status::Fail, Some(panic_message(payload.as_ref()))),
    };
    let result = CaseResult {
        id: case.id.clone(),
        suite: case.suite.clone(),
        upstream: case.upstream.clone(),
        status,
        duration_ms: elapsed_ms(started),
        message,
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

fn run_case_process(
    executable: &Path,
    manifest: &Path,
    index: usize,
    case: &Case,
    timeout_ms: u64,
) -> CaseResult {
    let started = Instant::now();
    let mut child = match Command::new(executable)
        .arg("--worker")
        .arg(manifest)
        .arg(index.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return CaseResult::failed(case, elapsed_ms(started), error.to_string());
        }
    };
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_millis(timeout_ms) => {
                thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return CaseResult {
                    id: case.id.clone(),
                    suite: case.suite.clone(),
                    upstream: case.upstream.clone(),
                    status: Status::Timeout,
                    duration_ms: elapsed_ms(started),
                    message: Some(format!("case exceeded {timeout_ms} ms")),
                };
            }
            Err(error) => {
                let _ = child.kill();
                return CaseResult::failed(case, elapsed_ms(started), error.to_string());
            }
        }
    }
    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(error) => return CaseResult::failed(case, elapsed_ms(started), error.to_string()),
    };
    if !output.status.success() {
        return CaseResult::failed(
            case,
            elapsed_ms(started),
            format!(
                "worker exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        );
    }
    match serde_json::from_slice::<CaseResult>(&output.stdout) {
        Ok(mut result) => {
            result.duration_ms = elapsed_ms(started);
            result
        }
        Err(error) => CaseResult::failed(
            case,
            elapsed_ms(started),
            format!("invalid worker response: {error}"),
        ),
    }
}

fn execute_case(case: &Case) -> Result<(), String> {
    match &case.test {
        Test::HtmlDump { source, contains } => {
            let document = parse_utf8(source.as_bytes()).document;
            let dump = document.dump();
            assert_contains_all(&dump, contains)
        }
        Test::Selector {
            source,
            selector,
            expected_ids,
        } => {
            let document = parse_utf8(source.as_bytes()).document;
            let selectors = parse_selector_list(selector).map_err(|error| error.to_string())?;
            let actual = document
                .query_selector_all(&selectors)
                .iter()
                .filter_map(|node| document.element_attribute(node, "id"))
                .collect::<Vec<_>>();
            equal("selector IDs", &actual, expected_ids)
        }
        Test::Cascade {
            source,
            css,
            target,
            property,
            expected,
        } => {
            let document = parse_utf8(source.as_bytes()).document;
            let stylesheet = parse_stylesheet(css);
            let snapshot = compute_styles(
                &document,
                &[CascadeStylesheet::new(CascadeOrigin::Author, &stylesheet)],
            );
            let selector = parse_selector_list(target).map_err(|error| error.to_string())?;
            let element = document
                .query_selector(&selector)
                .ok_or_else(|| format!("target {target:?} did not match"))?;
            let property = PropertyId::from_name(property)
                .ok_or_else(|| format!("unsupported property {property:?}"))?;
            let actual = snapshot
                .style_for(element.id())
                .ok_or("target has no computed style")?
                .get(property);
            equal("computed property", actual, expected)
        }
        Test::LayoutRef {
            test_html,
            test_css,
            reference_html,
            reference_css,
            width,
            height,
        } => {
            let test = render_pixels(test_html, test_css, *width, *height)?;
            let reference = render_pixels(reference_html, reference_css, *width, *height)?;
            if test == reference {
                Ok(())
            } else {
                Err(format!(
                    "reftest pixel digest mismatch: test={} reference={}",
                    digest_bytes(&test),
                    digest_bytes(&reference)
                ))
            }
        }
        Test::Accessibility {
            source,
            expectations,
            focus_ids,
        } => {
            let document = parse_utf8(source.as_bytes()).document;
            let tree = AccessibilityTree::build(&document);
            let flattened = tree.flatten();
            for expectation in expectations {
                let element = find_by_id(&document, &expectation.id)
                    .ok_or_else(|| format!("accessibility target #{} missing", expectation.id))?;
                let node = flattened
                    .iter()
                    .find(|node| node.node_slot == element.id().slot)
                    .ok_or_else(|| {
                        format!("#{} missing from accessibility tree", expectation.id)
                    })?;
                equal("accessible role", node.role.as_str(), &expectation.role)?;
                equal("accessible name", &node.name, &expectation.name)?;
                equal("focusable", &node.focusable, &expectation.focusable)?;
            }
            let actual_focus = tree
                .focus_order_slots
                .iter()
                .filter_map(|slot| id_for_slot(&document, *slot))
                .collect::<Vec<_>>();
            equal("focus order", &actual_focus, focus_ids)
        }
        Test::KeyboardAudit {
            source,
            focus_ids,
            finding_codes,
        } => {
            let document = parse_utf8(source.as_bytes()).document;
            let audit = audit_keyboard_navigation(&document);
            let actual_focus = audit
                .focus_order_slots
                .iter()
                .filter_map(|slot| id_for_slot(&document, *slot))
                .collect::<Vec<_>>();
            equal("keyboard focus order", &actual_focus, focus_ids)?;
            let actual_findings = audit
                .findings
                .iter()
                .map(|finding| finding.code.clone())
                .collect::<Vec<_>>();
            equal("keyboard findings", &actual_findings, finding_codes)
        }
    }
}

fn render_pixels(html: &str, css: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let state = document_state(html, css);
    let viewport = Viewport::new(width, height).map_err(|error| error.to_string())?;
    let list = state
        .readable_display_list(viewport)
        .map_err(|error| error.to_string())?;
    let frame = ReferenceRenderer::new()
        .render(viewport, &list)
        .map_err(|error| error.to_string())?;
    Ok(frame.premultiplied_rgba().to_vec())
}

fn document_state(html: &str, css: &str) -> DocumentState {
    let url = BrowserUrl::parse("https://wpt.meow.invalid/test.html").expect("static URL parses");
    DocumentState {
        url: url.clone(),
        base_url: url,
        document: parse_utf8(html.as_bytes()).document,
        encoding: "UTF-8",
        charset_source: CharsetSource::Default,
        response: None,
        stylesheets: vec![DocumentStylesheet {
            source: StylesheetSource::Inline {
                node: NodeId {
                    document: 0,
                    slot: 0,
                    generation: 0,
                },
            },
            media: None,
            stylesheet: parse_stylesheet(css),
        }],
        stylesheet_errors: Vec::new(),
        script_executions: Vec::new(),
        script_mutations: Vec::new(),
        images: BTreeMap::new(),
        image_errors: Vec::new(),
        image_cache_metrics: ImageCacheMetrics::default(),
        history_index: 0,
    }
}

fn find_by_id(document: &meow_html::Document, id: &str) -> Option<NodeHandle> {
    document
        .elements_in_tree_order()
        .into_iter()
        .find(|node| document.element_attribute(node, "id").as_deref() == Some(id))
}

fn id_for_slot(document: &meow_html::Document, slot: u32) -> Option<String> {
    document
        .elements_in_tree_order()
        .into_iter()
        .find(|node| node.id().slot == slot)
        .and_then(|node| document.element_attribute(&node, "id"))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), Box<dyn Error>> {
    if !(0.0..=1.0).contains(&manifest.pass_rate_target) {
        return Err("WPT pass-rate target must be between 0 and 1".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for case in &manifest.cases {
        if !ids.insert(&case.id) {
            return Err(format!("duplicate WPT case ID {}", case.id).into());
        }
        if case.id.is_empty() || case.suite.is_empty() || case.upstream.is_empty() {
            return Err("WPT cases require id, suite, and upstream fields".into());
        }
    }
    Ok(())
}

fn compare_baseline(expected: &Baseline, actual: &Baseline) -> Result<(), Box<dyn Error>> {
    if expected.schema_version != actual.schema_version {
        return Err("WPT baseline schema version changed".into());
    }
    if expected.manifest_digest != actual.manifest_digest {
        return Err(format!(
            "WPT manifest changed (baseline {}, actual {}); run --update-baseline after review",
            expected.manifest_digest, actual.manifest_digest
        )
        .into());
    }
    if expected.results != actual.results {
        let expected_map = expected
            .results
            .iter()
            .map(|entry| (&entry.id, entry.status))
            .collect::<BTreeMap<_, _>>();
        let differences = actual
            .results
            .iter()
            .filter_map(|entry| {
                let before = expected_map.get(&entry.id).copied();
                (before != Some(entry.status))
                    .then(|| format!("{}: {:?} -> {:?}", entry.id, before, entry.status))
            })
            .collect::<Vec<_>>();
        return Err(format!("WPT baseline changed: {}", differences.join(", ")).into());
    }
    Ok(())
}

fn render_dashboard(report: &Report) -> String {
    let rows = report
        .results
        .iter()
        .map(|result| {
            format!(
                "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td></tr>",
                result.status.as_str(),
                escape_html(&result.id),
                escape_html(&result.suite),
                escape_html(&result.upstream),
                result.status,
                result.duration_ms,
                escape_html(result.message.as_deref().unwrap_or(""))
            )
        })
        .collect::<String>();
    format!(
        "<!doctype html><meta charset=utf-8><title>MeowEngine WPT triage</title><style>body{{font:14px system-ui;margin:24px;background:#fafafa;color:#222}}.cards{{display:flex;gap:12px}}.card{{background:white;padding:12px 18px;border:1px solid #ddd;border-radius:8px}}table{{width:100%;border-collapse:collapse;background:white;margin-top:18px}}th,td{{border:1px solid #ddd;padding:7px;text-align:left}}tr.pass{{background:#effbef}}tr.fail{{background:#fff0f0}}tr.timeout{{background:#fff8df}}</style><h1>MeowEngine selected WPT triage</h1><div class=cards><div class=card><b>{}/{}</b><br>passed</div><div class=card><b>{:.2}%</b><br>pass rate</div><div class=card><b>{}</b><br>fail</div><div class=card><b>{}</b><br>timeout</div></div><p>Manifest digest: <code>{}</code>. Target: {:.2}%.</p><table><thead><tr><th>Case</th><th>Suite</th><th>Upstream selection</th><th>Status</th><th>ms</th><th>Message</th></tr></thead><tbody>{rows}</tbody></table>",
        report.summary.passed,
        report.summary.total,
        report.summary.pass_rate * 100.0,
        report.summary.failed,
        report.summary.timed_out,
        report.manifest_digest,
        report.selected_pass_rate_target * 100.0,
    )
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn assert_contains_all(value: &str, fragments: &[String]) -> Result<(), String> {
    for fragment in fragments {
        if !value.contains(fragment) {
            return Err(format!("output did not contain {fragment:?}"));
        }
    }
    Ok(())
}

fn equal<T: PartialEq + std::fmt::Debug + ?Sized>(
    label: &str,
    actual: &T,
    expected: &T,
) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{label} mismatch: actual={actual:?} expected={expected:?}"
        ))
    }
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
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

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Debug)]
struct Options {
    manifest: PathBuf,
    baseline: PathBuf,
    output: PathBuf,
    suite: Option<String>,
    timeout_ms: u64,
    update_baseline: bool,
}

impl Options {
    fn parse(arguments: Vec<OsString>) -> io::Result<Self> {
        let mut options = Self {
            manifest: PathBuf::from(DEFAULT_MANIFEST),
            baseline: PathBuf::from(DEFAULT_BASELINE),
            output: PathBuf::from(DEFAULT_OUTPUT),
            suite: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            update_baseline: false,
        };
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let text = argument.to_string_lossy();
            match text.as_ref() {
                "--update-baseline" => options.update_baseline = true,
                "--check" => {}
                "--manifest" => {
                    options.manifest = PathBuf::from(next(&mut arguments, "--manifest")?)
                }
                "--baseline" => {
                    options.baseline = PathBuf::from(next(&mut arguments, "--baseline")?)
                }
                "--output" => options.output = PathBuf::from(next(&mut arguments, "--output")?),
                "--suite" => {
                    options.suite = Some(utf8(next(&mut arguments, "--suite")?, "--suite")?)
                }
                "--timeout-ms" => {
                    options.timeout_ms =
                        utf8(next(&mut arguments, "--timeout-ms")?, "--timeout-ms")?
                            .parse()
                            .map_err(|_| {
                                io::Error::new(io::ErrorKind::InvalidInput, "invalid timeout")
                            })?;
                }
                _ if text.starts_with("--manifest=") => {
                    options.manifest = PathBuf::from(&text[11..])
                }
                _ if text.starts_with("--baseline=") => {
                    options.baseline = PathBuf::from(&text[11..])
                }
                _ if text.starts_with("--output=") => options.output = PathBuf::from(&text[9..]),
                _ if text.starts_with("--suite=") => options.suite = Some(text[8..].to_owned()),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown WPT option {argument:?}"),
                    ));
                }
            }
        }
        if options.timeout_ms == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "timeout must be positive",
            ));
        }
        Ok(options)
    }
}

fn next(arguments: &mut impl Iterator<Item = OsString>, option: &str) -> io::Result<OsString> {
    arguments.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{option} requires a value"),
        )
    })
}

fn utf8(value: OsString, option: &str) -> io::Result<String> {
    value.into_string().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{option} must be UTF-8"),
        )
    })
}

#[derive(Clone, Debug, Deserialize)]
struct Manifest {
    pass_rate_target: f64,
    cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
struct Case {
    id: String,
    suite: String,
    upstream: String,
    #[serde(flatten)]
    test: Test,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Test {
    HtmlDump {
        source: String,
        contains: Vec<String>,
    },
    Selector {
        source: String,
        selector: String,
        expected_ids: Vec<String>,
    },
    Cascade {
        source: String,
        css: String,
        target: String,
        property: String,
        expected: String,
    },
    LayoutRef {
        test_html: String,
        test_css: String,
        reference_html: String,
        reference_css: String,
        width: u32,
        height: u32,
    },
    Accessibility {
        source: String,
        expectations: Vec<AccessibilityExpectation>,
        focus_ids: Vec<String>,
    },
    KeyboardAudit {
        source: String,
        focus_ids: Vec<String>,
        finding_codes: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
struct AccessibilityExpectation {
    id: String,
    role: String,
    name: String,
    focusable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Pass,
    Fail,
    Timeout,
}

impl Status {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CaseResult {
    id: String,
    suite: String,
    upstream: String,
    status: Status,
    duration_ms: u64,
    message: Option<String>,
}

impl CaseResult {
    fn failed(case: &Case, duration_ms: u64, message: String) -> Self {
        Self {
            id: case.id.clone(),
            suite: case.suite.clone(),
            upstream: case.upstream.clone(),
            status: Status::Fail,
            duration_ms,
            message: Some(message),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct Report {
    schema_version: u32,
    manifest_digest: String,
    selected_pass_rate_target: f64,
    summary: Summary,
    results: Vec<CaseResult>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct Summary {
    total: usize,
    passed: usize,
    failed: usize,
    timed_out: usize,
    pass_rate: f64,
}

impl Summary {
    fn from_results(results: &[CaseResult]) -> Self {
        let passed = results
            .iter()
            .filter(|result| result.status == Status::Pass)
            .count();
        let failed = results
            .iter()
            .filter(|result| result.status == Status::Fail)
            .count();
        let timed_out = results
            .iter()
            .filter(|result| result.status == Status::Timeout)
            .count();
        Self {
            total: results.len(),
            passed,
            failed,
            timed_out,
            pass_rate: passed as f64 / results.len() as f64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Baseline {
    schema_version: u32,
    manifest_digest: String,
    results: Vec<BaselineEntry>,
}

impl Baseline {
    fn from_report(report: &Report) -> Self {
        Self {
            schema_version: report.schema_version,
            manifest_digest: report.manifest_digest.clone(),
            results: report
                .results
                .iter()
                .map(|result| BaselineEntry {
                    id: result.id.clone(),
                    status: result.status,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BaselineEntry {
    id: String,
    status: Status,
}
