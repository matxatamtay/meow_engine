use std::{
    path::Path,
    process::{Command, ExitCode},
};

pub fn run(command: &str, arguments: &[String]) -> ExitCode {
    let status = match command {
        "wpt" => workspace_command("cargo")
            .args(["run", "-p", "meow-wpt", "--"])
            .args(arguments)
            .status(),
        "fuzz" => workspace_command("cargo")
            .args(["run", "-p", "meow-fuzz", "--"])
            .args(arguments)
            .status(),
        "budgets" => workspace_command("cargo")
            .args(["run", "-p", "meow-bench", "--release", "--"])
            .args(arguments)
            .status(),
        "package" | "diagnostics" | "release-check" => {
            let script_command = if command == "release-check" {
                "verify"
            } else {
                command
            };
            workspace_command("python3")
                .args(["scripts/release.py", script_command])
                .args(arguments)
                .status()
        }
        "supply-chain" => workspace_command("python3")
            .arg("scripts/supply_chain.py")
            .args(arguments)
            .status(),
        "v8-verify" => workspace_command("python3")
            .args(["scripts/supply_chain.py", "verify-v8"])
            .args(arguments)
            .status(),
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

fn workspace_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    command
}
