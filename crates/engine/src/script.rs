//! Boa-backed JavaScript runtime and the W25-W27 DOM host bridge.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    io::Cursor,
    path::Path,
    rc::Rc,
};

use boa_engine::{
    Context, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source, vm::RuntimeLimits,
};
use meow_html::{Document, DomMutation, NodeHandle, NodeId};
use meow_url_policy::Origin;
use serde::{Deserialize, Serialize};

use crate::{
    BrowserUrl, parse_selector_list,
    storage::{StorageArea, StorageBindings, StorageManager},
};

const BINDINGS_BOOTSTRAP: &str = include_str!("../../js-runtime/src/browser_bootstrap.js");

thread_local! {
    static ACTIVE_HOST: RefCell<Option<Rc<RefCell<HostState>>>> = const { RefCell::new(None) };
}

/// Resource limits applied to one Boa realm.
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

/// One classic script source and its diagnostic identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptSource {
    pub code: String,
    pub url: BrowserUrl,
    pub node: Option<NodeId>,
}

/// Small, backend-neutral representation of an evaluation result.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object,
}

/// Result of dispatching one DOM event through capture, target, and bubble phases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventDispatchResult {
    pub default_prevented: bool,
}

/// Console severity retained by the document runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
}

/// One deterministic console entry emitted by page script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleMessage {
    pub level: ConsoleLevel,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFetchRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    mode: String,
    credentials: String,
    redirect: String,
    signal_id: Option<u64>,
}

/// Buffered fetch request emitted by the JavaScript realm for embedder processing.
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

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum RawWebSocketCommand {
    Connect { url: String, protocols: Vec<String> },
    SendText { id: u64, data: String },
    SendBinary { id: u64, data: Vec<u8> },
    Close { id: u64, code: u16, reason: String },
}

/// WebSocket command emitted by a JavaScript `WebSocket` object.
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

/// One bounded timer-pump result. Promise and `queueMicrotask` jobs are drained after every task.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimerRunReport {
    pub advanced_ms: u64,
    pub tasks_run: usize,
    pub budget_exhausted: bool,
    pub pending_timers: usize,
    pub errors: Vec<ScriptError>,
}

/// Scheduling phase used by the deterministic classic-script task queues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptExecutionPhase {
    ParserBlocking,
    Deferred,
}

/// One completed or failed script task retained for diagnostics and tests.
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

/// Backend boundary for one persistent JavaScript realm.
pub trait JsRuntime {
    fn execute(&mut self, source: &ScriptSource) -> Result<ScriptValue, ScriptError>;
    fn take_mutations(&mut self) -> Vec<DomMutation>;
}

/// Boa context plus one host realm bound to a stable DOM document.
pub struct BoaRuntime {
    context: Context,
    host: Rc<RefCell<HostState>>,
    limits: ScriptLimits,
}

pub type DocumentRuntime = BoaRuntime;

impl fmt::Debug for BoaRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoaRuntime")
            .field("location", &self.host.borrow().location)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl BoaRuntime {
    pub fn new(
        document: Document,
        location: BrowserUrl,
        limits: ScriptLimits,
    ) -> Result<Self, ScriptError> {
        let mut storage = StorageManager::ephemeral();
        let bindings = storage.bindings_for(&location.origin());
        Self::new_with_storage(document, location, limits, bindings)
    }

    pub(crate) fn new_with_storage(
        document: Document,
        location: BrowserUrl,
        limits: ScriptLimits,
        storage: StorageBindings,
    ) -> Result<Self, ScriptError> {
        let host = Rc::new(RefCell::new(HostState {
            document,
            location: location.clone(),
            storage,
            mutations: Vec::new(),
            console: Vec::new(),
            clock_ms: 0,
            next_timer_id: 1,
            next_timer_sequence: 1,
            timers: BTreeMap::new(),
            next_fetch_id: 1,
            fetch_tasks: Vec::new(),
            aborted_signals: BTreeSet::new(),
            next_websocket_id: 1,
            websocket_commands: Vec::new(),
        }));
        let mut context = Context::default();
        let mut runtime_limits = RuntimeLimits::default();
        runtime_limits.set_loop_iteration_limit(limits.loop_iterations);
        runtime_limits.set_recursion_limit(limits.recursion_depth);
        runtime_limits.set_stack_size_limit(limits.stack_size);
        runtime_limits.set_backtrace_limit(limits.backtrace_frames);
        context.set_runtime_limits(runtime_limits);
        register_host_functions(&mut context).map_err(|error| ScriptError {
            kind: ScriptErrorKind::Host,
            message: error.to_string(),
            source_url: location.clone(),
        })?;

        let mut runtime = Self {
            context,
            host,
            limits,
        };
        runtime
            .evaluate_raw(BINDINGS_BOOTSTRAP)
            .map_err(|error| map_boa_error(error, location))?;
        Ok(runtime)
    }

    /// Dispatches one trusted DOM event at an element target.
    pub fn dispatch_event(
        &mut self,
        target: NodeId,
        event_type: &str,
        bubbles: bool,
        cancelable: bool,
    ) -> Result<EventDispatchResult, ScriptError> {
        let location = self.host.borrow().location.clone();
        let source = format!(
            "__meow_dispatch_trusted({}, {}, {bubbles}, {cancelable})",
            js_string_literal(&node_id_string(target)),
            js_string_literal(event_type),
        );
        let accepted = self
            .evaluate_raw(&source)
            .map_err(|error| map_boa_error(error, location))?
            .as_boolean()
            .unwrap_or(true);
        Ok(EventDispatchResult {
            default_prevented: !accepted,
        })
    }

    /// Advances the deterministic document clock and runs at most `max_tasks` due timers.
    pub fn advance_time(&mut self, advance_ms: u64, max_tasks: usize) -> TimerRunReport {
        {
            let mut host = self.host.borrow_mut();
            host.clock_ms = host.clock_ms.saturating_add(advance_ms);
        }
        let mut report = TimerRunReport {
            advanced_ms: advance_ms,
            ..TimerRunReport::default()
        };
        while report.tasks_run < max_tasks {
            let next = {
                let host = self.host.borrow();
                host.timers
                    .iter()
                    .filter(|(_, timer)| timer.due_ms <= host.clock_ms)
                    .min_by_key(|(_, timer)| (timer.due_ms, timer.sequence))
                    .map(|(id, timer)| (*id, timer.clone()))
            };
            let Some((id, timer)) = next else {
                break;
            };
            {
                let mut host = self.host.borrow_mut();
                if let Some(interval_ms) = timer.interval_ms {
                    let sequence = host.next_timer_sequence;
                    host.next_timer_sequence = host.next_timer_sequence.saturating_add(1);
                    if let Some(active) = host.timers.get_mut(&id) {
                        active.due_ms = timer.due_ms.saturating_add(interval_ms);
                        active.sequence = sequence;
                    }
                } else {
                    host.timers.remove(&id);
                }
            }
            let location = self.host.borrow().location.clone();
            if let Err(error) = self.evaluate_raw(&format!("__meow_fire_timer({id})")) {
                report.errors.push(map_boa_error(error, location));
            }
            report.tasks_run += 1;
        }
        let host = self.host.borrow();
        report.pending_timers = host.timers.len();
        report.budget_exhausted = report.tasks_run == max_tasks
            && host
                .timers
                .values()
                .any(|timer| timer.due_ms <= host.clock_ms);
        report
    }

    #[must_use]
    pub fn has_pending_timers(&self) -> bool {
        !self.host.borrow().timers.is_empty()
    }

    pub fn take_console_messages(&mut self) -> Vec<ConsoleMessage> {
        std::mem::take(&mut self.host.borrow_mut().console)
    }

    #[must_use]
    pub fn has_pending_web_tasks(&self) -> bool {
        let host = self.host.borrow();
        !host.fetch_tasks.is_empty()
            || !host.aborted_signals.is_empty()
            || !host.websocket_commands.is_empty()
    }

    pub fn take_fetch_tasks(&mut self) -> Vec<FetchTask> {
        std::mem::take(&mut self.host.borrow_mut().fetch_tasks)
    }

    pub fn requeue_fetch_tasks(&mut self, mut tasks: Vec<FetchTask>) {
        if tasks.is_empty() {
            return;
        }
        let mut host = self.host.borrow_mut();
        tasks.append(&mut host.fetch_tasks);
        host.fetch_tasks = tasks;
    }

    pub fn take_aborted_signals(&mut self) -> BTreeSet<u64> {
        std::mem::take(&mut self.host.borrow_mut().aborted_signals)
    }

    pub fn complete_fetch(
        &mut self,
        id: u64,
        completion: &FetchCompletion,
    ) -> Result<(), ScriptError> {
        let payload = serde_json::to_string(completion).map_err(|error| ScriptError {
            kind: ScriptErrorKind::Host,
            message: error.to_string(),
            source_url: self.host.borrow().location.clone(),
        })?;
        let location = self.host.borrow().location.clone();
        self.evaluate_raw(&format!("__meow_complete_fetch({id}, {payload})"))
            .map(|_| ())
            .map_err(|error| map_boa_error(error, location))
    }

    pub fn take_websocket_commands(&mut self) -> Vec<WebSocketCommand> {
        std::mem::take(&mut self.host.borrow_mut().websocket_commands)
    }

    pub fn dispatch_websocket_event(
        &mut self,
        id: u64,
        event: &WebSocketEvent,
    ) -> Result<(), ScriptError> {
        let payload = serde_json::to_string(event).map_err(|error| ScriptError {
            kind: ScriptErrorKind::Host,
            message: error.to_string(),
            source_url: self.host.borrow().location.clone(),
        })?;
        let location = self.host.borrow().location.clone();
        self.evaluate_raw(&format!("__meow_websocket_event({id}, {payload})"))
            .map(|_| ())
            .map_err(|error| map_boa_error(error, location))
    }

    fn evaluate_raw(&mut self, source: &str) -> JsResult<JsValue> {
        let _guard = ActiveHostGuard::install(Rc::clone(&self.host))?;
        let value = self.context.eval(Source::from_bytes(source))?;
        self.context.run_jobs()?;
        Ok(value)
    }
}

impl JsRuntime for BoaRuntime {
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
        let _guard = ActiveHostGuard::install(Rc::clone(&self.host))
            .map_err(|error| map_boa_error(error, source.url.clone()))?;
        let value = self
            .context
            .eval(Source::from_reader(
                Cursor::new(source.code.as_bytes()),
                Some(Path::new(source.url.as_str())),
            ))
            .and_then(|value| {
                self.context.run_jobs()?;
                Ok(value)
            })
            .map_err(|error| map_boa_error(error, source.url.clone()))?;
        Ok(script_value(value))
    }

    fn take_mutations(&mut self) -> Vec<DomMutation> {
        std::mem::take(&mut self.host.borrow_mut().mutations)
    }
}

struct ActiveHostGuard;

impl ActiveHostGuard {
    fn install(host: Rc<RefCell<HostState>>) -> JsResult<Self> {
        ACTIVE_HOST.with(|slot| {
            let mut active = slot.borrow_mut();
            if active.is_some() {
                return Err(JsNativeError::error()
                    .with_message("nested MeowEngine JavaScript host activation")
                    .into());
            }
            *active = Some(host);
            Ok(Self)
        })
    }
}

impl Drop for ActiveHostGuard {
    fn drop(&mut self) {
        ACTIVE_HOST.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

struct HostState {
    document: Document,
    location: BrowserUrl,
    storage: StorageBindings,
    mutations: Vec<DomMutation>,
    console: Vec<ConsoleMessage>,
    clock_ms: u64,
    next_timer_id: u64,
    next_timer_sequence: u64,
    timers: BTreeMap<u64, HostTimer>,
    next_fetch_id: u64,
    fetch_tasks: Vec<FetchTask>,
    aborted_signals: BTreeSet<u64>,
    next_websocket_id: u64,
    websocket_commands: Vec<WebSocketCommand>,
}

#[derive(Clone, Debug)]
struct HostTimer {
    due_ms: u64,
    interval_ms: Option<u64>,
    sequence: u64,
}

fn register_host_functions(context: &mut Context) -> JsResult<()> {
    let functions = [
        ("__meow_location", 0, host_location as _),
        ("__meow_document_title", 0, host_document_title as _),
        ("__meow_set_document_title", 1, host_set_document_title as _),
        ("__meow_query_selector", 2, host_query_selector as _),
        ("__meow_local_name", 1, host_local_name as _),
        ("__meow_text_content", 1, host_text_content as _),
        ("__meow_set_text_content", 2, host_set_text_content as _),
        ("__meow_parent_element", 1, host_parent_element as _),
        (
            "__meow_first_element_child",
            1,
            host_first_element_child as _,
        ),
        (
            "__meow_next_element_sibling",
            1,
            host_next_element_sibling as _,
        ),
        ("__meow_get_attribute", 2, host_get_attribute as _),
        ("__meow_set_attribute", 3, host_set_attribute as _),
        ("__meow_remove_attribute", 2, host_remove_attribute as _),
        ("__meow_event_path", 1, host_event_path as _),
        ("__meow_schedule_timer", 2, host_schedule_timer as _),
        ("__meow_cancel_timer", 1, host_cancel_timer as _),
        ("__meow_console", 2, host_console as _),
        ("__meow_form_value", 1, host_form_value as _),
        ("__meow_set_form_value", 2, host_set_form_value as _),
        ("__meow_form_checked", 1, host_form_checked as _),
        ("__meow_set_form_checked", 2, host_set_form_checked as _),
        ("__meow_enqueue_fetch", 1, host_enqueue_fetch as _),
        ("__meow_abort_fetches", 1, host_abort_fetches as _),
        ("__meow_storage_length", 1, host_storage_length as _),
        ("__meow_storage_key", 2, host_storage_key as _),
        ("__meow_storage_get", 2, host_storage_get as _),
        ("__meow_storage_set", 3, host_storage_set as _),
        ("__meow_storage_remove", 2, host_storage_remove as _),
        ("__meow_storage_clear", 1, host_storage_clear as _),
        ("__meow_websocket_command", 1, host_websocket_command as _),
    ];
    for (name, length, function) in functions {
        context.register_global_builtin_callable(
            JsString::from(name),
            length,
            NativeFunction::from_fn_ptr(function),
        )?;
    }
    Ok(())
}

fn with_host<T>(callback: impl FnOnce(&mut HostState) -> JsResult<T>) -> JsResult<T> {
    ACTIVE_HOST.with(|slot| {
        let host = slot
            .borrow()
            .clone()
            .ok_or_else(|| JsNativeError::error().with_message("JavaScript host is not active"))?;
        let mut host = host.try_borrow_mut().map_err(|_| {
            JsNativeError::error().with_message("JavaScript host is already borrowed")
        })?;
        callback(&mut host)
    })
}

fn host_location(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    with_host(|host| Ok(JsValue::from(JsString::from(host.location.as_str()))))
}

fn host_document_title(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    with_host(|host| {
        let title = find_element(&host.document, "title")
            .map(|element| host.document.text_content(&element))
            .unwrap_or_default();
        Ok(JsValue::from(JsString::from(title)))
    })
}

fn host_set_document_title(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let value = argument_string(args, 0, context)?;
    with_host(|host| {
        if let Some(title) = find_element(&host.document, "title") {
            push_mutation(
                &mut host.mutations,
                host.document.replace_text_content(&title, &value),
            )?;
        } else if let Some(head) = find_element(&host.document, "head") {
            let (title, mutation) = host
                .document
                .append_element(&head, "title")
                .map_err(dom_error)?;
            host.mutations.push(mutation);
            let (_, mutation) = host
                .document
                .append_text_node(&title, &value)
                .map_err(dom_error)?;
            host.mutations.push(mutation);
        }
        Ok(JsValue::undefined())
    })
}

fn host_query_selector(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let scope = args.first().cloned().unwrap_or_else(JsValue::null);
    let selector = argument_string(args, 1, context)?;
    let selectors = parse_selector_list(&selector).map_err(|error| {
        JsNativeError::syntax().with_message(format!("invalid selector: {error}"))
    })?;
    with_host(|host| {
        let matched = if scope.is_null() || scope.is_undefined() {
            host.document.query_selector(&selectors)
        } else {
            let root = parse_node_value(&scope, context)?;
            host.document
                .element_by_id(root)
                .into_iter()
                .flat_map(|root| host.document.element_subtree(&root).into_iter().skip(1))
                .find(|element| host.document.matches_selector_list(element, &selectors))
        };
        Ok(node_value(matched.as_ref()))
    })
}

fn host_local_name(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let value = host
            .document
            .element_by_id(node)
            .and_then(|element| host.document.element_local_name(&element))
            .unwrap_or_default();
        Ok(JsValue::from(JsString::from(value)))
    })
}

fn host_text_content(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let value = host
            .document
            .element_by_id(node)
            .map(|element| host.document.text_content(&element))
            .unwrap_or_default();
        Ok(JsValue::from(JsString::from(value)))
    })
}

fn host_set_text_content(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    let value = argument_string(args, 1, context)?;
    with_host(|host| {
        let element = host.document.element_by_id(node).ok_or_else(stale_node)?;
        push_mutation(
            &mut host.mutations,
            host.document.replace_text_content(&element, &value),
        )?;
        Ok(JsValue::undefined())
    })
}

fn host_parent_element(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let parent = host
            .document
            .element_by_id(node)
            .and_then(|element| host.document.parent_element(&element));
        Ok(node_value(parent.as_ref()))
    })
}

fn host_first_element_child(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let child = host
            .document
            .element_by_id(node)
            .and_then(|element| host.document.element_children(&element).into_iter().next());
        Ok(node_value(child.as_ref()))
    })
}

fn host_next_element_sibling(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let sibling = host
            .document
            .element_by_id(node)
            .and_then(|element| host.document.next_element_sibling(&element));
        Ok(node_value(sibling.as_ref()))
    })
}

fn host_get_attribute(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    let name = argument_string(args, 1, context)?;
    with_host(|host| {
        let value = host
            .document
            .element_by_id(node)
            .and_then(|element| host.document.element_attribute(&element, &name));
        Ok(value.map_or_else(JsValue::null, |value| JsValue::from(JsString::from(value))))
    })
}

fn host_set_attribute(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    let name = argument_string(args, 1, context)?;
    let value = argument_string(args, 2, context)?;
    with_host(|host| {
        let element = host.document.element_by_id(node).ok_or_else(stale_node)?;
        push_mutation(
            &mut host.mutations,
            host.document.set_element_attribute(&element, &name, &value),
        )?;
        Ok(JsValue::undefined())
    })
}

fn host_remove_attribute(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    let name = argument_string(args, 1, context)?;
    with_host(|host| {
        let element = host.document.element_by_id(node).ok_or_else(stale_node)?;
        push_mutation(
            &mut host.mutations,
            host.document.remove_element_attribute(&element, &name),
        )?;
        Ok(JsValue::undefined())
    })
}

fn host_event_path(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let mut current = host.document.element_by_id(node).ok_or_else(stale_node)?;
        let mut path = vec![node_id_string(current.id())];
        while let Some(parent) = host.document.parent_element(&current) {
            path.push(node_id_string(parent.id()));
            current = parent;
        }
        path.push("@document".to_owned());
        path.push("@window".to_owned());
        Ok(JsValue::from(JsString::from(path.join(","))))
    })
}

fn host_schedule_timer(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let raw_delay = args.first().and_then(JsValue::as_number).unwrap_or(0.0);
    let repeat = args.get(1).and_then(JsValue::as_boolean).unwrap_or(false);
    let delay_ms = if raw_delay.is_finite() && raw_delay > 0.0 {
        raw_delay.min(u64::MAX as f64) as u64
    } else {
        0
    };
    with_host(|host| {
        let id = host.next_timer_id;
        host.next_timer_id = host.next_timer_id.saturating_add(1);
        let interval_ms = repeat.then_some(delay_ms.max(1));
        let effective_delay = interval_ms.unwrap_or(delay_ms);
        let sequence = host.next_timer_sequence;
        host.next_timer_sequence = host.next_timer_sequence.saturating_add(1);
        host.timers.insert(
            id,
            HostTimer {
                due_ms: host.clock_ms.saturating_add(effective_delay),
                interval_ms,
                sequence,
            },
        );
        Ok(JsValue::from(id as f64))
    })
}

fn host_cancel_timer(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let id = args.first().and_then(JsValue::as_number).unwrap_or(0.0);
    let id = if id.is_finite() && id >= 0.0 {
        id.min(u64::MAX as f64) as u64
    } else {
        0
    };
    with_host(|host| {
        host.timers.remove(&id);
        Ok(JsValue::undefined())
    })
}

fn host_console(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let level = argument_string(args, 0, context)?;
    let message = argument_string(args, 1, context)?;
    with_host(|host| {
        let level = match level.as_str() {
            "info" => ConsoleLevel::Info,
            "warn" => ConsoleLevel::Warn,
            "error" => ConsoleLevel::Error,
            _ => ConsoleLevel::Log,
        };
        host.console.push(ConsoleMessage { level, message });
        Ok(JsValue::undefined())
    })
}

fn host_form_value(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let value = host
            .document
            .element_by_id(node)
            .and_then(|element| host.document.element_attribute(&element, "value"))
            .unwrap_or_default();
        Ok(JsValue::from(JsString::from(value)))
    })
}

fn host_set_form_value(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    let value = argument_string(args, 1, context)?;
    with_host(|host| {
        let element = host.document.element_by_id(node).ok_or_else(stale_node)?;
        push_mutation(
            &mut host.mutations,
            host.document
                .set_element_attribute(&element, "value", &value),
        )?;
        Ok(JsValue::undefined())
    })
}

fn host_form_checked(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    with_host(|host| {
        let checked = host.document.element_by_id(node).is_some_and(|element| {
            host.document
                .element_attribute(&element, "checked")
                .is_some()
        });
        Ok(JsValue::from(checked))
    })
}

fn host_set_form_checked(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = argument_node(args, 0, context)?;
    let checked = args.get(1).and_then(JsValue::as_boolean).unwrap_or(false);
    with_host(|host| {
        let element = host.document.element_by_id(node).ok_or_else(stale_node)?;
        let mutation = if checked {
            host.document.set_element_attribute(&element, "checked", "")
        } else {
            host.document.remove_element_attribute(&element, "checked")
        };
        push_mutation(&mut host.mutations, mutation)?;
        Ok(JsValue::undefined())
    })
}

fn host_enqueue_fetch(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let descriptor = argument_string(args, 0, context)?;
    let raw: RawFetchRequest = serde_json::from_str(&descriptor).map_err(|error| {
        JsNativeError::typ().with_message(format!("invalid fetch request: {error}"))
    })?;
    with_host(|host| {
        let url = BrowserUrl::parse(&raw.url)
            .or_else(|_| host.location.resolve(&raw.url))
            .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
        let id = host.next_fetch_id;
        host.next_fetch_id = host.next_fetch_id.saturating_add(1);
        host.fetch_tasks.push(FetchTask {
            id,
            url,
            document_url: host.location.clone(),
            document_origin: host.location.origin(),
            method: raw.method,
            headers: raw.headers,
            body: raw.body.unwrap_or_default().into_bytes(),
            mode: raw.mode,
            credentials: raw.credentials,
            redirect: raw.redirect,
            signal_id: raw.signal_id,
        });
        Ok(JsValue::from(id as f64))
    })
}

fn host_abort_fetches(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let id = args.first().and_then(JsValue::as_number).unwrap_or(0.0) as u64;
    with_host(|host| {
        host.aborted_signals.insert(id);
        Ok(JsValue::undefined())
    })
}

fn storage_area(host: &HostState, kind: &str) -> JsResult<Rc<RefCell<StorageArea>>> {
    let area = match kind {
        "local" => host.storage.local.as_ref(),
        "session" => host.storage.session.as_ref(),
        _ => None,
    };
    area.cloned().ok_or_else(|| {
        JsNativeError::error()
            .with_message("SecurityError: storage is unavailable for this origin")
            .into()
    })
}

fn host_storage_length(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = argument_string(args, 0, context)?;
    with_host(|host| {
        Ok(JsValue::from(
            storage_area(host, &kind)?.borrow().len() as f64
        ))
    })
}

fn host_storage_key(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = argument_string(args, 0, context)?;
    let index = args
        .get(1)
        .and_then(JsValue::as_number)
        .unwrap_or(0.0)
        .max(0.0) as usize;
    with_host(|host| {
        Ok(storage_area(host, &kind)?
            .borrow()
            .key(index)
            .map_or_else(JsValue::null, |value| JsValue::from(JsString::from(value))))
    })
}

fn host_storage_get(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = argument_string(args, 0, context)?;
    let key = argument_string(args, 1, context)?;
    with_host(|host| {
        Ok(storage_area(host, &kind)?
            .borrow()
            .get(&key)
            .map_or_else(JsValue::null, |value| JsValue::from(JsString::from(value))))
    })
}

fn host_storage_set(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = argument_string(args, 0, context)?;
    let key = argument_string(args, 1, context)?;
    let value = argument_string(args, 2, context)?;
    with_host(|host| {
        storage_area(host, &kind)?
            .borrow_mut()
            .set(key, value)
            .map_err(|error| {
                JsNativeError::error().with_message(format!("QuotaExceededError: {error}"))
            })?;
        Ok(JsValue::undefined())
    })
}

fn host_storage_remove(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = argument_string(args, 0, context)?;
    let key = argument_string(args, 1, context)?;
    with_host(|host| {
        storage_area(host, &kind)?
            .borrow_mut()
            .remove(&key)
            .map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
        Ok(JsValue::undefined())
    })
}

fn host_storage_clear(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let kind = argument_string(args, 0, context)?;
    with_host(|host| {
        storage_area(host, &kind)?
            .borrow_mut()
            .clear()
            .map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
        Ok(JsValue::undefined())
    })
}

fn host_websocket_command(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let descriptor = argument_string(args, 0, context)?;
    let raw: RawWebSocketCommand = serde_json::from_str(&descriptor).map_err(|error| {
        JsNativeError::typ().with_message(format!("invalid WebSocket command: {error}"))
    })?;
    with_host(|host| {
        let (id, command) = match raw {
            RawWebSocketCommand::Connect { url, protocols } => {
                let url = BrowserUrl::parse(&url)
                    .or_else(|_| host.location.resolve(&url))
                    .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
                let id = host.next_websocket_id;
                host.next_websocket_id = host.next_websocket_id.saturating_add(1);
                (
                    id,
                    WebSocketCommand::Connect {
                        id,
                        url,
                        origin: host.location.origin(),
                        protocols,
                    },
                )
            }
            RawWebSocketCommand::SendText { id, data } => {
                (id, WebSocketCommand::SendText { id, data })
            }
            RawWebSocketCommand::SendBinary { id, data } => {
                (id, WebSocketCommand::SendBinary { id, data })
            }
            RawWebSocketCommand::Close { id, code, reason } => {
                (id, WebSocketCommand::Close { id, code, reason })
            }
        };
        host.websocket_commands.push(command);
        Ok(JsValue::from(id as f64))
    })
}

fn push_mutation(
    output: &mut Vec<DomMutation>,
    result: Result<Option<DomMutation>, meow_html::DomMutationError>,
) -> JsResult<()> {
    if let Some(mutation) = result.map_err(dom_error)? {
        output.push(mutation);
    }
    Ok(())
}

fn argument_node(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<NodeId> {
    let value = args.get(index).ok_or_else(|| {
        JsNativeError::typ().with_message(format!("missing node argument {index}"))
    })?;
    parse_node_value(value, context)
}

fn parse_node_value(value: &JsValue, context: &mut Context) -> JsResult<NodeId> {
    let encoded = value.to_string(context)?.to_std_string_escaped();
    let mut parts = encoded.split(':');
    let document = parts.next().and_then(|value| value.parse().ok());
    let slot = parts.next().and_then(|value| value.parse().ok());
    let generation = parts.next().and_then(|value| value.parse().ok());
    if parts.next().is_some() {
        return Err(stale_node().into());
    }
    match (document, slot, generation) {
        (Some(document), Some(slot), Some(generation)) => Ok(NodeId {
            document,
            slot,
            generation,
        }),
        _ => Err(stale_node().into()),
    }
}

fn argument_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    args.get(index)
        .ok_or_else(|| JsNativeError::typ().with_message(format!("missing argument {index}")))?
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

fn node_value(node: Option<&NodeHandle>) -> JsValue {
    node.map_or_else(JsValue::null, |node| {
        JsValue::from(JsString::from(node_id_string(node.id())))
    })
}

fn node_id_string(id: NodeId) -> String {
    format!("{}:{}:{}", id.document, id.slot, id.generation)
}

fn js_string_literal(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                use fmt::Write as _;
                write!(output, "\\u{:04x}", character as u32)
                    .expect("writing to String cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn find_element(document: &Document, local_name: &str) -> Option<NodeHandle> {
    document
        .elements_in_tree_order()
        .into_iter()
        .find(|element| document.element_local_name(element).as_deref() == Some(local_name))
}

fn stale_node() -> JsNativeError {
    JsNativeError::typ().with_message("stale or invalid DOM node handle")
}

fn dom_error(error: meow_html::DomMutationError) -> boa_engine::JsError {
    JsNativeError::typ().with_message(error.to_string()).into()
}

fn map_boa_error(error: boa_engine::JsError, source_url: BrowserUrl) -> ScriptError {
    let message = error.to_string();
    let lowercase = message.to_ascii_lowercase();
    let kind = if lowercase.contains("syntaxerror") || lowercase.contains("parser") {
        ScriptErrorKind::Syntax
    } else if lowercase.contains("iteration limit")
        || lowercase.contains("recursion limit")
        || lowercase.contains("stack size limit")
        || lowercase.contains("maximum call stack")
    {
        ScriptErrorKind::ResourceLimit
    } else if lowercase.contains("javascript host") || lowercase.contains("dom node handle") {
        ScriptErrorKind::Host
    } else {
        ScriptErrorKind::Exception
    };
    ScriptError {
        kind,
        message,
        source_url,
    }
}

fn script_value(value: JsValue) -> ScriptValue {
    if value.is_undefined() {
        ScriptValue::Undefined
    } else if value.is_null() {
        ScriptValue::Null
    } else if let Some(value) = value.as_boolean() {
        ScriptValue::Boolean(value)
    } else if let Some(value) = value.as_number() {
        ScriptValue::Number(value)
    } else if let Some(value) = value.as_string() {
        ScriptValue::String(value.to_std_string_escaped())
    } else {
        ScriptValue::Object
    }
}

#[cfg(test)]
mod tests {
    use meow_html::parse_utf8;

    use super::*;

    fn source(code: &str) -> ScriptSource {
        ScriptSource {
            code: code.to_owned(),
            url: BrowserUrl::parse("https://example.test/app.js").unwrap(),
            node: None,
        }
    }

    #[test]
    fn boa_runtime_maps_values_and_resource_limits() {
        let document = parse_utf8(b"<title>start</title>").document;
        let mut runtime = BoaRuntime::new(
            document,
            BrowserUrl::parse("https://example.test/").unwrap(),
            ScriptLimits {
                loop_iterations: 8,
                ..ScriptLimits::default()
            },
        )
        .unwrap();
        assert_eq!(
            runtime.execute(&source("1 + 3")).unwrap(),
            ScriptValue::Number(4.0)
        );
        let error = runtime.execute(&source("while (true) {}")).unwrap_err();
        assert_eq!(error.kind, ScriptErrorKind::ResourceLimit);
        let error = runtime
            .execute(&source("throw new Error('boom')"))
            .unwrap_err();
        assert_eq!(error.kind, ScriptErrorKind::Exception);
        let error = runtime.execute(&source("const = nope")).unwrap_err();
        assert_eq!(error.kind, ScriptErrorKind::Syntax);
    }

    #[test]
    fn document_bindings_mutate_title_attributes_and_text() {
        let document =
            parse_utf8(b"<title>before</title><main id='target'><span>old</span></main>").document;
        let mut runtime = BoaRuntime::new(
            document.clone(),
            BrowserUrl::parse("https://example.test/page").unwrap(),
            ScriptLimits::default(),
        )
        .unwrap();
        let result = runtime
            .execute(&source(
                r#"
                const target = document.querySelector('#target');
                target.setAttribute('class', 'hot');
                const first = target.firstElementChild;
                const traversal = first.parentElement.localName;
                target.textContent = location.href;
                target.removeAttribute('unused');
                document.title = traversal + ':' + target.getAttribute('class');
                document.title;
                "#,
            ))
            .unwrap();
        assert_eq!(result, ScriptValue::String("main:hot".to_owned()));
        let target = document
            .query_selector(&parse_selector_list("#target").unwrap())
            .unwrap();
        assert_eq!(
            document.element_attribute(&target, "class").as_deref(),
            Some("hot")
        );
        assert_eq!(document.text_content(&target), "https://example.test/page");
        assert!(!runtime.take_mutations().is_empty());
    }

    #[test]
    fn event_target_orders_capture_target_and_bubble_and_honors_once() {
        let document =
            parse_utf8(b"<main id='parent'><button id='target'>go</button></main>").document;
        let target = document
            .query_selector(&parse_selector_list("#target").unwrap())
            .unwrap();
        let mut runtime = BoaRuntime::new(
            document,
            BrowserUrl::parse("https://example.test/events").unwrap(),
            ScriptLimits::default(),
        )
        .unwrap();
        runtime
            .execute(&source(
                r#"
                window.order = [];
                const parent = document.querySelector('#parent');
                const target = document.querySelector('#target');
                window.addEventListener('click', () => order.push('window-capture'), true);
                document.addEventListener('click', () => order.push('document-capture'), true);
                parent.addEventListener('click', () => order.push('parent-capture'), true);
                target.addEventListener('click', event => {
                    order.push('target-once');
                    event.preventDefault();
                }, { once: true });
                parent.addEventListener('click', () => order.push('parent-bubble'));
                document.addEventListener('click', () => order.push('document-bubble'));
                window.addEventListener('click', () => order.push('window-bubble'));
                "#,
            ))
            .unwrap();

        let first = runtime
            .dispatch_event(target.id(), "click", true, true)
            .unwrap();
        assert!(first.default_prevented);
        assert_eq!(
            runtime.execute(&source("order.join('>')")).unwrap(),
            ScriptValue::String(
                "window-capture>document-capture>parent-capture>target-once>parent-bubble>document-bubble>window-bubble"
                    .to_owned(),
            )
        );

        let second = runtime
            .dispatch_event(target.id(), "click", true, true)
            .unwrap();
        assert!(!second.default_prevented);
        assert_eq!(
            runtime
                .execute(&source(
                    "order.filter(value => value === 'target-once').length"
                ))
                .unwrap(),
            ScriptValue::Number(1.0)
        );
    }

    #[test]
    fn timers_and_microtasks_follow_task_order_and_budget() {
        let document = parse_utf8(b"<main></main>").document;
        let mut runtime = BoaRuntime::new(
            document,
            BrowserUrl::parse("https://example.test/timers").unwrap(),
            ScriptLimits::default(),
        )
        .unwrap();
        runtime
            .execute(&source(
                r#"
                window.order = ['sync'];
                Promise.resolve().then(() => order.push('promise'));
                queueMicrotask(() => order.push('microtask'));
                setTimeout(() => {
                    order.push('timeout');
                    queueMicrotask(() => order.push('timeout-microtask'));
                }, 5);
                let ticks = 0;
                const interval = setInterval(() => {
                    order.push('interval-' + (++ticks));
                    if (ticks === 2) clearInterval(interval);
                }, 2);
                "#,
            ))
            .unwrap();
        assert_eq!(
            runtime.execute(&source("order.join('>')")).unwrap(),
            ScriptValue::String("sync>promise>microtask".to_owned())
        );

        let first = runtime.advance_time(2, 8);
        assert_eq!(first.tasks_run, 1);
        assert!(!first.budget_exhausted);
        let second = runtime.advance_time(3, 8);
        assert_eq!(second.tasks_run, 2);
        assert_eq!(second.pending_timers, 0);
        assert_eq!(
            runtime.execute(&source("order.join('>')")).unwrap(),
            ScriptValue::String(
                "sync>promise>microtask>interval-1>interval-2>timeout>timeout-microtask".to_owned(),
            )
        );

        runtime
            .execute(&source(
                "setTimeout(() => {}, 0); setTimeout(() => {}, 0); setTimeout(() => {}, 0);",
            ))
            .unwrap();
        let bounded = runtime.advance_time(0, 2);
        assert_eq!(bounded.tasks_run, 2);
        assert!(bounded.budget_exhausted);
        assert_eq!(bounded.pending_timers, 1);
    }

    #[test]
    fn console_and_form_properties_are_backed_by_dom_state() {
        let document = parse_utf8(
            b"<input id='name' value='before'><input id='done' type='checkbox' checked>",
        )
        .document;
        let mut runtime = BoaRuntime::new(
            document.clone(),
            BrowserUrl::parse("https://example.test/forms").unwrap(),
            ScriptLimits::default(),
        )
        .unwrap();
        runtime
            .execute(&source(
                r#"
                const name = document.querySelector('#name');
                const done = document.querySelector('#done');
                name.value = 'after';
                done.checked = false;
                console.log('form', name.value, done.checked);
                "#,
            ))
            .unwrap();

        let name = document
            .query_selector(&parse_selector_list("#name").unwrap())
            .unwrap();
        let done = document
            .query_selector(&parse_selector_list("#done").unwrap())
            .unwrap();
        assert_eq!(
            document.element_attribute(&name, "value").as_deref(),
            Some("after")
        );
        assert!(document.element_attribute(&done, "checked").is_none());
        assert_eq!(runtime.take_mutations().len(), 2);
        assert_eq!(
            runtime.take_console_messages(),
            vec![ConsoleMessage {
                level: ConsoleLevel::Log,
                message: "form after false".to_owned(),
            }]
        );
    }
}
