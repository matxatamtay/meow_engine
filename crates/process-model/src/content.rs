use std::{
    any::Any,
    fs,
    os::unix::net::{UnixListener, UnixStream},
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use meow_embedder_api::{BrowserEngine, CancellationToken, InteractionPoint, KeyboardCommand};
use meow_ipc::{Connection, Envelope, MessageKind, RemoteError, StreamTransport};
use meow_net::{LoadConfig, Loader};
use meow_sandbox::{SandboxPolicy, SandboxReport, apply_content_sandbox};

use crate::{
    BrowserInteraction, ContentRequest, ContentResponse, CrashReport, NetworkBrokerClient,
    ProcessError, PumpReport, WireFrame,
};

pub fn run_content_process(
    socket_path: impl AsRef<Path>,
    network_socket_path: impl AsRef<Path>,
    profile_dir: impl AsRef<Path>,
    sandbox_root: impl AsRef<Path>,
    crash_report_path: impl AsRef<Path>,
    enable_sandbox: bool,
) -> Result<(), ProcessError> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        fs::remove_file(socket_path)?;
    }
    fs::create_dir_all(profile_dir.as_ref())?;
    fs::create_dir_all(sandbox_root.as_ref())?;

    let listener = UnixListener::bind(socket_path)?;
    let network = NetworkBrokerClient::connect(network_socket_path)?;
    let loader = Loader::brokered(Arc::new(network), LoadConfig::default());
    let runtime = tokio::runtime::Runtime::new()?;
    let mut engine = BrowserEngine::new_with_loader_and_profile(loader, profile_dir.as_ref());
    let (stream, _) = listener.accept()?;
    drop(listener);

    let sandbox_report = if enable_sandbox {
        apply_content_sandbox(&SandboxPolicy::content(sandbox_root.as_ref()))?
    } else {
        SandboxReport {
            gaps: vec!["content sandbox disabled by launch option".to_owned()],
            ..SandboxReport::default()
        }
    };

    let current_request = AtomicU64::new(0);
    let mut connection = Connection::new(StreamTransport::new(stream));
    let result = catch_unwind(AssertUnwindSafe(|| {
        serve_content_connection(
            &mut connection,
            &runtime,
            &mut engine,
            &sandbox_report,
            &current_request,
        )
    }));
    let _ = fs::remove_file(socket_path);

    match result {
        Ok(result) => result,
        Err(payload) => {
            let message = panic_message(payload.as_ref());
            let request_id = match current_request.load(Ordering::Relaxed) {
                0 => None,
                value => Some(value),
            };
            let report = CrashReport::content(message.clone(), request_id);
            fs::write(crash_report_path, serde_json::to_vec_pretty(&report)?)?;
            Err(ProcessError::ContentCrashed(message))
        }
    }
}

fn serve_content_connection(
    connection: &mut Connection<StreamTransport<UnixStream>>,
    runtime: &tokio::runtime::Runtime,
    engine: &mut BrowserEngine,
    sandbox_report: &SandboxReport,
    current_request: &AtomicU64,
) -> Result<(), ProcessError> {
    loop {
        let request: Envelope<ContentRequest> = connection.receive()?;
        current_request.store(request.request_id.0, Ordering::Relaxed);
        if request.kind != MessageKind::Request {
            connection.send(&Envelope::<ContentResponse>::failure(
                request.request_id,
                RemoteError::new(
                    "unexpected_message",
                    "content process accepts requests only",
                ),
            ))?;
            continue;
        }
        let Some(payload) = request.payload else {
            connection.send(&Envelope::<ContentResponse>::failure(
                request.request_id,
                RemoteError::new("invalid_request", "content request omitted payload"),
            ))?;
            continue;
        };

        match handle_content_request(payload, runtime, engine, sandbox_report) {
            Ok((response, keep_running)) => {
                connection.send(&Envelope::response(request.request_id, response))?;
                if !keep_running {
                    return Ok(());
                }
            }
            Err(error) => connection.send(&Envelope::<ContentResponse>::failure(
                request.request_id,
                RemoteError::new("content_error", error.to_string()),
            ))?,
        }
    }
}

fn handle_content_request(
    request: ContentRequest,
    runtime: &tokio::runtime::Runtime,
    engine: &mut BrowserEngine,
    sandbox_report: &SandboxReport,
) -> Result<(ContentResponse, bool), ProcessError> {
    let response = match request {
        ContentRequest::Navigate { url } => {
            runtime
                .block_on(engine.navigate(&url, &CancellationToken::new()))
                .map_err(|error| ProcessError::Protocol(error.to_string()))?;
            ContentResponse::Text {
                value: engine.current_document().url.to_string(),
            }
        }
        ContentRequest::Back => ContentResponse::Bool {
            value: runtime
                .block_on(engine.back(&CancellationToken::new()))
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
        },
        ContentRequest::Forward => ContentResponse::Bool {
            value: runtime
                .block_on(engine.forward(&CancellationToken::new()))
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
        },
        ContentRequest::Reload => {
            runtime
                .block_on(engine.reload(&CancellationToken::new()))
                .map_err(|error| ProcessError::Protocol(error.to_string()))?;
            ContentResponse::Ack
        }
        ContentRequest::Render { width, height } => ContentResponse::Frame {
            frame: WireFrame::from_frame(
                &engine
                    .render_document_frame(width, height)
                    .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            ),
        },
        ContentRequest::Title { width, height } => ContentResponse::Text {
            value: engine
                .document_title(width, height)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
        },
        ContentRequest::CurrentUrl => ContentResponse::Text {
            value: engine.current_document().url.to_string(),
        },
        ContentRequest::Scroll {
            width,
            height,
            delta_x,
            delta_y,
        } => ContentResponse::Bool {
            value: engine
                .scroll_by(width, height, delta_x, delta_y)
                .map_err(|error| ProcessError::Protocol(error.to_string()))?,
        },
        ContentRequest::PointerDown {
            width,
            height,
            x,
            y,
        } => ContentResponse::Interaction {
            interaction: BrowserInteraction::from(
                engine
                    .pointer_down(width, height, InteractionPoint::new(x, y))
                    .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            ),
        },
        ContentRequest::PointerUp {
            width,
            height,
            x,
            y,
        } => ContentResponse::Interaction {
            interaction: BrowserInteraction::from(
                engine
                    .pointer_up(width, height, InteractionPoint::new(x, y))
                    .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            ),
        },
        ContentRequest::Keyboard { width, height, key } => ContentResponse::Interaction {
            interaction: BrowserInteraction::from(
                engine
                    .keyboard(width, height, KeyboardCommand::from(key))
                    .map_err(|error| ProcessError::Protocol(error.to_string()))?,
            ),
        },
        ContentRequest::Pump {
            elapsed_ms,
            max_tasks,
        } => {
            let timer = engine.advance_time(elapsed_ms, max_tasks);
            let web = runtime.block_on(engine.pump_web_tasks());
            let mut errors = timer
                .errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>();
            errors.extend(web.errors.into_iter().map(|error| error.to_string()));
            let console = engine
                .take_console_messages()
                .into_iter()
                .map(|message| format!("{:?}: {}", message.level, message.message))
                .collect();
            ContentResponse::Pump {
                report: PumpReport {
                    timer_tasks: timer.tasks_run,
                    fetches_completed: web.fetches_completed,
                    websocket_events: web.websocket_events,
                    frame_scheduled: engine.mutation_pipeline_report().frame_scheduled,
                    pending: engine.has_pending_timers() || engine.has_pending_web_tasks(),
                    errors,
                    console,
                },
            }
        }
        ContentRequest::Pending => ContentResponse::Bool {
            value: engine.has_pending_timers() || engine.has_pending_web_tasks(),
        },
        ContentRequest::SandboxStatus => ContentResponse::Sandbox {
            report: sandbox_report.clone(),
        },
        ContentRequest::CrashForTest => panic!("intentional content crash for containment test"),
        ContentRequest::Stop => return Ok((ContentResponse::Ack, false)),
    };
    Ok((response, true))
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "non-string panic payload".to_owned()
    }
}
