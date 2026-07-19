use std::{
    fs,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use meow_embedder_api::{Frame, KeyboardCommand};
use meow_ipc::{Connection, Envelope, MessageKind, RequestId, StreamTransport};
use meow_sandbox::SandboxReport;

use crate::{
    BrowserInteraction, ContentRequest, ContentResponse, CrashReport, ProcessError, PumpReport,
    WireKeyboard,
};

pub struct ContentProcessClient {
    connection: Connection<StreamTransport<UnixStream>>,
    next_request_id: u64,
}

impl std::fmt::Debug for ContentProcessClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContentProcessClient")
            .field("next_request_id", &self.next_request_id)
            .finish_non_exhaustive()
    }
}

impl ContentProcessClient {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, ProcessError> {
        Ok(Self {
            connection: Connection::new(StreamTransport::new(UnixStream::connect(path)?)),
            next_request_id: 1,
        })
    }

    pub fn request(&mut self, request: ContentRequest) -> Result<ContentResponse, ProcessError> {
        let request_id = RequestId(self.next_request_id);
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.connection
            .send(&Envelope::request(request_id, request))?;
        let response: Envelope<ContentResponse> = self.connection.receive()?;
        if response.kind != MessageKind::Response {
            return Err(ProcessError::Protocol(format!(
                "content returned {:?} instead of response",
                response.kind
            )));
        }
        if response.request_id != request_id {
            return Err(ProcessError::Protocol(format!(
                "content request ID mismatch: expected {}, got {}",
                request_id.0, response.request_id.0
            )));
        }
        if let Some(error) = response.error {
            return Err(ProcessError::Remote(error));
        }
        response
            .payload
            .ok_or_else(|| ProcessError::Protocol("content response omitted payload".to_owned()))
    }

    pub fn navigate(&mut self, url: impl Into<String>) -> Result<String, ProcessError> {
        expect_text(self.request(ContentRequest::Navigate { url: url.into() })?)
    }

    pub fn back(&mut self) -> Result<bool, ProcessError> {
        expect_bool(self.request(ContentRequest::Back)?)
    }

    pub fn forward(&mut self) -> Result<bool, ProcessError> {
        expect_bool(self.request(ContentRequest::Forward)?)
    }

    pub fn reload(&mut self) -> Result<(), ProcessError> {
        expect_ack(self.request(ContentRequest::Reload)?)
    }

    pub fn render(&mut self, width: u32, height: u32) -> Result<Frame, ProcessError> {
        match self.request(ContentRequest::Render { width, height })? {
            ContentResponse::Frame { frame } => frame.into_frame(),
            response => Err(unexpected_response("frame", &response)),
        }
    }

    pub fn title(&mut self, width: u32, height: u32) -> Result<String, ProcessError> {
        expect_text(self.request(ContentRequest::Title { width, height })?)
    }

    pub fn current_url(&mut self) -> Result<String, ProcessError> {
        expect_text(self.request(ContentRequest::CurrentUrl)?)
    }

    pub fn scroll(
        &mut self,
        width: u32,
        height: u32,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<bool, ProcessError> {
        expect_bool(self.request(ContentRequest::Scroll {
            width,
            height,
            delta_x,
            delta_y,
        })?)
    }

    pub fn pointer_down(
        &mut self,
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    ) -> Result<BrowserInteraction, ProcessError> {
        expect_interaction(self.request(ContentRequest::PointerDown {
            width,
            height,
            x,
            y,
        })?)
    }

    pub fn pointer_up(
        &mut self,
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    ) -> Result<BrowserInteraction, ProcessError> {
        expect_interaction(self.request(ContentRequest::PointerUp {
            width,
            height,
            x,
            y,
        })?)
    }

    pub fn keyboard(
        &mut self,
        width: u32,
        height: u32,
        key: KeyboardCommand,
    ) -> Result<BrowserInteraction, ProcessError> {
        expect_interaction(self.request(ContentRequest::Keyboard {
            width,
            height,
            key: WireKeyboard::from(key),
        })?)
    }

    pub fn pump(&mut self, elapsed_ms: u64, max_tasks: usize) -> Result<PumpReport, ProcessError> {
        match self.request(ContentRequest::Pump {
            elapsed_ms,
            max_tasks,
        })? {
            ContentResponse::Pump { report } => Ok(report),
            response => Err(unexpected_response("pump report", &response)),
        }
    }

    pub fn pending(&mut self) -> Result<bool, ProcessError> {
        expect_bool(self.request(ContentRequest::Pending)?)
    }

    pub fn sandbox_status(&mut self) -> Result<SandboxReport, ProcessError> {
        match self.request(ContentRequest::SandboxStatus)? {
            ContentResponse::Sandbox { report } => Ok(report),
            response => Err(unexpected_response("sandbox report", &response)),
        }
    }

    pub fn inspector_snapshot(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<meow_inspector::InspectorSnapshot, ProcessError> {
        match self.request(ContentRequest::Inspect { width, height })? {
            ContentResponse::Inspector { snapshot } => Ok(*snapshot),
            response => Err(unexpected_response("inspector snapshot", &response)),
        }
    }

    pub fn stop(&mut self) -> Result<(), ProcessError> {
        expect_ack(self.request(ContentRequest::Stop)?)
    }
}

pub struct ProcessSupervisor {
    executable: PathBuf,
    runtime_dir: PathBuf,
    profile_dir: PathBuf,
    network_socket: PathBuf,
    content_socket: PathBuf,
    sandbox_root: PathBuf,
    crash_report: PathBuf,
    sandbox_enabled: bool,
    network_child: Child,
    content_child: Child,
    client: ContentProcessClient,
}

impl std::fmt::Debug for ProcessSupervisor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessSupervisor")
            .field("runtime_dir", &self.runtime_dir)
            .field("profile_dir", &self.profile_dir)
            .field("sandbox_enabled", &self.sandbox_enabled)
            .field("network_pid", &self.network_child.id())
            .field("content_pid", &self.content_child.id())
            .finish()
    }
}

impl ProcessSupervisor {
    pub fn spawn(
        executable: impl Into<PathBuf>,
        profile_dir: impl Into<PathBuf>,
        sandbox_enabled: bool,
    ) -> Result<Self, ProcessError> {
        let executable = executable.into();
        let profile_dir = absolute_path(profile_dir.into())?;
        fs::create_dir_all(&profile_dir)?;
        let runtime_dir = unique_runtime_dir();
        fs::create_dir_all(&runtime_dir)?;
        let network_socket = runtime_dir.join("network.sock");
        let content_socket = runtime_dir.join("content.sock");
        let sandbox_root = runtime_dir.join("content-root");
        let crash_report = runtime_dir.join("content-crash.json");
        fs::create_dir_all(&sandbox_root)?;

        let mut network_child = Command::new(&executable)
            .arg("--meow-network-process")
            .arg(&network_socket)
            .spawn()
            .map_err(|error| ProcessError::Spawn(error.to_string()))?;
        wait_for_socket(&network_socket, &mut network_child, "network")?;

        let mut content_child = spawn_content_child(
            &executable,
            &content_socket,
            &network_socket,
            &profile_dir,
            &sandbox_root,
            &crash_report,
            sandbox_enabled,
        )?;
        wait_for_socket(&content_socket, &mut content_child, "content")?;
        let client = ContentProcessClient::connect(&content_socket)?;

        Ok(Self {
            executable,
            runtime_dir,
            profile_dir,
            network_socket,
            content_socket,
            sandbox_root,
            crash_report,
            sandbox_enabled,
            network_child,
            content_child,
            client,
        })
    }

    #[must_use]
    pub fn client(&self) -> &ContentProcessClient {
        &self.client
    }

    pub fn client_mut(&mut self) -> &mut ContentProcessClient {
        &mut self.client
    }

    #[must_use]
    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    pub fn restart_content(&mut self) -> Result<(), ProcessError> {
        terminate_child(&mut self.content_child);
        let _ = fs::remove_file(&self.content_socket);
        let _ = fs::remove_file(&self.crash_report);
        self.content_child = spawn_content_child(
            &self.executable,
            &self.content_socket,
            &self.network_socket,
            &self.profile_dir,
            &self.sandbox_root,
            &self.crash_report,
            self.sandbox_enabled,
        )?;
        wait_for_socket(&self.content_socket, &mut self.content_child, "content")?;
        self.client = ContentProcessClient::connect(&self.content_socket)?;
        Ok(())
    }

    pub fn crash_content_for_test(&mut self) -> Result<CrashReport, ProcessError> {
        let result = self.client.request(ContentRequest::CrashForTest);
        if result.is_ok() {
            return Err(ProcessError::Protocol(
                "content crash test unexpectedly returned a response".to_owned(),
            ));
        }
        let status = self.content_child.wait()?;
        if status.success() {
            return Err(ProcessError::Protocol(
                "content crash test exited successfully".to_owned(),
            ));
        }
        let report = read_crash_report(&self.crash_report, status)?;
        self.restart_content()?;
        Ok(report)
    }
}

impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        let _ = self.client.stop();
        terminate_child(&mut self.content_child);
        terminate_child(&mut self.network_child);
        let _ = fs::remove_dir_all(&self.runtime_dir);
    }
}

fn spawn_content_child(
    executable: &Path,
    content_socket: &Path,
    network_socket: &Path,
    profile_dir: &Path,
    sandbox_root: &Path,
    crash_report: &Path,
    sandbox_enabled: bool,
) -> Result<Child, ProcessError> {
    Command::new(executable)
        .arg("--meow-content-process")
        .arg(content_socket)
        .arg(network_socket)
        .arg(profile_dir)
        .arg(sandbox_root)
        .arg(crash_report)
        .arg(if sandbox_enabled { "1" } else { "0" })
        .spawn()
        .map_err(|error| ProcessError::Spawn(error.to_string()))
}

fn wait_for_socket(path: &Path, child: &mut Child, name: &str) -> Result<(), ProcessError> {
    for _ in 0..500 {
        if path.exists() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(ProcessError::Spawn(format!(
                "{name} process exited before IPC socket was ready: {status}"
            )));
        }
        thread::sleep(Duration::from_millis(10));
    }
    terminate_child(child);
    Err(ProcessError::Spawn(format!(
        "{name} process did not publish its IPC socket"
    )))
}

fn terminate_child(child: &mut Child) {
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn read_crash_report(path: &Path, status: ExitStatus) -> Result<CrashReport, ProcessError> {
    let bytes = fs::read(path).map_err(|error| {
        ProcessError::ContentCrashed(format!(
            "content exited with {status}, but crash report could not be read: {error}"
        ))
    })?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn unique_runtime_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("meow-process-{}-{nonce}", std::process::id()))
}

fn absolute_path(path: PathBuf) -> Result<PathBuf, ProcessError> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn expect_ack(response: ContentResponse) -> Result<(), ProcessError> {
    match response {
        ContentResponse::Ack => Ok(()),
        response => Err(unexpected_response("ack", &response)),
    }
}

fn expect_bool(response: ContentResponse) -> Result<bool, ProcessError> {
    match response {
        ContentResponse::Bool { value } => Ok(value),
        response => Err(unexpected_response("boolean", &response)),
    }
}

fn expect_text(response: ContentResponse) -> Result<String, ProcessError> {
    match response {
        ContentResponse::Text { value } => Ok(value),
        response => Err(unexpected_response("text", &response)),
    }
}

fn expect_interaction(response: ContentResponse) -> Result<BrowserInteraction, ProcessError> {
    match response {
        ContentResponse::Interaction { interaction } => Ok(interaction),
        response => Err(unexpected_response("interaction", &response)),
    }
}

fn unexpected_response(expected: &str, response: &ContentResponse) -> ProcessError {
    ProcessError::Protocol(format!("expected {expected} response, got {response:?}"))
}
