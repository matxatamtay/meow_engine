use std::process::Command;

#[test]
#[cfg(target_os = "linux")]
fn isolated_content_crash_does_not_kill_shell_or_network_process() {
    let output = Command::new(env!("CARGO_BIN_EXE_meow-process-probe"))
        .output()
        .expect("multiprocess probe should start");
    assert!(
        output.status.success(),
        "probe failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"content_crash_contained\":true"));
    assert!(stdout.contains("\"network_process_survived\":true"));
    assert!(stdout.contains("\"brokered_http\":true"));
    assert!(stdout.contains("\"seccomp_applied\":true"));
}
