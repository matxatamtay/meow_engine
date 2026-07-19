//! W41-W44 multiprocess browser boundary.

mod content;
mod error;
mod network;
mod protocol;
mod supervisor;

pub use content::run_content_process;
pub use error::ProcessError;
pub use network::{NetworkBrokerClient, run_network_process};
pub use protocol::{
    BrowserInteraction, ContentRequest, ContentResponse, CrashReport, PumpReport, WireFrame,
    WireKeyboard,
};
pub use supervisor::{ContentProcessClient, ProcessSupervisor};
