use std::process::{Command, ExitCode};

pub fn run(command: &str, arguments: &[String]) -> ExitCode {
    let status = match command {
        "wpt" => Command::new("cargo")
            .args(["run", "-p", "meow-wpt", "--"])
            .args(arguments)
            .status(),
        "fuzz" => Command::new("cargo")
            .args(["run", "-p", "meow-fuzz", "--"])
            .args(arguments)
            .status(),
        "budgets" => Command::new("cargo")
            .args(["run", "-p", "meow-bench", "--release", "--"])
            .args(arguments)
            .status(),
        "package" | "diagnostics" | "release-check" => {
            let script_command = if command == "release-check" {
                "verify"
            } else {
                command
            };
            Command::new("python3")
                .args(["scripts/release.py", script_command])
                .args(arguments)
                .status()
        }
        _ => unreachable!("known release command"),
    };
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!("failed to run {command}: {error}");
            ExitCode::FAILURE
        }
    }
}
