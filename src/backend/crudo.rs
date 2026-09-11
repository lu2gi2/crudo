use std::fmt;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::attachments::Attachment;
use crate::events::{BackendConnectionStatus, CrudoEvent};

#[derive(Debug, Clone)]
pub enum BackendError {
    NotConnected,
    InvalidState(String),
    CommandFailed(String),
    CommunicationError(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "CRUDO backend is not connected"),
            Self::InvalidState(s) => write!(f, "Invalid backend state: {s}"),
            Self::CommandFailed(s) => write!(f, "Backend command failed: {s}"),
            Self::CommunicationError(s) => write!(f, "Backend communication error: {s}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Transport-independent interface for the CRUDO industrial agent backend.
/// Allows seamless transition between Disconnected state and real IPC/HTTP/Unix-Socket backends.
#[async_trait::async_trait]
pub trait CrudoBackend: Send + Sync {
    /// Submits a user prompt along with any attached files.
    async fn submit_message(
        &self,
        text: &str,
        attachments: &[Attachment],
    ) -> Result<(), BackendError>;

    /// Registers a file attachment with the backend processing pipeline.
    async fn attach_file(&self, attachment: &Attachment) -> Result<(), BackendError>;

    /// Executes an explicit backend command.
    async fn execute_command(&self, cmd: &str, args: &[String]) -> Result<(), BackendError>;

    /// Cancels an in-flight background task or reasoning step.
    async fn cancel_task(&self, task_id: &str) -> Result<(), BackendError>;

    /// Returns the current connection status.
    fn connection_status(&self) -> BackendConnectionStatus;

    /// Subscribes to the live event stream produced by CRUDO.
    fn subscribe(&self) -> broadcast::Receiver<CrudoEvent>;
}

/// Truthful disconnected backend used when no live CRUDO server is attached.
/// Emits no fake responses and reports truthful connection status.
#[derive(Debug, Clone)]
pub struct DisconnectedBackend {
    sender: broadcast::Sender<CrudoEvent>,
}

impl Default for DisconnectedBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DisconnectedBackend {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(128);
        Self { sender }
    }

    pub fn sender(&self) -> broadcast::Sender<CrudoEvent> {
        self.sender.clone()
    }
}

#[async_trait::async_trait]
impl CrudoBackend for DisconnectedBackend {
    async fn submit_message(
        &self,
        _text: &str,
        _attachments: &[Attachment],
    ) -> Result<(), BackendError> {
        Err(BackendError::NotConnected)
    }

    async fn attach_file(&self, _attachment: &Attachment) -> Result<(), BackendError> {
        Err(BackendError::NotConnected)
    }

    async fn execute_command(&self, _cmd: &str, _args: &[String]) -> Result<(), BackendError> {
        Err(BackendError::NotConnected)
    }

    async fn cancel_task(&self, _task_id: &str) -> Result<(), BackendError> {
        Err(BackendError::NotConnected)
    }

    fn connection_status(&self) -> BackendConnectionStatus {
        BackendConnectionStatus::NotConnected
    }

    fn subscribe(&self) -> broadcast::Receiver<CrudoEvent> {
        self.sender.subscribe()
    }
}

pub type SharedBackend = Arc<dyn CrudoBackend>;
