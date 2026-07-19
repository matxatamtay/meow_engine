use std::process::Command;

#[test]
#[cfg(target_os = "linux")]
fn browser_binary_hosts_isolated_children_and_recovers_content() {
    let output = Command::new(env!("CARGO_BIN_EXE_meow-browser"))
        .arg("--process-smoke-test")
        .output()
        .expect("browser process smoke should start");
    assert!(
        output.status.success(),
        "browser smoke failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"browser_binary_multiprocess\":true"));
    assert!(stdout.contains("\"content_crash_contained\":true"));
    assert!(stdout.contains("\"seccomp_applied\":true"));
}
