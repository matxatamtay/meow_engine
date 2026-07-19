use std::{fs, process::Command};

#[test]
fn selected_manifest_matches_reproducible_baseline() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let output_dir = std::env::temp_dir().join(format!("meow-wpt-{}", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_meow-wpt"))
        .current_dir(root)
        .args(["--check", "--output"])
        .arg(&output_dir)
        .output()
        .expect("run selected WPT");
    assert!(
        output.status.success(),
        "WPT failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_dir.join("report.json").is_file());
    assert!(output_dir.join("dashboard.html").is_file());
    let _ = fs::remove_dir_all(output_dir);
}
