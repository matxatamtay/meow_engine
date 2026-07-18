use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const REQUIRED_FILES: &[&str] = &[
    "Cargo.toml",
    "rust-toolchain.toml",
    "LICENSE",
    ".github/CODEOWNERS",
    ".github/workflows/ci.yml",
    "docs/adr/0000-template.md",
];

pub(crate) fn run() -> ExitCode {
    let workspace = workspace_root();
    if let Err(error) = check_required_files(&workspace) {
        eprintln!("[doctor] {error}");
        return ExitCode::FAILURE;
    }
    let commands: &[(&str, &str, &[&str])] = &[
        ("rustc", "rustc", &["--version"]),
        ("cargo", "cargo", &["--version"]),
        ("rustfmt", "cargo", &["fmt", "--version"]),
        ("clippy", "cargo", &["clippy", "--version"]),
        (
            "workspace metadata",
            "cargo",
            &["metadata", "--no-deps", "--format-version", "1"],
        ),
    ];
    for (name, program, args) in commands {
        if let Err(error) = run_check(&workspace, name, program, args) {
            eprintln!("[doctor] {error}");
            return ExitCode::FAILURE;
        }
    }
    println!("[doctor] MeowEngine bootstrap is healthy.");
    ExitCode::SUCCESS
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask must live at tools/xtask")
        .to_path_buf()
}

fn check_required_files(workspace: &Path) -> Result<(), String> {
    for relative_path in REQUIRED_FILES {
        let path = workspace.join(relative_path);
        print!("[doctor] checking {relative_path}... ");
        flush_stdout()?;
        if !path.is_file() {
            println!("missing");
            return Err(format!("required file is missing: {relative_path}"));
        }

        println!("ok");
    }

    Ok(())
}

fn run_check(workspace: &Path, name: &str, program: &str, args: &[&str]) -> Result<(), String> {
    print!("[doctor] checking {name}... ");
    flush_stdout()?;

    let output = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|error| format!("could not run {program}: {error}"))?;

    if output.status.success() {
        if name == "workspace metadata" {
            println!("ok");
        } else {
            let detail = String::from_utf8_lossy(&output.stdout);
            println!("{}", detail.lines().next().unwrap_or("ok"));
        }
        return Ok(());
    }

    println!("failed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();

    if message.is_empty() {
        Err(format!("{name} exited with {}", output.status))
    } else {
        Err(format!("{name} failed: {message}"))
    }
}

fn flush_stdout() -> Result<(), String> {
    io::stdout()
        .flush()
        .map_err(|error| format!("could not flush stdout: {error}"))
}
