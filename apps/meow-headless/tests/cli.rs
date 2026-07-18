use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[test]
fn writes_byte_identical_pngs_for_identical_inputs() {
    let directory = temporary_directory("deterministic");
    let first_path = directory.join("first.png");
    let second_path = directory.join("second.png");

    let first_run = render(&first_path, 320, 200);
    let second_run = render(&second_path, 320, 200);
    assert_success(&first_run);
    assert_success(&second_run);

    let first = fs::read(&first_path).expect("first PNG should exist");
    let second = fs::read(&second_path).expect("second PNG should exist");

    assert_eq!(first, second);
    assert_eq!(first.get(..8), Some(PNG_SIGNATURE.as_slice()));
    assert_eq!(png_dimensions(&first), Some((320, 200)));
    assert_eq!(first.len(), 6_350);
    assert_eq!(fnv1a64(&first), 0x99cd_e2cb_3f11_5bba);

    fs::remove_dir_all(directory).expect("temporary directory should be removable");
}

#[test]
fn creates_missing_output_directories() {
    let directory = temporary_directory("nested");
    let output_path = directory.join("a/b/reference.png");

    let result = render(&output_path, 128, 96);

    assert_success(&result);
    assert!(output_path.is_file());
    fs::remove_dir_all(directory).expect("temporary directory should be removable");
}

fn render(output: &Path, width: u32, height: u32) -> Output {
    Command::new(env!("CARGO_BIN_EXE_meow-headless"))
        .arg("--output")
        .arg(output)
        .arg(format!("--width={width}"))
        .arg(format!("--height={height}"))
        .output()
        .expect("meow-headless should run")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "process failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..8)? != PNG_SIGNATURE || bytes.get(12..16)? != b"IHDR" {
        return None;
    }

    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let directory = env::temp_dir().join(format!(
        "meow-headless-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("temporary directory should be creatable");
    directory
}
