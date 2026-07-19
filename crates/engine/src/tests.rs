use std::net::SocketAddr;

use meow_css::{PropertyId, parse_selector_list};
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

    let main = state
        .document
        .query_selector(&parse_selector_list("main.card").unwrap())
        .expect("styled document should contain main.card");
    let computed = state.computed_styles();
    let style = computed.style_for(main.id()).unwrap();
    assert_eq!(style.get(PropertyId::Color), "red");
    assert_eq!(
        style.get(PropertyId::Display),
        "block",
        "inactive print CSS must not override the HTML user-agent display rule"
    );
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

#[tokio::test]
async fn back_forward_reload_and_branching_preserve_session_history() {
    let server = TestServer::spawn().await;
    let mut navigator = Navigator::default();
    let cancellation = CancellationToken::new();

    navigator
        .navigate(server.url("/page").as_str(), &cancellation)
        .await
        .unwrap();
    navigator
        .navigate(server.url("/styled").as_str(), &cancellation)
        .await
        .unwrap();
    assert_eq!(navigator.history().len(), 3);
    assert!(navigator.can_go_back());
    assert!(!navigator.can_go_forward());

    navigator.back(&cancellation).await.unwrap().unwrap();
    assert_eq!(navigator.current().url, server.url("/page"));
    assert!(navigator.can_go_forward());

    navigator.forward(&cancellation).await.unwrap().unwrap();
    assert_eq!(navigator.current().url, server.url("/styled"));
    let history_len = navigator.history().len();
    navigator.reload(&cancellation).await.unwrap();
    assert_eq!(navigator.history().len(), history_len);
    assert_eq!(navigator.current().history_index, 2);

    navigator.back(&cancellation).await.unwrap().unwrap();
    navigator
        .navigate(server.url("/style-errors").as_str(), &cancellation)
        .await
        .unwrap();
    assert_eq!(navigator.history().len(), 3);
    assert_eq!(navigator.current().history_index, 2);
    assert_eq!(navigator.current().url, server.url("/style-errors"));
    assert!(!navigator.can_go_forward());
}

#[tokio::test]
async fn classic_scripts_mutate_dom_restyle_and_preserve_blocking_defer_order() {
    let server = TestServer::spawn().await;
    let mut navigator = Navigator::default();

    navigator
        .navigate(server.url("/scripts").as_str(), &CancellationToken::new())
        .await
        .expect("scripted navigation should commit");
    let state = navigator.current();

    assert_eq!(state.script_executions.len(), 5);
    assert!(
        state
            .script_executions
            .iter()
            .all(ScriptExecution::succeeded)
    );
    assert_eq!(
        state
            .script_executions
            .iter()
            .map(|execution| execution.phase)
            .collect::<Vec<_>>(),
        vec![
            ScriptExecutionPhase::ParserBlocking,
            ScriptExecutionPhase::ParserBlocking,
            ScriptExecutionPhase::ParserBlocking,
            ScriptExecutionPhase::Deferred,
            ScriptExecutionPhase::Deferred,
        ]
    );
    assert!(!state.script_mutations.is_empty());

    let target = state
        .document
        .query_selector(&parse_selector_list("#target").unwrap())
        .expect("target should survive script execution");
    assert_eq!(
        state
            .document
            .element_attribute(&target, "class")
            .as_deref(),
        Some("hot")
    );
    assert_eq!(state.document.text_content(&target), "changed");
    let title = state
        .document
        .query_selector(&parse_selector_list("title").unwrap())
        .unwrap();
    assert_eq!(
        state.document.text_content(&title),
        "inline-1>external-blocking>inline-2>defer-a>defer-b"
    );

    let styles = state.computed_styles();
    assert_eq!(
        styles
            .style_for(target.id())
            .unwrap()
            .get(PropertyId::Color),
        "red",
        "script attribute mutation must be visible to the post-script style pass"
    );
    let dump = state.dump_scripts();
    assert!(dump.contains("phase=Deferred"));
    assert!(dump.contains("script-mutations="));
}

#[tokio::test]
async fn script_exceptions_and_load_failures_are_non_fatal_and_ordered() {
    let server = TestServer::spawn().await;
    let mut navigator = Navigator::default();

    navigator
        .navigate(
            server.url("/script-errors").as_str(),
            &CancellationToken::new(),
        )
        .await
        .expect("script failures must not abort document commit");
    let state = navigator.current();

    assert_eq!(state.script_executions.len(), 3);
    assert_eq!(
        state.script_executions[0].error.as_ref().unwrap().kind,
        ScriptErrorKind::Exception
    );
    assert_eq!(
        state.script_executions[1].error.as_ref().unwrap().kind,
        ScriptErrorKind::Load
    );
    assert!(state.script_executions[2].succeeded());
    let title = state
        .document
        .query_selector(&parse_selector_list("title").unwrap())
        .unwrap();
    assert_eq!(state.document.text_content(&title), "after-errors");
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
        "/scripts" => (
            "200 OK",
            "text/html; charset=utf-8",
            br#"<!doctype html>
                <title>start</title>
                <style>.hot { color: red; }</style>
                <main id="target">old</main>
                <script>
                    window.order = ['inline-1'];
                    const target = document.querySelector('#target');
                    target.setAttribute('class', 'hot');
                    target.textContent = 'changed';
                </script>
                <script src="/classic.js"></script>
                <script defer src="/defer-a.js"></script>
                <script>window.order.push('inline-2');</script>
                <script defer src="/defer-b.js"></script>"#
                .to_vec(),
        ),
        "/script-errors" => (
            "200 OK",
            "text/html; charset=utf-8",
            br#"<!doctype html>
                <title>before</title>
                <script>throw new Error('boom')</script>
                <script src="/missing.js"></script>
                <script>document.title = 'after-errors'</script>"#
                .to_vec(),
        ),
        "/classic.js" => (
            "200 OK",
            "text/javascript; charset=utf-8",
            b"window.order.push('external-blocking');".to_vec(),
        ),
        "/defer-a.js" => (
            "200 OK",
            "application/javascript; charset=utf-8",
            b"window.order.push('defer-a');".to_vec(),
        ),
        "/defer-b.js" => (
            "200 OK",
            "text/javascript; charset=utf-8",
            b"window.order.push('defer-b'); document.title = window.order.join('>');".to_vec(),
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
