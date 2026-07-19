use std::{error::Error, fmt};

#[derive(Debug)]
pub enum ProcessError {
    Io(std::io::Error),
    Ipc(meow_ipc::IpcError),
    Serialization(serde_json::Error),
    Protocol(String),
    Remote(meow_ipc::RemoteError),
    Spawn(String),
    ContentCrashed(String),
    Sandbox(meow_sandbox::SandboxError),
    Join(tokio::task::JoinError),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Ipc(error) => error.fmt(formatter),
            Self::Serialization(error) => error.fmt(formatter),
            Self::Protocol(error) => write!(formatter, "process protocol error: {error}"),
            Self::Remote(error) => write!(formatter, "remote {}: {}", error.code, error.message),
            Self::Spawn(error) => write!(formatter, "process spawn failed: {error}"),
            Self::ContentCrashed(error) => write!(formatter, "content process crashed: {error}"),
            Self::Sandbox(error) => error.fmt(formatter),
            Self::Join(error) => write!(formatter, "process bridge task failed: {error}"),
        }
    }
}

impl Error for ProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Ipc(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::Sandbox(error) => Some(error),
            Self::Join(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ProcessError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<meow_ipc::IpcError> for ProcessError {
    fn from(error: meow_ipc::IpcError) -> Self {
        Self::Ipc(error)
    }
}

impl From<serde_json::Error> for ProcessError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

impl From<meow_sandbox::SandboxError> for ProcessError {
    fn from(error: meow_sandbox::SandboxError) -> Self {
        Self::Sandbox(error)
    }
}

impl From<tokio::task::JoinError> for ProcessError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Join(error)
    }
}
