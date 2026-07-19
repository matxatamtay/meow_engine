//! Fail-closed browser-host facade used by the W3 V8 production configuration.

use std::{collections::BTreeSet, error::Error, fmt};

use meow_html::{Document, DomMutation, NodeId};
use meow_js_runtime::BackendKind;
use meow_url_policy::Origin;
use serde::Serialize;

use crate::{BrowserUrl, storage::StorageBindings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptLimits {
    pub max_source_bytes: usize,
    pub loop_iterations: u64,
    pub recursion_depth: usize,
    pub stack_size: usize,
    pub backtrace_frames: usize,
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 512 * 1024,
            loop_iterations: 1_000_000,
            recursion_depth: 128,
            stack_size: 4_096,
            backtrace_frames: 16,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptSource {
    pub code: String,
    pub url: BrowserUrl,
    pub node: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventDispatchResult {
    pub default_prevented: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleMessage {
    pub level: ConsoleLevel,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct FetchTask {
    pub id: u64,
    pub url: BrowserUrl,
    pub document_url: BrowserUrl,
    pub document_origin: Origin,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub mode: String,
    pub credentials: String,
    pub redirect: String,
    pub signal_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FetchCompletion {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<FetchResponseInit>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchResponseInit {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub url: String,
    pub redirected: bool,
    #[serde(rename = "type")]
    pub response_type: String,
}

#[derive(Clone, Debug)]
pub enum WebSocketCommand {
    Connect {
        id: u64,
        url: BrowserUrl,
        origin: Origin,
        protocols: Vec<String>,
    },
    SendText {
        id: u64,
        data: String,
    },
    SendBinary {
        id: u64,
        data: Vec<u8>,
    },
    Close {
        id: u64,
        code: u16,
        reason: String,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WebSocketEvent {
    Open {
        protocol: String,
    },
    Text {
        data: String,
        origin: String,
    },
    Binary {
        data: Vec<u8>,
        origin: String,
    },
    Error {
        message: String,
    },
    Close {
        code: u16,
        reason: String,
        #[serde(rename = "wasClean")]
        was_clean: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptErrorKind {
    Syntax,
    Exception,
    ResourceLimit,
    Host,
    Load,
    BackendUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub kind: ScriptErrorKind,
    pub message: String,
    pub source_url: BrowserUrl,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.source_url, self.message)
    }
}

impl Error for ScriptError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimerRunReport {
    pub advanced_ms: u64,
    pub tasks_run: usize,
    pub budget_exhausted: bool,
    pub pending_timers: usize,
    pub errors: Vec<ScriptError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptExecutionPhase {
    ParserBlocking,
    Deferred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptExecution {
    pub node: Option<NodeId>,
    pub source_url: BrowserUrl,
    pub phase: ScriptExecutionPhase,
    pub error: Option<ScriptError>,
}

impl ScriptExecution {
    #[must_use]
    pub const fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

pub trait JsRuntime {
    fn execute(&mut self, source: &ScriptSource) -> Result<ScriptValue, ScriptError>;
    fn take_mutations(&mut self) -> Vec<DomMutation>;
}

pub struct V8Runtime {
    location: BrowserUrl,
    limits: ScriptLimits,
    mutations: Vec<DomMutation>,
}

impl fmt::Debug for V8Runtime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("V8Runtime")
            .field("backend", &BackendKind::V8)
            .field("location", &self.location)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl V8Runtime {
    pub fn new(
        document: Document,
        location: BrowserUrl,
        limits: ScriptLimits,
    ) -> Result<Self, ScriptError> {
        let _ = document;
        Ok(Self {
            location,
            limits,
            mutations: Vec::new(),
        })
    }

    pub(crate) fn new_with_storage(
        document: Document,
        location: BrowserUrl,
        limits: ScriptLimits,
        _storage: StorageBindings,
    ) -> Result<Self, ScriptError> {
        Self::new(document, location, limits)
    }

    pub fn dispatch_event(
        &mut self,
        _target: NodeId,
        _event_type: &str,
        _bubbles: bool,
        _cancelable: bool,
    ) -> Result<EventDispatchResult, ScriptError> {
        Ok(EventDispatchResult::default())
    }

    pub fn advance_time(&mut self, advance_ms: u64, _max_tasks: usize) -> TimerRunReport {
        TimerRunReport {
            advanced_ms: advance_ms,
            ..TimerRunReport::default()
        }
    }

    #[must_use]
    pub const fn has_pending_timers(&self) -> bool {
        false
    }

    pub fn take_console_messages(&mut self) -> Vec<ConsoleMessage> {
        Vec::new()
    }

    #[must_use]
    pub const fn has_pending_web_tasks(&self) -> bool {
        false
    }

    pub fn take_fetch_tasks(&mut self) -> Vec<FetchTask> {
        Vec::new()
    }

    pub fn requeue_fetch_tasks(&mut self, _tasks: Vec<FetchTask>) {}

    pub fn take_aborted_signals(&mut self) -> BTreeSet<u64> {
        BTreeSet::new()
    }

    pub fn complete_fetch(
        &mut self,
        _id: u64,
        _completion: &FetchCompletion,
    ) -> Result<(), ScriptError> {
        Ok(())
    }

    pub fn take_websocket_commands(&mut self) -> Vec<WebSocketCommand> {
        Vec::new()
    }

    pub fn dispatch_websocket_event(
        &mut self,
        _id: u64,
        _event: &WebSocketEvent,
    ) -> Result<(), ScriptError> {
        Ok(())
    }
}

impl JsRuntime for V8Runtime {
    fn execute(&mut self, source: &ScriptSource) -> Result<ScriptValue, ScriptError> {
        if source.code.len() > self.limits.max_source_bytes {
            return Err(ScriptError {
                kind: ScriptErrorKind::ResourceLimit,
                message: format!(
                    "classic script exceeded {} byte source limit",
                    self.limits.max_source_bytes
                ),
                source_url: source.url.clone(),
            });
        }
        if source.code.trim().is_empty() {
            return Ok(ScriptValue::Undefined);
        }
        Err(ScriptError {
            kind: ScriptErrorKind::BackendUnavailable,
            message: "V8 browser host is selected but the isolate is not linked until Y2-W4"
                .to_owned(),
            source_url: source.url.clone(),
        })
    }

    fn take_mutations(&mut self) -> Vec<DomMutation> {
        std::mem::take(&mut self.mutations)
    }
}

pub type DocumentRuntime = V8Runtime;
