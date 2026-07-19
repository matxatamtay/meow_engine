use std::path::{Path, PathBuf};

use meow_embedder_api::{
    BrowserEngine, CancellationToken, Frame, InteractionPoint, InteractionResult, KeyboardCommand,
};
use meow_process_model::{BrowserInteraction, ProcessSupervisor};
use tokio::runtime::Runtime;

pub enum BrowserSession {
    Local {
        engine: Box<BrowserEngine>,
        runtime: Runtime,
    },
    Remote(ProcessSupervisor),
}

impl std::fmt::Debug for BrowserSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local { .. } => formatter.write_str("BrowserSession::Local"),
            Self::Remote(supervisor) => formatter
                .debug_tuple("BrowserSession::Remote")
                .field(supervisor)
                .finish(),
        }
    }
}

impl BrowserSession {
    pub fn local(profile_dir: impl Into<PathBuf>) -> Result<Self, String> {
        Ok(Self::Local {
            engine: Box::new(BrowserEngine::new_with_profile(profile_dir)),
            runtime: Runtime::new().map_err(|error| error.to_string())?,
        })
    }

    pub fn remote(
        executable: impl Into<PathBuf>,
        profile_dir: impl Into<PathBuf>,
        sandbox_enabled: bool,
    ) -> Result<Self, String> {
        ProcessSupervisor::spawn(executable, profile_dir, sandbox_enabled)
            .map(Self::Remote)
            .map_err(|error| error.to_string())
    }

    pub fn navigate(&mut self, url: &str) -> Result<String, String> {
        match self {
            Self::Local { engine, runtime } => {
                runtime
                    .block_on(engine.navigate(url, &CancellationToken::new()))
                    .map_err(|error| error.to_string())?;
                Ok(engine.current_document().url.to_string())
            }
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .navigate(url)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn current_url(&mut self) -> Result<String, String> {
        match self {
            Self::Local { engine, .. } => Ok(engine.current_document().url.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .current_url()
                .map_err(|error| error.to_string()),
        }
    }

    pub fn document_title(&mut self, width: u32, height: u32) -> Result<String, String> {
        match self {
            Self::Local { engine, .. } => engine
                .document_title(width, height)
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .title(width, height)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn render(&mut self, width: u32, height: u32) -> Result<Frame, String> {
        match self {
            Self::Local { engine, .. } => engine
                .render_document_frame(width, height)
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .render(width, height)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn back(&mut self) -> Result<bool, String> {
        match self {
            Self::Local { engine, runtime } => runtime
                .block_on(engine.back(&CancellationToken::new()))
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .back()
                .map_err(|error| error.to_string()),
        }
    }

    pub fn forward(&mut self) -> Result<bool, String> {
        match self {
            Self::Local { engine, runtime } => runtime
                .block_on(engine.forward(&CancellationToken::new()))
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .forward()
                .map_err(|error| error.to_string()),
        }
    }

    pub fn reload(&mut self) -> Result<(), String> {
        match self {
            Self::Local { engine, runtime } => runtime
                .block_on(engine.reload(&CancellationToken::new()))
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .reload()
                .map_err(|error| error.to_string()),
        }
    }

    pub fn scroll(
        &mut self,
        width: u32,
        height: u32,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<bool, String> {
        match self {
            Self::Local { engine, .. } => engine
                .scroll_by(width, height, delta_x, delta_y)
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .scroll(width, height, delta_x, delta_y)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn pointer_down(
        &mut self,
        width: u32,
        height: u32,
        point: InteractionPoint,
    ) -> Result<SessionInteraction, String> {
        match self {
            Self::Local { engine, .. } => engine
                .pointer_down(width, height, point)
                .map(SessionInteraction::from)
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .pointer_down(width, height, point.x, point.y)
                .map(SessionInteraction::from)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn pointer_up(
        &mut self,
        width: u32,
        height: u32,
        point: InteractionPoint,
    ) -> Result<SessionInteraction, String> {
        match self {
            Self::Local { engine, .. } => engine
                .pointer_up(width, height, point)
                .map(SessionInteraction::from)
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .pointer_up(width, height, point.x, point.y)
                .map(SessionInteraction::from)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn keyboard(
        &mut self,
        width: u32,
        height: u32,
        command: KeyboardCommand,
    ) -> Result<SessionInteraction, String> {
        match self {
            Self::Local { engine, .. } => engine
                .keyboard(width, height, command)
                .map(SessionInteraction::from)
                .map_err(|error| error.to_string()),
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .keyboard(width, height, command)
                .map(SessionInteraction::from)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn pump(&mut self, elapsed_ms: u64, max_tasks: usize) -> Result<SessionPump, String> {
        match self {
            Self::Local { engine, runtime } => {
                let timer = engine.advance_time(elapsed_ms, max_tasks);
                let web = runtime.block_on(engine.pump_web_tasks());
                let mut errors = timer
                    .errors
                    .into_iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>();
                errors.extend(web.errors.into_iter().map(|error| error.to_string()));
                let console = take_local_console(engine);
                Ok(SessionPump {
                    timer_tasks: timer.tasks_run,
                    fetches_completed: web.fetches_completed,
                    websocket_events: web.websocket_events,
                    frame_scheduled: engine.mutation_pipeline_report().frame_scheduled,
                    pending: engine.has_pending_timers() || engine.has_pending_web_tasks(),
                    errors,
                    console,
                })
            }
            Self::Remote(supervisor) => supervisor
                .client_mut()
                .pump(elapsed_ms, max_tasks)
                .map(|report| SessionPump {
                    timer_tasks: report.timer_tasks,
                    fetches_completed: report.fetches_completed,
                    websocket_events: report.websocket_events,
                    frame_scheduled: report.frame_scheduled,
                    pending: report.pending,
                    errors: report.errors,
                    console: report.console,
                })
                .map_err(|error| error.to_string()),
        }
    }

    pub fn take_console(&mut self) -> Vec<String> {
        match self {
            Self::Local { engine, .. } => take_local_console(engine),
            Self::Remote(_) => Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    #[allow(dead_code)]
    pub fn profile_dir(&self) -> Option<&Path> {
        match self {
            Self::Local { .. } => None,
            Self::Remote(supervisor) => Some(supervisor.profile_dir()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionInteraction {
    pub redraw: bool,
    pub navigation: Option<String>,
}

impl From<InteractionResult> for SessionInteraction {
    fn from(value: InteractionResult) -> Self {
        Self {
            redraw: value.redraw,
            navigation: value.navigation.map(|url| url.to_string()),
        }
    }
}

impl From<BrowserInteraction> for SessionInteraction {
    fn from(value: BrowserInteraction) -> Self {
        Self {
            redraw: value.redraw,
            navigation: value.navigation,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionPump {
    pub timer_tasks: usize,
    pub fetches_completed: usize,
    pub websocket_events: usize,
    pub frame_scheduled: bool,
    pub pending: bool,
    pub errors: Vec<String>,
    pub console: Vec<String>,
}

fn take_local_console(engine: &mut BrowserEngine) -> Vec<String> {
    engine
        .take_console_messages()
        .into_iter()
        .map(|message| format!("{:?}: {}", message.level, message.message))
        .collect()
}
