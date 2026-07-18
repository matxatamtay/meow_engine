use std::process::Command;

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
}

#[test]
fn help_succeeds() {
    let output = xtask().arg("--help").output().expect("run xtask help");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("cargo xtask doctor"));
}

#[test]
fn unknown_command_returns_usage_error() {
    let output = xtask().arg("nope").output().expect("run unknown command");

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown xtask command: nope"));
}

#[test]
fn doctor_accepts_the_bootstrapped_repository() {
    let output = xtask().arg("doctor").output().expect("run xtask doctor");

    assert!(
        output.status.success(),
        "doctor failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("bootstrap is healthy"));
}
