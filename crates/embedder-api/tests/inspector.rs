use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use meow_embedder_api::{BrowserEngine, CancellationToken};

#[test]
fn inspector_captures_layout_failed_request_network_and_console() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let count = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]);
            if index == 0 {
                assert!(request.starts_with("GET /page"));
                let body = br#"<!doctype html><title>Inspector Cat</title><link rel='stylesheet' href='/missing.css'><style>#root{width:120px}</style><main id='root'><p>cat</p></main><script>console.error('layout cat')</script>"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            } else {
                assert!(request.starts_with("GET /missing.css"));
                let body = b"missing";
                write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
            stream.flush().unwrap();
        }
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut engine = BrowserEngine::new();
    runtime
        .block_on(engine.navigate(&format!("http://{address}/page"), &CancellationToken::new()))
        .unwrap();
    let snapshot = engine.inspector_snapshot(640, 480).unwrap();
    server.join().unwrap();

    assert!(snapshot.has_required_panels());
    assert!(snapshot.dom_tree.contains("<main id=\"root\""));
    assert!(snapshot.computed_style.contains("element slot="));
    assert!(snapshot.box_model.contains("principal-block"));
    assert!(snapshot.layout_tree.contains("#layout-tree"));
    assert_eq!(snapshot.network_waterfall.len(), 2);
    assert_eq!(snapshot.network_waterfall[1].status, Some(404));
    assert!(!snapshot.stylesheet_errors.is_empty());
    assert!(
        snapshot
            .console
            .iter()
            .any(|entry| entry.message.contains("layout cat"))
    );
    assert!(snapshot.accessibility_tree.is_object());
}
