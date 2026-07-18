//! Top-level frame and navigation orchestration for MeowEngine.

use std::{error::Error, fmt};

use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};
use meow_display_list::{DisplayList, DisplayListError, Viewport, reference_scene};
use meow_html::{Document, parse_bytes, parse_utf8};
use meow_net::{Loader, NetError, Request, ResponseMetadata};
use meow_url_policy::UrlPolicyError;

pub use meow_net::CancellationToken;
pub use meow_url_policy::BrowserUrl;

/// Human-readable engine name used by first-party applications.
pub const ENGINE_NAME: &str = "MeowEngine";

/// Engine coordinator that produces resolved, backend-neutral frames.
#[derive(Debug, Default)]
pub struct Engine;

impl Engine {
    /// Creates an engine coordinator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the display list for one viewport.
    pub fn build_display_list(
        &mut self,
        viewport: Viewport,
    ) -> Result<DisplayList, DisplayListError> {
        reference_scene(viewport)
    }
}

/// Returns the workspace package version embedded at compile time.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Source that selected the committed document encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharsetSource {
    /// The synthetic `about:blank` document always uses UTF-8.
    AboutBlank,
    /// A Unicode byte-order mark.
    Bom,
    /// An HTTP `Content-Type` charset parameter.
    HttpHeader,
    /// A `<meta charset>` or equivalent declaration in the first 1024 bytes.
    Meta,
    /// The HTML fallback encoding.
    Default,
}

/// One committed session-history entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Monotonic entry sequence within this navigator.
    pub sequence: u64,
    /// Committed document URL.
    pub url: BrowserUrl,
}

/// Fully parsed and committed top-level document state.
#[derive(Clone, Debug)]
pub struct DocumentState {
    /// URL that produced the document.
    pub url: BrowserUrl,
    /// URL used for relative-reference resolution after applying `<base>`.
    pub base_url: BrowserUrl,
    /// Parsed DOM document.
    pub document: Document,
    /// Canonical Encoding Standard name.
    pub encoding: &'static str,
    /// Why the encoding was selected.
    pub charset_source: CharsetSource,
    /// HTTP response metadata. Synthetic documents have none.
    pub response: Option<ResponseMetadata>,
    /// Index of this document in the current history list.
    pub history_index: usize,
}

/// Top-level navigation lifecycle owner.
#[derive(Debug)]
pub struct Navigator {
    loader: Loader,
    current: DocumentState,
    history: Vec<HistoryEntry>,
    next_sequence: u64,
}

impl Navigator {
    /// Creates a navigator with a committed `about:blank` document and history entry.
    #[must_use]
    pub fn new(loader: Loader) -> Self {
        let url = BrowserUrl::about_blank();
        let current = DocumentState {
            url: url.clone(),
            base_url: url.clone(),
            document: parse_utf8(b"").document,
            encoding: UTF_8.name(),
            charset_source: CharsetSource::AboutBlank,
            response: None,
            history_index: 0,
        };
        Self {
            loader,
            current,
            history: vec![HistoryEntry { sequence: 0, url }],
            next_sequence: 1,
        }
    }

    /// Returns the current committed document.
    #[must_use]
    pub const fn current(&self) -> &DocumentState {
        &self.current
    }

    /// Returns committed history entries.
    #[must_use]
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// Resolves, loads, parses, and atomically commits a top-level navigation.
    pub async fn navigate(
        &mut self,
        input: &str,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        let target = BrowserUrl::parse(input)
            .or_else(|_| self.current.base_url.resolve(input))
            .map_err(NavigationError::Url)?;
        self.navigate_to(target, cancellation).await
    }

    /// Loads a canonical target URL and atomically commits it.
    pub async fn navigate_to(
        &mut self,
        target: BrowserUrl,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        tracing::debug!(url = %target, "starting top-level navigation");
        let pending = if target.as_str() == "about:blank" {
            DocumentState {
                url: target.clone(),
                base_url: target.clone(),
                document: parse_utf8(b"").document,
                encoding: UTF_8.name(),
                charset_source: CharsetSource::AboutBlank,
                response: None,
                history_index: self.history.len(),
            }
        } else {
            let response = self
                .loader
                .load(Request::get(target), cancellation)
                .await
                .map_err(NavigationError::Network)?;
            let (encoding, charset_source) =
                sniff_encoding(&response.body, response.metadata.content_type.as_deref());
            let parsed = parse_bytes(&response.body, encoding);
            let final_url = response.metadata.final_url.clone();
            let base_url = parsed
                .document
                .first_base_href()
                .and_then(|reference| final_url.resolve(&reference).ok())
                .unwrap_or_else(|| final_url.clone());

            DocumentState {
                url: final_url,
                base_url,
                document: parsed.document,
                encoding: encoding.name(),
                charset_source,
                response: Some(response.metadata),
                history_index: self.history.len(),
            }
        };

        self.history.push(HistoryEntry {
            sequence: self.next_sequence,
            url: pending.url.clone(),
        });
        self.next_sequence += 1;
        self.current = pending;
        tracing::debug!(url = %self.current.url, history_index = self.current.history_index, "committed top-level navigation");
        Ok(&self.current)
    }
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new(Loader::default())
    }
}

fn sniff_encoding(bytes: &[u8], content_type: Option<&str>) -> (&'static Encoding, CharsetSource) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (UTF_8, CharsetSource::Bom);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return (
            Encoding::for_label(b"utf-16le").expect("encoding_rs provides UTF-16LE"),
            CharsetSource::Bom,
        );
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return (
            Encoding::for_label(b"utf-16be").expect("encoding_rs provides UTF-16BE"),
            CharsetSource::Bom,
        );
    }
    if let Some(label) = content_type.and_then(charset_parameter)
        && let Some(encoding) = Encoding::for_label(label.as_bytes())
    {
        return (encoding, CharsetSource::HttpHeader);
    }
    if let Some(label) = sniff_meta_charset(bytes)
        && let Some(encoding) = Encoding::for_label(label.as_bytes())
    {
        return (encoding, CharsetSource::Meta);
    }
    (WINDOWS_1252, CharsetSource::Default)
}

fn charset_parameter(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("charset") {
            return None;
        }
        let value = value.trim().trim_matches(['\'', '"']);
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    let sample = bytes
        .iter()
        .take(1024)
        .map(|byte| {
            if byte.is_ascii() {
                byte.to_ascii_lowercase() as char
            } else {
                ' '
            }
        })
        .collect::<String>();
    let mut search_from = 0;
    while let Some(offset) = sample[search_from..].find("charset") {
        let mut cursor = search_from + offset + "charset".len();
        let tail = sample.as_bytes();
        while tail.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if tail.get(cursor) != Some(&b'=') {
            search_from = cursor;
            continue;
        }
        cursor += 1;
        while tail.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let quote = tail
            .get(cursor)
            .copied()
            .filter(|byte| matches!(byte, b'\'' | b'"'));
        if quote.is_some() {
            cursor += 1;
        }
        let start = cursor;
        while let Some(byte) = tail.get(cursor) {
            let terminates = quote.map_or_else(
                || byte.is_ascii_whitespace() || matches!(byte, b';' | b'/' | b'>'),
                |quote| *byte == quote,
            );
            if terminates {
                break;
            }
            cursor += 1;
        }
        if cursor > start {
            return Some(sample[start..cursor].to_owned());
        }
        search_from = cursor.saturating_add(1);
    }
    None
}

/// Navigation failure before document commit.
#[derive(Debug)]
pub enum NavigationError {
    /// Target URL or relative reference was invalid.
    Url(UrlPolicyError),
    /// Network loading failed.
    Network(NetError),
}

impl fmt::Display for NavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(error) => error.fmt(formatter),
            Self::Network(error) => error.fmt(formatter),
        }
    }
}

impl Error for NavigationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Url(error) => Some(error),
            Self::Network(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        task::JoinHandle,
    };

    use super::*;

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
        let (status, response_body) = if path == "/page" {
            ("200 OK", body)
        } else {
            ("404 Not Found", Vec::new())
        };
        let headers = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=windows-1252\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        );
        let _ = stream.write_all(headers.as_bytes()).await;
        let _ = stream.write_all(&response_body).await;
        let _ = stream.shutdown().await;
    }
}
