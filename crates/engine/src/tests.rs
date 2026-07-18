use std::net::SocketAddr;

use meow_display_list::Viewport;
use meow_net::NetError;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

use super::{encoding::sniff_encoding, *};

#[test]
fn engine_outputs_commands_without_selecting_a_renderer() {
    let viewport = Viewport::new(320, 200).expect("viewport should be valid");
    let list = Engine::new()
        .build_display_list(viewport)
        .expect("frame should build");

    assert!(!list.is_empty());
}

#[test]
fn navigator_starts_with_committed_about_blank() {
    let navigator = Navigator::default();

    assert_eq!(navigator.current().url.as_str(), "about:blank");
    assert_eq!(navigator.current().base_url.as_str(), "about:blank");
    assert_eq!(
        navigator.current().charset_source,
        CharsetSource::AboutBlank
    );
    assert_eq!(navigator.history().len(), 1);
    assert!(navigator.current().document.dump().contains("<html>"));
    assert!(navigator.current().stylesheets.is_empty());
}

#[tokio::test]
async fn loads_url_parses_dom_resolves_base_and_commits_history() {
    let server = TestServer::spawn().await;
    let mut navigator = Navigator::default();
    let url = server.url("/page");

    navigator
        .navigate(url.as_str(), &CancellationToken::new())
        .await
        .expect("navigation should commit");
    let state = navigator.current();

    assert_eq!(state.url, url);
    assert_eq!(state.base_url, server.url("/assets/"));
    assert_eq!(state.encoding, "windows-1252");
    assert_eq!(state.charset_source, CharsetSource::HttpHeader);
    assert!(state.document.dump().contains("café"));
    assert_eq!(state.history_index, 1);
    assert_eq!(
        state.response.as_ref().unwrap().received_bytes,
        server.body_len()
    );
    assert_eq!(navigator.history().len(), 2);
    assert_eq!(navigator.history()[1].url, url);
}

#[tokio::test]
async fn loads_inline_and_external_stylesheets_in_document_order() {
    let server = TestServer::spawn().await;
    let mut navigator = Navigator::default();

    navigator
        .navigate(server.url("/styled").as_str(), &CancellationToken::new())
        .await
        .expect("styled navigation should commit");
    let state = navigator.current();

    assert_eq!(state.stylesheets.len(), 2);
    assert!(state.stylesheet_errors.is_empty());
    assert!(matches!(
        state.stylesheets[0].source,
        StylesheetSource::Inline { .. }
    ));
    assert_eq!(state.stylesheets[0].media.as_deref(), Some("screen"));
    assert_eq!(state.stylesheets[0].stylesheet.diagnostics.len(), 1);
    assert!(matches!(
        state.stylesheets[1].source,
        StylesheetSource::External { .. }
    ));
    assert_eq!(state.stylesheets[1].media.as_deref(), Some("print"));
    let dump = state.dump_stylesheets();
    assert!(dump.contains(r#"selectors="main""#));
    assert!(dump.contains(r#"selectors=".card""#));
    assert!(dump.contains("important=true"));
}

#[tokio::test]
async fn linked_stylesheet_failures_are_non_fatal_and_reported() {
    let server = TestServer::spawn().await;
    let mut navigator = Navigator::default();

    navigator
        .navigate(
            server.url("/style-errors").as_str(),
            &CancellationToken::new(),
        )
        .await
        .expect("document should commit despite stylesheet failures");
    let state = navigator.current();

    assert_eq!(state.url, server.url("/style-errors"));
    assert_eq!(state.stylesheets.len(), 1);
    assert_eq!(state.stylesheet_errors.len(), 2);
    assert!(state.stylesheet_errors.iter().any(|error| {
        error.href.as_deref() == Some("/missing.css") && error.message.contains("404 Not Found")
    }));
    assert!(state.stylesheet_errors.iter().any(|error| {
        error
            .href
            .as_deref()
            .is_some_and(|href| href.starts_with("data:text/css"))
            && error.message.contains("unsupported URL scheme")
    }));
    assert!(state.dump_stylesheets().contains("stylesheet-error[1]"));
}

#[tokio::test]
async fn failed_navigation_does_not_replace_committed_document() {
    let mut navigator = Navigator::default();
    let before_dump = navigator.current().document.dump();
    let before_history = navigator.history().len();

    let error = navigator
        .navigate("data:text/html,cat", &CancellationToken::new())
        .await
        .expect_err("unsupported scheme should fail before commit");

    assert!(matches!(
        error,
        NavigationError::Network(NetError::UnsupportedScheme(_))
    ));
    assert_eq!(navigator.current().url.as_str(), "about:blank");
    assert_eq!(navigator.current().document.dump(), before_dump);
    assert_eq!(navigator.history().len(), before_history);
}

#[test]
fn charset_sniffing_prefers_bom_header_meta_then_default() {
    assert_eq!(
        sniff_encoding(b"\xef\xbb\xbf<p>x", Some("text/html; charset=shift_jis")).1,
        CharsetSource::Bom
    );
    assert_eq!(
        sniff_encoding(b"<p>x", Some("text/html; charset=utf-8")).1,
        CharsetSource::HttpHeader
    );
    assert_eq!(
        sniff_encoding(b"<meta charset='utf-8'><p>x", Some("text/html")).1,
        CharsetSource::Meta
    );
    assert_eq!(
        sniff_encoding(b"<p>x", Some("text/html")).1,
        CharsetSource::Default
    );
}

struct TestServer {
    address: SocketAddr,
    task: JoinHandle<()>,
    body: Vec<u8>,
}

impl TestServer {
    async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener.local_addr().unwrap();
        let body = b"<!doctype html><base href='/assets/'><p>caf\xe9</p>".to_vec();
        let response_body = body.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let body = response_body.clone();
                tokio::spawn(handle_connection(stream, body));
            }
        });
        Self {
            address,
            task,
            body,
        }
    }

    fn url(&self, path: &str) -> BrowserUrl {
        BrowserUrl::parse(&format!("http://{}{path}", self.address)).unwrap()
    }

    fn body_len(&self) -> usize {
        self.body.len()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_connection(mut stream: TcpStream, body: Vec<u8>) {
    let mut request = [0_u8; 4096];
    let Ok(read) = stream.read(&mut request).await else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (status, content_type, response_body) = match path {
        "/page" => ("200 OK", "text/html; charset=windows-1252", body),
        "/styled" => (
            "200 OK",
            "text/html; charset=utf-8",
            br#"<!doctype html>
                <style media="screen">main { color: red; broken; }</style>
                <link rel="stylesheet" href="/style.css" media="print">
                <main class="card">styled</main>"#
                .to_vec(),
        ),
        "/style.css" => (
            "200 OK",
            "text/css; charset=utf-8",
            b".card { display: block !important; }".to_vec(),
        ),
        "/style-errors" => (
            "200 OK",
            "text/html; charset=utf-8",
            br#"<!doctype html>
                <style>main { color: green; }</style>
                <link rel="stylesheet" href="/missing.css">
                <link rel="stylesheet" href="data:text/css,p%7Bcolor:red%7D">
                <main>still committed</main>"#
                .to_vec(),
        ),
        _ => ("404 Not Found", "text/plain; charset=utf-8", Vec::new()),
    };
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response_body.len()
    );
    let _ = stream.write_all(headers.as_bytes()).await;
    let _ = stream.write_all(&response_body).await;
    let _ = stream.shutdown().await;
}
