use std::time::{SystemTime, UNIX_EPOCH};

use meow_display_list::{DisplayCommand, DisplayList, RasterImage, Viewport};
use meow_embedder_api::{Frame, InteractionResult, KeyboardCommand};
use meow_sandbox::SandboxReport;
use serde::{Deserialize, Serialize};

use crate::ProcessError;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ContentRequest {
    Navigate {
        url: String,
    },
    Back,
    Forward,
    Reload,
    Render {
        width: u32,
        height: u32,
    },
    Title {
        width: u32,
        height: u32,
    },
    CurrentUrl,
    Scroll {
        width: u32,
        height: u32,
        delta_x: i32,
        delta_y: i32,
    },
    PointerDown {
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    },
    PointerUp {
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    },
    Keyboard {
        width: u32,
        height: u32,
        key: WireKeyboard,
    },
    Pump {
        elapsed_ms: u64,
        max_tasks: usize,
    },
    Pending,
    SandboxStatus,
    CrashForTest,
    Stop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ContentResponse {
    Ack,
    Bool { value: bool },
    Text { value: String },
    Frame { frame: WireFrame },
    Interaction { interaction: BrowserInteraction },
    Pump { report: PumpReport },
    Sandbox { report: SandboxReport },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireKeyboard {
    Text(String),
    Tab { reverse: bool },
    Enter,
    Space,
    Backspace,
}

impl From<WireKeyboard> for KeyboardCommand {
    fn from(value: WireKeyboard) -> Self {
        match value {
            WireKeyboard::Text(value) => Self::Text(value),
            WireKeyboard::Tab { reverse } => Self::Tab { reverse },
            WireKeyboard::Enter => Self::Enter,
            WireKeyboard::Space => Self::Space,
            WireKeyboard::Backspace => Self::Backspace,
        }
    }
}

impl From<KeyboardCommand> for WireKeyboard {
    fn from(value: KeyboardCommand) -> Self {
        match value {
            KeyboardCommand::Text(value) => Self::Text(value),
            KeyboardCommand::Tab { reverse } => Self::Tab { reverse },
            KeyboardCommand::Enter => Self::Enter,
            KeyboardCommand::Space => Self::Space,
            KeyboardCommand::Backspace => Self::Backspace,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserInteraction {
    pub redraw: bool,
    pub navigation: Option<String>,
}

impl From<InteractionResult> for BrowserInteraction {
    fn from(value: InteractionResult) -> Self {
        Self {
            redraw: value.redraw,
            navigation: value.navigation.map(|url| url.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PumpReport {
    pub timer_tasks: usize,
    pub fetches_completed: usize,
    pub websocket_events: usize,
    pub frame_scheduled: bool,
    pub pending: bool,
    pub errors: Vec<String>,
    pub console: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WireFrame {
    pub viewport: Viewport,
    pub commands: Vec<DisplayCommand>,
    pub images: Vec<RasterImage>,
}

impl WireFrame {
    #[must_use]
    pub fn from_frame(frame: &Frame) -> Self {
        Self {
            viewport: frame.viewport(),
            commands: frame.display_list().commands().to_vec(),
            images: frame.display_list().images().to_vec(),
        }
    }

    pub fn into_frame(self) -> Result<Frame, ProcessError> {
        let viewport = Viewport::new(self.viewport.width, self.viewport.height)
            .map_err(|error| ProcessError::Protocol(error.to_string()))?;
        let display_list = DisplayList::from_wire_parts(self.commands, self.images)
            .map_err(|error| ProcessError::Protocol(error.to_string()))?;
        Ok(Frame::from_parts(viewport, display_list))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrashReport {
    pub process: String,
    pub pid: u32,
    pub message: String,
    pub request_id: Option<u64>,
    pub timestamp_millis: u64,
}

impl CrashReport {
    #[must_use]
    pub fn content(message: String, request_id: Option<u64>) -> Self {
        Self {
            process: "content".to_owned(),
            pid: std::process::id(),
            message,
            request_id,
            timestamp_millis: u64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
            )
            .unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meow_embedder_api::{BrowserEngine, REFERENCE_HEIGHT, REFERENCE_WIDTH};

    #[test]
    fn frame_wire_round_trip_revalidates_display_list() {
        let mut engine = BrowserEngine::new();
        let frame = engine
            .render_frame(REFERENCE_WIDTH, REFERENCE_HEIGHT)
            .unwrap();
        let decoded = WireFrame::from_frame(&frame).into_frame().unwrap();
        assert_eq!(decoded.viewport(), frame.viewport());
        assert_eq!(
            decoded.display_list().commands(),
            frame.display_list().commands()
        );
    }
}
