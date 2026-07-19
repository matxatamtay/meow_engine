use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const PNG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[test]
fn release_candidate_renders_curated_site_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap();
    let site_root = root.join("tests/curated-sites");
    let cases = ["article", "forms", "flex-media", "scripted"];
    let pages = cases
        .iter()
        .map(|id| {
            (
                format!("/{id}"),
                fs::read(site_root.join(format!("{id}.html"))).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..pages.len() {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let count = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let body = pages
                .iter()
                .find(|(candidate, _)| candidate == path)
                .map(|(_, body)| body.as_slice())
                .unwrap_or(b"not found");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
        }
    });

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_root = env::temp_dir().join(format!("meow-curated-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&output_root).unwrap();
    for id in cases {
        let output_path = output_root.join(format!("{id}.png"));
        let output = Command::new(env!("CARGO_BIN_EXE_meow-headless"))
            .args(["--width=800", "--height=600", "--output"])
            .arg(&output_path)
            .arg("--url")
            .arg(format!("http://{address}/{id}"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{id} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let png = fs::read(output_path).unwrap();
        assert_eq!(
            png.get(..8),
            Some(PNG.as_slice()),
            "{id} did not produce PNG"
        );
        assert!(png.len() > 1000, "{id} PNG was unexpectedly small");
    }
    server.join().unwrap();
    let _ = fs::remove_dir_all(output_root);
}
