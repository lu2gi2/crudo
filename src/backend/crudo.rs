use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;

use crate::attachments::Attachment;
use crate::events::{AgentEvent, BackendConnectionStatus, CrudoEvent, DocumentStage};

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

/// Interactive demonstration backend simulating event-driven agent activity
/// for manual verification and testing without external network dependencies.
#[derive(Debug, Clone)]
pub struct DemoBackend {
    sender: broadcast::Sender<CrudoEvent>,
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoBackend {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    pub fn sender(&self) -> broadcast::Sender<CrudoEvent> {
        self.sender.clone()
    }
}

#[async_trait::async_trait]
impl CrudoBackend for DemoBackend {
    async fn submit_message(
        &self,
        text: &str,
        attachments: &[Attachment],
    ) -> Result<(), BackendError> {
        let tx = self.sender.clone();
        let query = text.to_string();
        let atts = attachments.to_vec();

        tokio::spawn(async move {
            let msg_id = format!("resp_{}", chrono::Local::now().timestamp_millis());

            if !atts.is_empty() {
                let filename = atts[0].filename.clone();
                let doc_id = format!("doc_{}", chrono::Local::now().timestamp_millis());

                // 1. Document Started
                let _ = tx.send(CrudoEvent::DocumentStarted {
                    document_id: doc_id.clone(),
                    filename: filename.clone(),
                });

                // 2. Document Progress Stages
                let stages = [
                    (DocumentStage::Reading, 20, 2, 10),
                    (DocumentStage::Parsing, 50, 5, 10),
                    (DocumentStage::Extracting, 80, 8, 10),
                    (DocumentStage::Indexing, 100, 10, 10),
                ];

                for (stage, pct, cur, tot) in stages {
                    sleep(Duration::from_millis(500)).await;
                    let _ = tx.send(CrudoEvent::DocumentProgress {
                        document_id: doc_id.clone(),
                        filename: filename.clone(),
                        stage,
                        current: cur,
                        total: tot,
                        percentage: pct,
                    });
                }

                sleep(Duration::from_millis(400)).await;
                // 3. Document Completed -> Triggers Thinking state
                let _ = tx.send(CrudoEvent::DocumentCompleted {
                    document_id: doc_id,
                    filename: filename.clone(),
                });

                // 4. Reasoning / Thinking about the document
                sleep(Duration::from_millis(800)).await;
                let _ = tx.send(CrudoEvent::ResponseStarted {
                    message_id: msg_id.clone(),
                });

                let response_text = format!(
                    "Successfully analyzed {filename}. The document contains operational parameters, piping specifications, and safety thresholds. What specific equipment or line would you like to inspect?"
                );

                for chunk in response_text.split_inclusive(' ') {
                    sleep(Duration::from_millis(40)).await;
                    let _ = tx.send(CrudoEvent::ResponseDelta {
                        message_id: msg_id.clone(),
                        delta: chunk.to_string(),
                    });
                }

                let _ = tx.send(CrudoEvent::ResponseCompleted { message_id: msg_id });
            } else if crate::app::is_coding_request(&query) {
                // Phase 1: Thinking
                let _ = tx.send(CrudoEvent::AgentActivity(AgentEvent::ModelStarted {
                    model: "CRUDO-Coder".to_string(),
                }));

                sleep(Duration::from_millis(1200)).await;

                // Phase 2: Writing Code
                let _ = tx.send(CrudoEvent::AgentActivity(AgentEvent::CodingStarted));

                sleep(Duration::from_millis(400)).await;
                let _ = tx.send(CrudoEvent::ResponseStarted {
                    message_id: msg_id.clone(),
                });

                let code_response = "```python\ndef calculate_distillation_yield(feed_rate: float, efficiency: float) -> float:\n    \"\"\"Calculates crude distillation output yield in barrels per day.\"\"\"\n    if not (0.0 <= efficiency <= 1.0):\n        raise ValueError(\"Efficiency must be between 0.0 and 1.0\")\n    return feed_rate * efficiency\n\n# Example execution\nif __name__ == '__main__':\n    feed = 50_000.0  # bpd\n    eff = 0.88\n    print(f\"Refined Yield: {calculate_distillation_yield(feed, eff):,.1f} bpd\")\n```";

                for line in code_response.split_inclusive('\n') {
                    sleep(Duration::from_millis(50)).await;
                    let _ = tx.send(CrudoEvent::ResponseDelta {
                        message_id: msg_id.clone(),
                        delta: line.to_string(),
                    });
                }

                let _ = tx.send(CrudoEvent::ResponseCompleted { message_id: msg_id });
            } else {
                // General Question: Thinking
                let _ = tx.send(CrudoEvent::AgentActivity(AgentEvent::ModelStarted {
                    model: "CRUDO-Core".to_string(),
                }));

                sleep(Duration::from_millis(1000)).await;

                let _ = tx.send(CrudoEvent::ResponseStarted {
                    message_id: msg_id.clone(),
                });

                let answer = "Crude distillation units (CDU) separate crude petroleum into fractions based on boiling point differentials. Heavy gas oil, atmospheric gas oil, and kerosene are drawn off at successive column tray elevations.";

                for chunk in answer.split_inclusive(' ') {
                    sleep(Duration::from_millis(40)).await;
                    let _ = tx.send(CrudoEvent::ResponseDelta {
                        message_id: msg_id.clone(),
                        delta: chunk.to_string(),
                    });
                }

                let _ = tx.send(CrudoEvent::ResponseCompleted { message_id: msg_id });
            }
        });

        Ok(())
    }

    async fn attach_file(&self, _attachment: &Attachment) -> Result<(), BackendError> {
        Ok(())
    }

    async fn execute_command(&self, _cmd: &str, _args: &[String]) -> Result<(), BackendError> {
        Ok(())
    }

    async fn cancel_task(&self, _task_id: &str) -> Result<(), BackendError> {
        Ok(())
    }

    fn connection_status(&self) -> BackendConnectionStatus {
        BackendConnectionStatus::Connected
    }

    fn subscribe(&self) -> broadcast::Receiver<CrudoEvent> {
        self.sender.subscribe()
    }
}

pub type SharedBackend = Arc<dyn CrudoBackend>;
