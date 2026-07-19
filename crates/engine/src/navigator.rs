//! Top-level navigation, document commit, stylesheet loading, and session history.

use std::collections::VecDeque;

use encoding_rs::{Encoding, UTF_8};
use meow_css::parse_stylesheet;
use meow_html::{
    Document, ScriptCandidate, ScriptCandidateKind, StylesheetCandidateKind, parse_bytes,
    parse_utf8,
};
use meow_net::{CancellationToken, Loader, NetError, Request};
use meow_url_policy::BrowserUrl;

use super::{
    BoaRuntime, ConsoleMessage, EventDispatchResult, FormControlState, ImageCache, JsRuntime,
    ScriptError, ScriptErrorKind, ScriptExecution, ScriptExecutionPhase, ScriptLimits,
    ScriptSource, StorageManager, TimerRunReport, WebPlatform, WebTaskReport,
    encoding::{charset_parameter, sniff_encoding},
    error::NavigationError,
    images::load_document_images,
    model::{
        CharsetSource, DocumentState, DocumentStylesheet, HistoryEntry, StylesheetLoadError,
        StylesheetSource,
    },
};

/// Top-level navigation lifecycle and session-history owner.
#[derive(Debug)]
pub struct Navigator {
    loader: Loader,
    current: DocumentState,
    runtime: BoaRuntime,
    storage: StorageManager,
    image_cache: ImageCache,
    history: Vec<HistoryEntry>,
    next_sequence: u64,
}

impl Navigator {
    /// Creates a navigator with a committed `about:blank` document and history entry.
    #[must_use]
    pub fn new(loader: Loader) -> Self {
        Self::new_with_storage(loader, StorageManager::ephemeral())
    }

    #[must_use]
    pub fn new_with_storage(loader: Loader, mut storage: StorageManager) -> Self {
        let url = BrowserUrl::about_blank();
        let current = blank_document(url.clone(), 0);
        let bindings = storage.bindings_for(&url.origin());
        let runtime = BoaRuntime::new_with_storage(
            current.document.clone(),
            url.clone(),
            ScriptLimits::default(),
            bindings,
        )
        .expect("about:blank JavaScript realm must initialize");
        Self {
            loader,
            current,
            runtime,
            storage,
            image_cache: ImageCache::default(),
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

    /// Dispatches one DOM event in the persistent realm for the committed document.
    pub fn dispatch_event(
        &mut self,
        target: meow_html::NodeId,
        event_type: &str,
        bubbles: bool,
        cancelable: bool,
    ) -> Result<EventDispatchResult, ScriptError> {
        self.runtime
            .dispatch_event(target, event_type, bubbles, cancelable)
    }

    /// Advances the document timer clock with a bounded task budget.
    pub fn advance_time(&mut self, advance_ms: u64, max_tasks: usize) -> TimerRunReport {
        self.runtime.advance_time(advance_ms, max_tasks)
    }

    #[must_use]
    pub fn has_pending_timers(&self) -> bool {
        self.runtime.has_pending_timers()
    }

    pub fn take_console_messages(&mut self) -> Vec<ConsoleMessage> {
        self.runtime.take_console_messages()
    }

    /// Returns the loader's bounded network waterfall.
    #[must_use]
    pub fn network_diagnostics(&self) -> Vec<meow_net::NetworkDiagnostic> {
        self.loader.diagnostics()
    }

    pub async fn pump_web_tasks(&mut self, platform: &mut WebPlatform) -> WebTaskReport {
        platform.pump(&mut self.runtime).await
    }

    #[must_use]
    pub fn has_pending_web_tasks(&self) -> bool {
        self.runtime.has_pending_web_tasks()
    }

    /// Drains runtime DOM records into committed diagnostics and returns this batch.
    pub fn take_runtime_mutations(&mut self) -> Vec<meow_html::DomMutation> {
        let mutations = self.runtime.take_mutations();
        self.current
            .script_mutations
            .extend(mutations.iter().cloned());
        mutations
    }

    /// Mirrors live native form state into DOM attributes visible to JavaScript.
    pub fn sync_form_controls(
        &mut self,
        states: &[FormControlState],
    ) -> Vec<meow_html::DomMutation> {
        let mut mutations = Vec::new();
        for state in states {
            let mutation = match state {
                FormControlState::Text { node, value } => self
                    .current
                    .document
                    .element_by_id(*node)
                    .and_then(|element| {
                        self.current
                            .document
                            .set_element_attribute(&element, "value", value)
                            .ok()
                            .flatten()
                    }),
                FormControlState::Checkbox { node, checked } => self
                    .current
                    .document
                    .element_by_id(*node)
                    .and_then(|element| {
                        let result = if *checked {
                            self.current
                                .document
                                .set_element_attribute(&element, "checked", "")
                        } else {
                            self.current
                                .document
                                .remove_element_attribute(&element, "checked")
                        };
                        result.ok().flatten()
                    }),
            };
            if let Some(mutation) = mutation {
                mutations.push(mutation);
            }
        }
        self.current
            .script_mutations
            .extend(mutations.iter().cloned());
        mutations
    }

    /// Returns required controls that fail the W32 basic validation subset.
    #[must_use]
    pub fn invalid_form_controls(&self, form: meow_html::NodeId) -> Vec<meow_html::NodeId> {
        let Some(form) = self.current.document.element_by_id(form) else {
            return Vec::new();
        };
        self.current
            .document
            .element_subtree(&form)
            .into_iter()
            .skip(1)
            .filter(|element| {
                let local_name = self.current.document.element_local_name(element);
                matches!(local_name.as_deref(), Some("input" | "textarea" | "select"))
                    && self
                        .current
                        .document
                        .element_attribute(element, "required")
                        .is_some()
                    && self
                        .current
                        .document
                        .element_attribute(element, "disabled")
                        .is_none()
            })
            .filter(|element| {
                let input_type = self
                    .current
                    .document
                    .element_attribute(element, "type")
                    .unwrap_or_else(|| "text".to_owned());
                if matches!(input_type.as_str(), "checkbox" | "radio") {
                    self.current
                        .document
                        .element_attribute(element, "checked")
                        .is_none()
                } else {
                    let value = self
                        .current
                        .document
                        .element_attribute(element, "value")
                        .unwrap_or_else(|| self.current.document.text_content(element));
                    value.trim().is_empty()
                }
            })
            .map(|element| element.id())
            .collect()
    }

    /// Returns whether a previous session-history entry exists.
    #[must_use]
    pub const fn can_go_back(&self) -> bool {
        self.current.history_index > 0
    }

    /// Returns whether a following session-history entry exists.
    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        self.current.history_index + 1 < self.history.len()
    }

    /// Resolves, loads, parses, and atomically commits a new top-level navigation.
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

    /// Loads a canonical target and appends a new session-history entry.
    pub async fn navigate_to(
        &mut self,
        target: BrowserUrl,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        tracing::debug!(url = %target, "starting top-level navigation");
        let retained = self.current.history_index + 1;
        let pending = self.load_document(target, retained, cancellation).await?;

        self.history.truncate(retained);
        self.history.push(HistoryEntry {
            sequence: self.next_sequence,
            url: pending.state.url.clone(),
        });
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.current = pending.state;
        self.runtime = pending.runtime;
        tracing::debug!(
            url = %self.current.url,
            history_index = self.current.history_index,
            history_length = self.history.len(),
            "committed top-level navigation"
        );
        Ok(&self.current)
    }

    /// Traverses to the previous history entry. A load failure leaves state untouched.
    pub async fn back(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<Option<&DocumentState>, NavigationError> {
        let Some(index) = self.current.history_index.checked_sub(1) else {
            return Ok(None);
        };
        self.traverse_to(index, cancellation).await.map(Some)
    }

    /// Traverses to the next history entry. A load failure leaves state untouched.
    pub async fn forward(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<Option<&DocumentState>, NavigationError> {
        let index = self.current.history_index + 1;
        if index >= self.history.len() {
            return Ok(None);
        }
        self.traverse_to(index, cancellation).await.map(Some)
    }

    /// Reloads the current URL without adding or replacing a history entry.
    pub async fn reload(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        let index = self.current.history_index;
        let target = self.current.url.clone();
        let pending = self.load_document(target, index, cancellation).await?;
        self.history[index].url = pending.state.url.clone();
        self.current = pending.state;
        self.runtime = pending.runtime;
        tracing::debug!(url = %self.current.url, history_index = index, "reloaded document");
        Ok(&self.current)
    }

    async fn traverse_to(
        &mut self,
        index: usize,
        cancellation: &CancellationToken,
    ) -> Result<&DocumentState, NavigationError> {
        let target = self.history[index].url.clone();
        tracing::debug!(url = %target, history_index = index, "traversing session history");
        let pending = self.load_document(target, index, cancellation).await?;
        self.history[index].url = pending.state.url.clone();
        self.current = pending.state;
        self.runtime = pending.runtime;
        Ok(&self.current)
    }

    async fn load_document(
        &mut self,
        target: BrowserUrl,
        history_index: usize,
        cancellation: &CancellationToken,
    ) -> Result<PendingDocument, NavigationError> {
        if target.as_str() == "about:blank" {
            let state = blank_document(target.clone(), history_index);
            let bindings = self.storage.bindings_for(&target.origin());
            let runtime = BoaRuntime::new_with_storage(
                state.document.clone(),
                target,
                ScriptLimits::default(),
                bindings,
            )
            .map_err(NavigationError::Script)?;
            return Ok(PendingDocument { state, runtime });
        }

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
        let (stylesheets, stylesheet_errors) =
            load_stylesheets(&self.loader, &parsed.document, &base_url, cancellation)
                .await
                .map_err(NavigationError::Network)?;
        let (images, image_errors) = load_document_images(
            &self.loader,
            &parsed.document,
            &base_url,
            cancellation,
            &mut self.image_cache,
        )
        .await;
        let image_cache_metrics = self.image_cache.metrics();
        let storage = self.storage.bindings_for(&final_url.origin());
        let (runtime, script_executions, script_mutations) = execute_document_scripts(
            &self.loader,
            &parsed.document,
            &base_url,
            &final_url,
            storage,
            cancellation,
        )
        .await?;

        Ok(PendingDocument {
            state: DocumentState {
                url: final_url,
                base_url,
                document: parsed.document,
                encoding: encoding.name(),
                charset_source,
                response: Some(response.metadata),
                stylesheets,
                stylesheet_errors,
                script_executions,
                script_mutations,
                images,
                image_errors,
                image_cache_metrics,
                history_index,
            },
            runtime,
        })
    }
}

struct PendingDocument {
    state: DocumentState,
    runtime: BoaRuntime,
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new(Loader::default())
    }
}

fn blank_document(url: BrowserUrl, history_index: usize) -> DocumentState {
    DocumentState {
        url: url.clone(),
        base_url: url,
        document: parse_utf8(b"").document,
        encoding: UTF_8.name(),
        charset_source: CharsetSource::AboutBlank,
        response: None,
        stylesheets: Vec::new(),
        stylesheet_errors: Vec::new(),
        script_executions: Vec::new(),
        script_mutations: Vec::new(),
        images: Default::default(),
        image_errors: Vec::new(),
        image_cache_metrics: Default::default(),
        history_index,
    }
}

#[derive(Debug)]
struct PendingScriptTask {
    node: meow_html::NodeId,
    source_url: BrowserUrl,
    phase: ScriptExecutionPhase,
    source: Result<ScriptSource, ScriptError>,
}

async fn execute_document_scripts(
    loader: &Loader,
    document: &Document,
    base_url: &BrowserUrl,
    document_url: &BrowserUrl,
    storage: super::storage::StorageBindings,
    cancellation: &CancellationToken,
) -> Result<
    (
        BoaRuntime,
        Vec<ScriptExecution>,
        Vec<meow_html::DomMutation>,
    ),
    NavigationError,
> {
    let candidates = document.script_candidates();
    let mut runtime = BoaRuntime::new_with_storage(
        document.clone(),
        document_url.clone(),
        ScriptLimits::default(),
        storage,
    )
    .map_err(NavigationError::Script)?;
    let mut blocking = VecDeque::new();
    let mut deferred = VecDeque::new();
    let mut executions = Vec::new();
    let mut mutations = Vec::new();

    for candidate in candidates {
        let task = prepare_script_task(loader, base_url, document_url, candidate, cancellation)
            .await
            .map_err(NavigationError::Network)?;
        match task.phase {
            ScriptExecutionPhase::ParserBlocking => {
                blocking.push_back(task);
                drain_script_tasks(&mut runtime, &mut blocking, &mut executions, &mut mutations);
            }
            ScriptExecutionPhase::Deferred => deferred.push_back(task),
        }
    }

    drain_script_tasks(&mut runtime, &mut deferred, &mut executions, &mut mutations);
    Ok((runtime, executions, mutations))
}

async fn prepare_script_task(
    loader: &Loader,
    base_url: &BrowserUrl,
    document_url: &BrowserUrl,
    candidate: ScriptCandidate,
    cancellation: &CancellationToken,
) -> Result<PendingScriptTask, NetError> {
    let is_external = matches!(&candidate.kind, ScriptCandidateKind::External(_));
    let phase = if candidate.defer && is_external {
        ScriptExecutionPhase::Deferred
    } else {
        ScriptExecutionPhase::ParserBlocking
    };
    if candidate.async_attribute {
        tracing::debug!(
            node = candidate.node.slot,
            "async classic script uses deterministic parser-blocking scheduling in the alpha subset"
        );
    }

    match candidate.kind {
        ScriptCandidateKind::Inline(code) => Ok(PendingScriptTask {
            node: candidate.node,
            source_url: document_url.clone(),
            phase,
            source: Ok(ScriptSource {
                code,
                url: document_url.clone(),
                node: Some(candidate.node),
            }),
        }),
        ScriptCandidateKind::External(reference) => {
            let requested_url = match base_url.resolve(&reference) {
                Ok(url) => url,
                Err(error) => {
                    let source_url = base_url.clone();
                    return Ok(PendingScriptTask {
                        node: candidate.node,
                        source_url: source_url.clone(),
                        phase,
                        source: Err(script_load_error(source_url, error.to_string())),
                    });
                }
            };
            match loader
                .load(Request::script(requested_url.clone()), cancellation)
                .await
            {
                Ok(response) if response.status.is_success() => {
                    let source_url = response.metadata.final_url.clone();
                    let code = decode_classic_script(
                        &response.body,
                        response.metadata.content_type.as_deref(),
                    );
                    Ok(PendingScriptTask {
                        node: candidate.node,
                        source_url: source_url.clone(),
                        phase,
                        source: Ok(ScriptSource {
                            code,
                            url: source_url,
                            node: Some(candidate.node),
                        }),
                    })
                }
                Ok(response) => Ok(PendingScriptTask {
                    node: candidate.node,
                    source_url: requested_url.clone(),
                    phase,
                    source: Err(script_load_error(
                        requested_url,
                        format!("script HTTP status {}", response.status),
                    )),
                }),
                Err(NetError::Cancelled) => Err(NetError::Cancelled),
                Err(error) => Ok(PendingScriptTask {
                    node: candidate.node,
                    source_url: requested_url.clone(),
                    phase,
                    source: Err(script_load_error(requested_url, error.to_string())),
                }),
            }
        }
    }
}

fn drain_script_tasks(
    runtime: &mut BoaRuntime,
    queue: &mut VecDeque<PendingScriptTask>,
    executions: &mut Vec<ScriptExecution>,
    mutations: &mut Vec<meow_html::DomMutation>,
) {
    while let Some(task) = queue.pop_front() {
        let error = match task.source {
            Ok(source) => runtime.execute(&source).err(),
            Err(error) => Some(error),
        };
        mutations.extend(runtime.take_mutations());
        executions.push(ScriptExecution {
            node: Some(task.node),
            source_url: task.source_url,
            phase: task.phase,
            error,
        });
    }
}

fn script_load_error(source_url: BrowserUrl, message: String) -> ScriptError {
    ScriptError {
        kind: ScriptErrorKind::Load,
        message,
        source_url,
    }
}

fn decode_classic_script(bytes: &[u8], content_type: Option<&str>) -> String {
    let encoding = content_type
        .and_then(charset_parameter)
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(UTF_8);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

async fn load_stylesheets(
    loader: &Loader,
    document: &Document,
    base_url: &BrowserUrl,
    cancellation: &CancellationToken,
) -> Result<(Vec<DocumentStylesheet>, Vec<StylesheetLoadError>), NetError> {
    let mut stylesheets = Vec::new();
    let mut errors = Vec::new();

    for candidate in document.stylesheet_candidates() {
        match candidate.kind {
            StylesheetCandidateKind::Inline(css) => stylesheets.push(DocumentStylesheet {
                source: StylesheetSource::Inline {
                    node: candidate.node,
                },
                media: candidate.media,
                stylesheet: parse_stylesheet(&css),
            }),
            StylesheetCandidateKind::Linked(href) => {
                let requested_url = match base_url.resolve(&href) {
                    Ok(url) => url,
                    Err(error) => {
                        errors.push(StylesheetLoadError {
                            node: candidate.node,
                            href: Some(href),
                            message: error.to_string(),
                        });
                        continue;
                    }
                };
                match loader
                    .load(Request::stylesheet(requested_url.clone()), cancellation)
                    .await
                {
                    Ok(response) if response.status.is_success() => {
                        let css = decode_stylesheet(
                            &response.body,
                            response.metadata.content_type.as_deref(),
                        );
                        stylesheets.push(DocumentStylesheet {
                            source: StylesheetSource::External {
                                node: candidate.node,
                                requested_url,
                                final_url: response.metadata.final_url,
                            },
                            media: candidate.media,
                            stylesheet: parse_stylesheet(&css),
                        });
                    }
                    Ok(response) => errors.push(StylesheetLoadError {
                        node: candidate.node,
                        href: Some(href),
                        message: format!("stylesheet HTTP status {}", response.status),
                    }),
                    Err(NetError::Cancelled) => return Err(NetError::Cancelled),
                    Err(error) => errors.push(StylesheetLoadError {
                        node: candidate.node,
                        href: Some(href),
                        message: error.to_string(),
                    }),
                }
            }
        }
    }

    Ok((stylesheets, errors))
}

fn decode_stylesheet(bytes: &[u8], content_type: Option<&str>) -> String {
    let encoding = content_type
        .and_then(charset_parameter)
        .and_then(|label| Encoding::for_label(label.as_bytes()))
        .unwrap_or(UTF_8);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}
