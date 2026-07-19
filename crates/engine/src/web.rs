//! Fetch, CORS, cookie, and WebSocket processing for one browser-engine instance.

use std::collections::{BTreeSet, HashMap, VecDeque};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http::{
    HeaderMap, HeaderName, HeaderValue, Method,
    header::{
        ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
        ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS,
        ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, CONTENT_TYPE, ORIGIN,
        SEC_WEBSOCKET_PROTOCOL, SET_COOKIE,
    },
};
use meow_net::{CancellationToken, CredentialsMode, Loader, Request, RequestContext};
use meow_url_policy::{BrowserUrl, Origin as BrowserOrigin};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        protocol::{CloseFrame, frame::coding::CloseCode},
    },
};

use crate::{
    DocumentRuntime, FetchCompletion, FetchResponseInit, FetchTask, ScriptError, WebSocketCommand,
    WebSocketEvent,
};

const MAX_FETCHES_PER_PUMP: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebTaskReport {
    pub fetches_completed: usize,
    pub websocket_events: usize,
    pub pending_websockets: usize,
    pub budget_exhausted: bool,
    pub errors: Vec<ScriptError>,
}

#[derive(Debug)]
pub struct WebPlatform {
    loader: Loader,
    sockets: HashMap<u64, mpsc::UnboundedSender<WebSocketOutgoing>>,
    websocket_event_sender: mpsc::UnboundedSender<(u64, WebSocketEvent)>,
    websocket_event_receiver: mpsc::UnboundedReceiver<(u64, WebSocketEvent)>,
}

impl WebPlatform {
    #[must_use]
    pub fn new(loader: Loader) -> Self {
        let (websocket_event_sender, websocket_event_receiver) = mpsc::unbounded_channel();
        Self {
            loader,
            sockets: HashMap::new(),
            websocket_event_sender,
            websocket_event_receiver,
        }
    }

    pub async fn pump(&mut self, runtime: &mut DocumentRuntime) -> WebTaskReport {
        let mut report = WebTaskReport::default();
        self.drain_websocket_events(runtime, &mut report);
        self.process_websocket_commands(runtime.take_websocket_commands());

        let aborted = runtime.take_aborted_signals();
        let mut fetches = VecDeque::from(runtime.take_fetch_tasks());
        while report.fetches_completed < MAX_FETCHES_PER_PUMP {
            let Some(task) = fetches.pop_front() else {
                let newly_queued = runtime.take_fetch_tasks();
                if newly_queued.is_empty() {
                    break;
                }
                fetches.extend(newly_queued);
                continue;
            };
            let completion = if task
                .signal_id
                .is_some_and(|signal| aborted.contains(&signal))
            {
                aborted_completion()
            } else {
                self.perform_fetch(&task).await
            };
            if let Err(error) = runtime.complete_fetch(task.id, &completion) {
                report.errors.push(error);
            }
            report.fetches_completed += 1;
            self.process_websocket_commands(runtime.take_websocket_commands());
            fetches.extend(runtime.take_fetch_tasks());
        }
        report.budget_exhausted = !fetches.is_empty();
        runtime.requeue_fetch_tasks(fetches.into_iter().collect());
        self.drain_websocket_events(runtime, &mut report);
        self.process_websocket_commands(runtime.take_websocket_commands());
        report.pending_websockets = self.sockets.len();
        report
    }

    #[must_use]
    pub fn has_pending_work(&self) -> bool {
        !self.sockets.is_empty() || !self.websocket_event_receiver.is_empty()
    }

    pub fn document_committed(&mut self) {
        for sender in self.sockets.values() {
            let _ = sender.send(WebSocketOutgoing::Close(
                1001,
                "document navigated".to_owned(),
            ));
        }
        self.sockets.clear();
        while self.websocket_event_receiver.try_recv().is_ok() {}
    }

    async fn perform_fetch(&mut self, task: &FetchTask) -> FetchCompletion {
        match self.perform_fetch_inner(task).await {
            Ok(completion) => completion,
            Err(error) => failed_completion(&error),
        }
    }

    async fn perform_fetch_inner(&mut self, task: &FetchTask) -> Result<FetchCompletion, String> {
        if !task.url.is_http_family() {
            return Err(format!("unsupported fetch scheme {}", task.url.scheme()));
        }
        let method = Method::from_bytes(task.method.as_bytes())
            .map_err(|_| format!("invalid fetch method {}", task.method))?;
        let mut headers = request_headers(&task.headers)?;
        let cross_origin = task.document_origin != task.url.origin();
        match task.mode.as_str() {
            "same-origin" if cross_origin => {
                return Err("same-origin fetch blocked a cross-origin URL".to_owned());
            }
            "cors" | "same-origin" | "no-cors" => {}
            mode => return Err(format!("unsupported fetch mode {mode}")),
        }
        if task.mode == "no-cors" && (!is_simple_method(&method) || !are_simple_headers(&headers)) {
            return Err("no-cors fetch requires a simple method and headers".to_owned());
        }
        let credentials = match task.credentials.as_str() {
            "omit" => CredentialsMode::Omit,
            "same-origin" => CredentialsMode::SameOrigin,
            "include" => CredentialsMode::Include,
            value => return Err(format!("unsupported credentials mode {value}")),
        };
        if cross_origin && task.mode == "cors" {
            headers.insert(
                ORIGIN,
                HeaderValue::from_str(&task.document_origin.to_string())
                    .map_err(|error| error.to_string())?,
            );
            if !is_simple_method(&method) || !are_simple_headers(&headers) {
                self.preflight(task, &method, &headers, credentials).await?;
            }
        }
        let request = Request {
            method: method.clone(),
            url: task.url.clone(),
            headers,
            body: Bytes::copy_from_slice(&task.body),
        };
        let response = self
            .loader
            .load_with_context(
                request,
                RequestContext::document(task.document_url.clone(), credentials),
                &CancellationToken::new(),
            )
            .await
            .map_err(|error| error.to_string())?;
        if task.redirect == "error" && !response.metadata.redirects.is_empty() {
            return Err("fetch redirect blocked by redirect=error".to_owned());
        }
        if !matches!(task.redirect.as_str(), "follow" | "error") {
            return Err(format!("unsupported redirect mode {}", task.redirect));
        }
        let final_cross_origin = task.document_origin != response.metadata.final_url.origin();
        if task.mode == "same-origin" && final_cross_origin {
            return Err("same-origin fetch redirected cross-origin".to_owned());
        }
        if task.mode == "cors" && final_cross_origin {
            validate_cors_response(&response.headers, &task.document_origin, credentials)?;
        }
        if task.mode == "no-cors" && final_cross_origin {
            return Ok(FetchCompletion {
                ok: true,
                name: None,
                error: None,
                body: Some(String::new()),
                response: Some(FetchResponseInit {
                    status: 0,
                    status_text: String::new(),
                    headers: Vec::new(),
                    url: String::new(),
                    redirected: false,
                    response_type: "opaque".to_owned(),
                }),
            });
        }
        let exposed_headers =
            response_headers(&response.headers, final_cross_origin && task.mode == "cors");
        Ok(FetchCompletion {
            ok: true,
            name: None,
            error: None,
            body: Some(String::from_utf8_lossy(&response.body).into_owned()),
            response: Some(FetchResponseInit {
                status: response.status.as_u16(),
                status_text: response
                    .status
                    .canonical_reason()
                    .unwrap_or_default()
                    .to_owned(),
                headers: exposed_headers,
                url: response.metadata.final_url.to_string(),
                redirected: !response.metadata.redirects.is_empty(),
                response_type: if final_cross_origin {
                    "cors".to_owned()
                } else {
                    "basic".to_owned()
                },
            }),
        })
    }

    async fn preflight(
        &self,
        task: &FetchTask,
        method: &Method,
        headers: &HeaderMap,
        credentials: CredentialsMode,
    ) -> Result<(), String> {
        let mut preflight_headers = HeaderMap::new();
        preflight_headers.insert(
            ORIGIN,
            HeaderValue::from_str(&task.document_origin.to_string())
                .map_err(|error| error.to_string())?,
        );
        preflight_headers.insert(
            ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_str(method.as_str()).map_err(|error| error.to_string())?,
        );
        let non_simple = non_simple_header_names(headers);
        if !non_simple.is_empty() {
            preflight_headers.insert(
                ACCESS_CONTROL_REQUEST_HEADERS,
                HeaderValue::from_str(&non_simple.join(", ")).map_err(|error| error.to_string())?,
            );
        }
        let response = self
            .loader
            .load_with_context(
                Request {
                    method: Method::OPTIONS,
                    url: task.url.clone(),
                    headers: preflight_headers,
                    body: Bytes::new(),
                },
                RequestContext::document(task.document_url.clone(), CredentialsMode::Omit),
                &CancellationToken::new(),
            )
            .await
            .map_err(|error| error.to_string())?;
        if !response.status.is_success() {
            return Err(format!(
                "CORS preflight failed with status {}",
                response.status
            ));
        }
        validate_cors_response(&response.headers, &task.document_origin, credentials)?;
        let allowed_methods = comma_values(&response.headers, ACCESS_CONTROL_ALLOW_METHODS);
        if !allowed_methods
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(method.as_str()))
        {
            return Err(format!("CORS preflight did not allow method {method}"));
        }
        let allowed_headers = comma_values(&response.headers, ACCESS_CONTROL_ALLOW_HEADERS);
        for header in non_simple {
            if !allowed_headers
                .iter()
                .any(|allowed| allowed == "*" || allowed.eq_ignore_ascii_case(&header))
            {
                return Err(format!("CORS preflight did not allow header {header}"));
            }
        }
        Ok(())
    }

    fn process_websocket_commands(&mut self, commands: Vec<WebSocketCommand>) {
        for command in commands {
            match command {
                WebSocketCommand::Connect {
                    id,
                    url,
                    origin,
                    protocols,
                } => self.connect_websocket(id, url, origin, protocols),
                WebSocketCommand::SendText { id, data } => {
                    if let Some(sender) = self.sockets.get(&id) {
                        let _ = sender.send(WebSocketOutgoing::Text(data));
                    }
                }
                WebSocketCommand::SendBinary { id, data } => {
                    if let Some(sender) = self.sockets.get(&id) {
                        let _ = sender.send(WebSocketOutgoing::Binary(data));
                    }
                }
                WebSocketCommand::Close { id, code, reason } => {
                    if let Some(sender) = self.sockets.get(&id) {
                        let _ = sender.send(WebSocketOutgoing::Close(code, reason));
                    }
                }
            }
        }
    }

    fn connect_websocket(
        &mut self,
        id: u64,
        url: BrowserUrl,
        origin: BrowserOrigin,
        protocols: Vec<String>,
    ) {
        if !matches!(url.scheme(), "ws" | "wss") {
            let _ = self.websocket_event_sender.send((
                id,
                WebSocketEvent::Error {
                    message: "WebSocket URL must use ws or wss".to_owned(),
                },
            ));
            let _ = self.websocket_event_sender.send((
                id,
                WebSocketEvent::Close {
                    code: 1006,
                    reason: String::new(),
                    was_clean: false,
                },
            ));
            return;
        }
        if self.loader.is_brokered() {
            let _ = self.websocket_event_sender.send((
                id,
                WebSocketEvent::Error {
                    message:
                        "WebSocket broker support is not enabled for the isolated content process"
                            .to_owned(),
                },
            ));
            let _ = self.websocket_event_sender.send((
                id,
                WebSocketEvent::Close {
                    code: 1006,
                    reason: "network permission denied".to_owned(),
                    was_clean: false,
                },
            ));
            return;
        }
        let (outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
        self.sockets.insert(id, outgoing_sender);
        let event_sender = self.websocket_event_sender.clone();
        tokio::spawn(async move {
            websocket_task(id, url, origin, protocols, outgoing_receiver, event_sender).await;
        });
    }

    fn drain_websocket_events(
        &mut self,
        runtime: &mut DocumentRuntime,
        report: &mut WebTaskReport,
    ) {
        while let Ok((id, event)) = self.websocket_event_receiver.try_recv() {
            if matches!(event, WebSocketEvent::Close { .. }) {
                self.sockets.remove(&id);
            }
            if let Err(error) = runtime.dispatch_websocket_event(id, &event) {
                report.errors.push(error);
            }
            report.websocket_events += 1;
        }
    }
}

impl Default for WebPlatform {
    fn default() -> Self {
        Self::new(Loader::default())
    }
}

fn request_headers(entries: &[(String, String)]) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    for (name, value) in entries {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| error.to_string())?;
        if is_forbidden_request_header(&name) {
            return Err(format!("forbidden request header {name}"));
        }
        let value = HeaderValue::from_str(value).map_err(|error| error.to_string())?;
        headers.append(name, value);
    }
    Ok(headers)
}

fn is_forbidden_request_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "accept-charset"
            | "accept-encoding"
            | "access-control-request-headers"
            | "access-control-request-method"
            | "connection"
            | "content-length"
            | "cookie"
            | "cookie2"
            | "date"
            | "dnt"
            | "expect"
            | "host"
            | "keep-alive"
            | "origin"
            | "referer"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "via"
    ) || name.as_str().starts_with("proxy-")
        || name.as_str().starts_with("sec-")
}

fn is_simple_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::POST)
}

fn are_simple_headers(headers: &HeaderMap) -> bool {
    headers.iter().all(|(name, value)| {
        matches!(
            name.as_str(),
            "accept" | "accept-language" | "content-language" | "content-type" | "origin"
        ) && (name != CONTENT_TYPE || is_simple_content_type(value))
    })
}

fn is_simple_content_type(value: &HeaderValue) -> bool {
    value
        .to_str()
        .ok()
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|mime| {
            matches!(
                mime.to_ascii_lowercase().as_str(),
                "application/x-www-form-urlencoded" | "multipart/form-data" | "text/plain"
            )
        })
}

fn non_simple_header_names(headers: &HeaderMap) -> Vec<String> {
    let mut names = BTreeSet::new();
    for (name, value) in headers {
        let simple = matches!(
            name.as_str(),
            "accept" | "accept-language" | "content-language" | "origin"
        ) || (name == CONTENT_TYPE && is_simple_content_type(value));
        if !simple {
            names.insert(name.as_str().to_owned());
        }
    }
    names.into_iter().collect()
}

fn validate_cors_response(
    headers: &HeaderMap,
    origin: &BrowserOrigin,
    credentials: CredentialsMode,
) -> Result<(), String> {
    let expected = origin.to_string();
    let allowed = headers
        .get(ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "CORS response omitted Access-Control-Allow-Origin".to_owned())?;
    if allowed != expected && !(allowed == "*" && !matches!(credentials, CredentialsMode::Include))
    {
        return Err(format!("CORS origin {expected} was not allowed"));
    }
    if matches!(credentials, CredentialsMode::Include)
        && headers
            .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok())
            != Some("true")
    {
        return Err(
            "credentialed CORS response omitted Access-Control-Allow-Credentials: true".to_owned(),
        );
    }
    Ok(())
}

fn comma_values(headers: &HeaderMap, name: HeaderName) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn response_headers(headers: &HeaderMap, cors: bool) -> Vec<(String, String)> {
    let exposed = comma_values(headers, ACCESS_CONTROL_EXPOSE_HEADERS)
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    headers
        .iter()
        .filter(|(name, _)| *name != SET_COOKIE)
        .filter(|(name, _)| {
            !cors
                || matches!(
                    name.as_str(),
                    "cache-control"
                        | "content-language"
                        | "content-length"
                        | "content-type"
                        | "expires"
                        | "last-modified"
                        | "pragma"
                )
                || exposed.contains(name.as_str())
                || exposed.contains("*")
        })
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect()
}

fn failed_completion(error: &str) -> FetchCompletion {
    FetchCompletion {
        ok: false,
        name: None,
        error: Some(error.to_owned()),
        body: None,
        response: None,
    }
}

fn aborted_completion() -> FetchCompletion {
    FetchCompletion {
        ok: false,
        name: Some("AbortError".to_owned()),
        error: Some("The operation was aborted".to_owned()),
        body: None,
        response: None,
    }
}

enum WebSocketOutgoing {
    Text(String),
    Binary(Vec<u8>),
    Close(u16, String),
}

async fn websocket_task(
    id: u64,
    url: BrowserUrl,
    origin: BrowserOrigin,
    protocols: Vec<String>,
    mut outgoing: mpsc::UnboundedReceiver<WebSocketOutgoing>,
    events: mpsc::UnboundedSender<(u64, WebSocketEvent)>,
) {
    let result = async {
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|error| error.to_string())?;
        request.headers_mut().insert(
            ORIGIN,
            HeaderValue::from_str(&origin.to_string()).map_err(|error| error.to_string())?,
        );
        if !protocols.is_empty() {
            request.headers_mut().insert(
                SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_str(&protocols.join(", ")).map_err(|error| error.to_string())?,
            );
        }
        let (mut socket, response) = connect_async(request)
            .await
            .map_err(|error| error.to_string())?;
        let protocol = response
            .headers()
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let _ = events.send((id, WebSocketEvent::Open { protocol }));
        loop {
            tokio::select! {
                command = outgoing.recv() => {
                    let Some(command) = command else { break; };
                    match command {
                        WebSocketOutgoing::Text(data) => socket.send(Message::Text(data.into())).await,
                        WebSocketOutgoing::Binary(data) => socket.send(Message::Binary(data.into())).await,
                        WebSocketOutgoing::Close(code, reason) => socket.send(Message::Close(Some(CloseFrame {
                            code: CloseCode::from(code),
                            reason: reason.into(),
                        }))).await,
                    }.map_err(|error| error.to_string())?;
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Text(data))) => {
                            let _ = events.send((id, WebSocketEvent::Text {
                                data: data.to_string(),
                                origin: url.origin().to_string(),
                            }));
                        }
                        Some(Ok(Message::Binary(data))) => {
                            let _ = events.send((id, WebSocketEvent::Binary {
                                data: data.to_vec(),
                                origin: url.origin().to_string(),
                            }));
                        }
                        Some(Ok(Message::Close(frame))) => {
                            let (code, reason) = frame.map_or((1000, String::new()), |frame| {
                                (u16::from(frame.code), frame.reason.to_string())
                            });
                            let _ = events.send((id, WebSocketEvent::Close {
                                code,
                                reason,
                                was_clean: true,
                            }));
                            return Ok::<(), String>(());
                        }
                        Some(Ok(Message::Ping(data))) => socket.send(Message::Pong(data)).await.map_err(|error| error.to_string())?,
                        Some(Ok(Message::Pong(_) | Message::Frame(_))) => {}
                        Some(Err(error)) => return Err(error.to_string()),
                        None => {
                            let _ = events.send((id, WebSocketEvent::Close {
                                code: 1006,
                                reason: String::new(),
                                was_clean: false,
                            }));
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(message) = result {
        let _ = events.send((id, WebSocketEvent::Error { message }));
        let _ = events.send((
            id,
            WebSocketEvent::Close {
                code: 1006,
                reason: String::new(),
                was_clean: false,
            },
        ));
    }
}
