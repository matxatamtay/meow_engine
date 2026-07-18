use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some("doctor") => doctor(),
        _ => {
            eprintln!("usage: cargo xtask doctor");
            ExitCode::from(2)
        }
    }
}

fn doctor() -> ExitCode {
    let checks: &[(&str, &[&str])] = &[
        ("format", &["fmt", "--all", "--", "--check"]),
        ("workspace", &["check", "--workspace", "--all-targets"]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("tests", &["test", "--workspace"]),
    ];

    for (name, args) in checks {
        println!("[doctor] checking {name}...");

        match Command::new("cargo").args(*args).status() {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("[doctor] {name} failed with {status}");
                return ExitCode::FAILURE;
            }
            Err(error) => {
                eprintln!("[doctor] could not run cargo for {name}: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    println!("[doctor] MeowEngine workspace is healthy.");
    ExitCode::SUCCESS
}
