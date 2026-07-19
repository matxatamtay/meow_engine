use std::process::Command;

#[test]
#[cfg(target_os = "linux")]
fn subprocess_applies_rlimits_and_denies_new_network_sockets() {
    let output = Command::new(env!("CARGO_BIN_EXE_meow-sandbox-probe"))
        .output()
        .expect("sandbox probe should start");
    assert!(
        output.status.success(),
        "probe failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"seccomp_applied\":true"));
    assert!(stdout.contains("nofile=128"));
}
