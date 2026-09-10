use std::sync::{Arc, Mutex};

/// Thread-safe responder allowing the TUI to deliver an asynchronous
/// permission decision back to the blocking ConversationRuntime turn.
#[derive(Debug, Clone)]
pub struct PermissionResponder {
    sender: Arc<Mutex<Option<std::sync::mpsc::SyncSender<runtime::PermissionPromptDecision>>>>,
}

impl PermissionResponder {
    pub fn new(sender: std::sync::mpsc::SyncSender<runtime::PermissionPromptDecision>) -> Self {
        Self {
            sender: Arc::new(Mutex::new(Some(sender))),
        }
    }

    pub fn respond(&self, decision: runtime::PermissionPromptDecision) -> bool {
        if let Ok(mut guard) = self.sender.lock() {
            if let Some(sender) = guard.take() {
                let _ = sender.send(decision);
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Started,
    Thinking,
    ToolStarted {
        tool: String,
        summary: String,
    },
    ToolOutput {
        id: usize,
        output: String,
    },
    ToolFinished {
        id: usize,
        duration_ms: u64,
    },
    TextChunk(String),
    PermissionRequested {
        id: usize,
        tool_name: String,
        summary: String,
        current_mode: String,
        reason: Option<String>,
        responder: PermissionResponder,
    },
    PermissionResolved {
        id: usize,
        allowed: bool,
    },
    Completed,
    Error(String),
    Cancelled,
}
