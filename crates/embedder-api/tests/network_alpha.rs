use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use meow_embedder_api::{BrowserEngine, CancellationToken};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn same_origin_fetch_redirect_abort_and_cookie_chain_work() {
    let server = TestServer::spawn(Arc::new(|request| match request.path.as_str() {
        "/" => TestResponse::html(
            r#"<!doctype html><p id='json'>pending</p><p id='abort'>pending</p><p id='cookie'>pending</p>
<script>
const json = document.querySelector('#json');
const aborted = document.querySelector('#abort');
const cookie = document.querySelector('#cookie');
fetch('/redirect')
  .then(response => response.json().then(data => ({ data, redirected: response.redirected })))
  .then(result => json.textContent = result.data.value + ':' + result.redirected)
  .catch(error => json.textContent = 'error:' + error.name);
const controller = new AbortController();
fetch('/slow', { signal: controller.signal })
  .then(() => aborted.textContent = 'missed')
  .catch(error => aborted.textContent = error.name);
controller.abort();
fetch('/cookie/set', { credentials: 'same-origin' })
  .then(() => fetch('/cookie/echo', { credentials: 'same-origin' }))
  .then(response => response.text())
  .then(value => cookie.textContent = value)
  .catch(error => cookie.textContent = 'error:' + error.name);
</script>"#,
        ),
        "/redirect" => TestResponse::redirect("/api"),
        "/api" => TestResponse::json(r#"{"value":"meow"}"#),
        "/slow" => TestResponse::text("should not be requested"),
        "/cookie/set" => TestResponse::new(200, "OK", "set")
            .header("Set-Cookie", "sid=cat; Path=/; SameSite=Lax"),
        "/cookie/echo" => TestResponse::text(
            request
                .headers
                .get("cookie")
                .map(String::as_str)
                .unwrap_or("missing"),
        ),
        _ => TestResponse::not_found(),
    }))
    .await;

    let mut engine = BrowserEngine::new();
    engine
        .navigate(&server.url("/"), &CancellationToken::new())
        .await
        .unwrap();
    pump_until_idle(&mut engine).await;

    assert_eq!(element_text(&engine, "json"), "meow:true");
    assert_eq!(element_text(&engine, "abort"), "AbortError");
    assert_eq!(element_text(&engine, "cookie"), "sid=cat");
}

#[tokio::test]
async fn cors_denial_exact_origin_and_preflight_are_enforced() {
    let preflight_seen = Arc::new(AtomicBool::new(false));
    let preflight_seen_for_server = Arc::clone(&preflight_seen);
    let cross = TestServer::spawn(Arc::new(move |request| {
        let request_origin = request.headers.get("origin").cloned().unwrap_or_default();
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/denied") => TestResponse::json(r#"{"value":"leak"}"#),
            ("GET", "/allowed") => TestResponse::json(r#"{"value":"cors-ok"}"#)
                .header("Access-Control-Allow-Origin", &request_origin),
            ("OPTIONS", "/preflight") => {
                preflight_seen_for_server.store(true, Ordering::Release);
                TestResponse::new(204, "No Content", "")
                    .header("Access-Control-Allow-Origin", &request_origin)
                    .header("Access-Control-Allow-Methods", "PUT")
                    .header("Access-Control-Allow-Headers", "x-meow")
            }
            ("PUT", "/preflight") => TestResponse::json(&format!(
                r#"{{"method":"{}","header":"{}","body":"{}"}}"#,
                request.method,
                request
                    .headers
                    .get("x-meow")
                    .map(String::as_str)
                    .unwrap_or(""),
                request.body
            ))
            .header("Access-Control-Allow-Origin", &request_origin),
            _ => TestResponse::not_found(),
        }
    }))
    .await;

    let cross_base = cross.url("");
    let page = TestServer::spawn(Arc::new(move |request| {
        if request.path != "/" {
            return TestResponse::not_found();
        }
        TestResponse::html(&format!(
            r#"<!doctype html><p id='denied'>pending</p><p id='allowed'>pending</p><p id='preflight'>pending</p>
<script>
const denied = document.querySelector('#denied');
const allowed = document.querySelector('#allowed');
const preflight = document.querySelector('#preflight');
fetch('{cross_base}/denied')
  .then(() => denied.textContent = 'leaked')
  .catch(() => denied.textContent = 'blocked');
fetch('{cross_base}/allowed')
  .then(response => response.json())
  .then(data => allowed.textContent = data.value)
  .catch(error => allowed.textContent = 'error:' + error.name);
fetch('{cross_base}/preflight', {{
  method: 'PUT',
  headers: {{ 'x-meow': 'yes' }},
  body: 'payload'
}})
  .then(response => response.json())
  .then(data => preflight.textContent = data.method + ':' + data.header + ':' + data.body)
  .catch(error => preflight.textContent = 'error:' + error.name);
</script>"#,
        ))
    }))
    .await;

    let mut engine = BrowserEngine::new();
    engine
        .navigate(&page.url("/"), &CancellationToken::new())
        .await
        .unwrap();
    pump_until_idle(&mut engine).await;

    assert_eq!(element_text(&engine, "denied"), "blocked");
    assert_eq!(element_text(&engine, "allowed"), "cors-ok");
    assert_eq!(element_text(&engine, "preflight"), "PUT:yes:payload");
    assert!(preflight_seen.load(Ordering::Acquire));
}

#[tokio::test]
async fn local_storage_survives_reload_and_profile_restart() {
    let server = TestServer::spawn(Arc::new(|request| {
        if request.path != "/" {
            return TestResponse::not_found();
        }
        TestResponse::html(
            r#"<!doctype html><p id='counts'></p><script>
const local = Number(localStorage.getItem('count') || '0') + 1;
const session = Number(sessionStorage.getItem('count') || '0') + 1;
localStorage.setItem('count', String(local));
sessionStorage.setItem('count', String(session));
document.querySelector('#counts').textContent = local + ':' + session;
</script>"#,
        )
    }))
    .await;
    let profile = temporary_profile();

    {
        let mut engine = BrowserEngine::new_with_profile(&profile);
        engine
            .navigate(&server.url("/"), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(element_text(&engine, "counts"), "1:1");
        engine.reload(&CancellationToken::new()).await.unwrap();
        assert_eq!(element_text(&engine, "counts"), "2:2");
    }

    {
        let mut engine = BrowserEngine::new_with_profile(&profile);
        engine
            .navigate(&server.url("/"), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(element_text(&engine, "counts"), "3:1");
    }

    let _ = std::fs::remove_dir_all(profile);
}

async fn pump_until_idle(engine: &mut BrowserEngine) {
    for _ in 0..16 {
        let report = engine.pump_web_tasks().await;
        assert!(
            report.errors.is_empty(),
            "web task errors: {:?}",
            report.errors
        );
        if !engine.has_pending_web_tasks() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("web task queue did not become idle");
}

fn element_text(engine: &BrowserEngine, id: &str) -> String {
    let document = &engine.current_document().document;
    let element = document
        .elements_in_tree_order()
        .into_iter()
        .find(|element| document.element_attribute(element, "id").as_deref() == Some(id))
        .expect("element should exist");
    document.text_content(&element)
}

fn temporary_profile() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "meow-profile-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[derive(Clone, Debug)]
struct TestRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: String,
}

#[derive(Clone, Debug)]
struct TestResponse {
    status: u16,
    reason: &'static str,
    headers: Vec<(String, String)>,
    body: String,
}

impl TestResponse {
    fn new(status: u16, reason: &'static str, body: impl Into<String>) -> Self {
        Self {
            status,
            reason,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    fn html(body: &str) -> Self {
        Self::new(200, "OK", body).header("Content-Type", "text/html; charset=utf-8")
    }

    fn json(body: &str) -> Self {
        Self::new(200, "OK", body).header("Content-Type", "application/json")
    }

    fn text(body: &str) -> Self {
        Self::new(200, "OK", body).header("Content-Type", "text/plain; charset=utf-8")
    }

    fn redirect(location: &str) -> Self {
        Self::new(302, "Found", "").header("Location", location)
    }

    fn not_found() -> Self {
        Self::new(404, "Not Found", "not found")
    }

    fn serialize(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            self.reason,
            self.body.len()
        );
        for (name, value) in &self.headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        response.push_str(&self.body);
        response.into_bytes()
    }
}

struct TestServer {
    address: std::net::SocketAddr,
}

impl TestServer {
    async fn spawn(handler: Arc<dyn Fn(TestRequest) -> TestResponse + Send + Sync>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    let request = read_request(&mut stream).await;
                    let response = handler(request);
                    stream.write_all(&response.serialize()).await.unwrap();
                });
            }
        });
        Self { address }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> TestRequest {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0, "client closed before headers");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap().to_owned();
    let path = request_parts.next().unwrap().to_owned();
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0, "client closed before body");
        bytes.extend_from_slice(&buffer[..count]);
    }
    let body =
        String::from_utf8_lossy(&bytes[header_end..header_end + content_length]).into_owned();
    TestRequest {
        method,
        path,
        headers,
        body,
    }
}
