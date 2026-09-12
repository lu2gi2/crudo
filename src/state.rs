use chrono::{DateTime, Local};

use crate::attachments::{Attachment, AttachmentState};
use crate::events::{
    Actor, AgentActivity, BackendConnectionStatus, DocumentStage, McpStatus, ModelStatus,
    SandboxStatus,
};
use crate::input::InputState;
use crate::system::SystemMetrics;

/// A single conversational message.
/// Visible in the main chat workspace.
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub id: String,
    pub role: Actor,
    pub content: String,
    pub attachments: Vec<Attachment>,
    pub timestamp: DateTime<Local>,
    pub is_streaming: bool,
}

impl ConversationMessage {
    pub fn new_user(content: String, attachments: Vec<Attachment>) -> Self {
        Self {
            id: format!("usr_{}", Local::now().timestamp_nanos_opt().unwrap_or(0)),
            role: Actor::User,
            content,
            attachments,
            timestamp: Local::now(),
            is_streaming: false,
        }
    }

    pub fn new_crudo(content: String) -> Self {
        Self {
            id: format!("crudo_{}", Local::now().timestamp_nanos_opt().unwrap_or(0)),
            role: Actor::Crudo,
            content,
            attachments: Vec::new(),
            timestamp: Local::now(),
            is_streaming: false,
        }
    }

    pub fn new_streaming(id: String) -> Self {
        Self {
            id,
            role: Actor::Crudo,
            content: String::new(),
            attachments: Vec::new(),
            timestamp: Local::now(),
            is_streaming: true,
        }
    }
}

/// Active document parsing state driven by backend events.
#[derive(Debug, Clone)]
pub struct DocumentProgressState {
    pub document_id: String,
    pub filename: String,
    pub stage: DocumentStage,
    pub current: u64,
    pub total: u64,
    pub percentage: u8,
}

impl DocumentProgressState {
    pub fn new(document_id: String, filename: String) -> Self {
        Self {
            document_id,
            filename,
            stage: DocumentStage::Reading,
            current: 0,
            total: 0,
            percentage: 0,
        }
    }
}

/// Execution activity record for MCP calls, tools, sandbox runs, model steps.
/// Kept separate from the normal conversation so chat remains clean.
#[derive(Debug, Clone)]
pub struct ExecutionActivity {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub category: &'static str,
    pub title: String,
    pub status: &'static str,
    pub details: Option<String>,
}

/// Conversation state holding messages and scroll state.
#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    pub messages: Vec<ConversationMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub active_document: Option<DocumentProgressState>,
    pub activity: AgentActivity,
    pub is_coding_task: bool,
    pub content_height: std::cell::Cell<usize>,
    pub viewport_height: std::cell::Cell<usize>,
    pub last_viewport_height: std::cell::Cell<usize>,
    pub last_total_rows: std::cell::Cell<usize>,
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            active_document: None,
            activity: AgentActivity::Idle,
            is_coding_task: false,
            content_height: std::cell::Cell::new(0),
            viewport_height: std::cell::Cell::new(10),
            last_viewport_height: std::cell::Cell::new(10),
            last_total_rows: std::cell::Cell::new(0),
        }
    }

    pub fn is_welcome(&self) -> bool {
        self.messages.is_empty() && self.active_document.is_none()
    }

    pub fn max_scroll(&self) -> usize {
        let content_h = self.content_height.get().max(self.last_total_rows.get());
        let viewport_h = self
            .viewport_height
            .get()
            .max(self.last_viewport_height.get());
        content_h.saturating_sub(viewport_h)
    }

    pub fn estimate_message_lines(message: &ConversationMessage) -> usize {
        match message.role {
            Actor::User => {
                let text_lines = message.content.lines().count().max(1);
                // Top border with integrated ▶ USER (1), attachments, content, bottom border (1), spacing (2)
                4 + message.attachments.len() + text_lines
            }
            Actor::Crudo => {
                let text_lines = message.content.lines().count().max(1);
                // Label (1), blank (1), content, spacing (2)
                4 + text_lines
            }
        }
    }

    pub fn add_message(&mut self, message: ConversationMessage) {
        let added = Self::estimate_message_lines(&message);
        self.messages.push(message);
        let new_h = self.content_height.get() + added;
        self.content_height.set(new_h);
        self.last_total_rows.set(new_h);

        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
        self.auto_scroll = true;
        self.active_document = None;
        self.activity = AgentActivity::Idle;
        self.is_coding_task = false;
        self.content_height.set(0);
        self.last_total_rows.set(0);
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let max_scroll = self.max_scroll();
        let current = if self.auto_scroll {
            max_scroll
        } else {
            self.scroll_offset.min(max_scroll)
        };
        self.scroll_offset = current.saturating_sub(lines);
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self, lines: usize) {
        let max_scroll = self.max_scroll();
        let current = if self.auto_scroll {
            max_scroll
        } else {
            self.scroll_offset.min(max_scroll)
        };
        self.scroll_offset = (current + lines).min(max_scroll);
        if self.scroll_offset >= max_scroll {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll_up(page_size);
    }

    pub fn scroll_page_down(&mut self, page_size: usize) {
        self.scroll_down(page_size);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        let max_scroll = self.max_scroll();
        self.auto_scroll = max_scroll == 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
        self.auto_scroll = true;
    }
}

/// Subsystem backend status.
#[derive(Debug, Clone)]
pub struct BackendState {
    pub connection: BackendConnectionStatus,
    pub model: ModelStatus,
    pub mcp: McpStatus,
    pub sandbox: SandboxStatus,
}

impl Default for BackendState {
    fn default() -> Self {
        Self {
            connection: BackendConnectionStatus::NotConnected,
            model: ModelStatus::NotConnected,
            mcp: McpStatus::NotConnected,
            sandbox: SandboxStatus::NotConnected,
        }
    }
}

/// Root Application State.
#[derive(Debug, Clone)]
pub struct AppState {
    pub conversation: ConversationState,
    pub input: InputState,
    pub attachments: AttachmentState,
    pub system: SystemMetrics,
    pub backend: BackendState,
    pub execution_log: Vec<ExecutionActivity>,
    pub terminal_size: (u16, u16),
    pub should_quit: bool,
    pub show_activity_drawer: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            conversation: ConversationState::new(),
            input: InputState::new(),
            attachments: AttachmentState::new(),
            system: SystemMetrics::default(),
            backend: BackendState::default(),
            execution_log: Vec::new(),
            terminal_size: (80, 24),
            should_quit: false,
            show_activity_drawer: false,
        }
    }

    pub fn is_welcome(&self) -> bool {
        self.conversation.is_welcome()
    }

    pub fn log_activity(
        &mut self,
        category: &'static str,
        title: String,
        status: &'static str,
        details: Option<String>,
    ) {
        let entry = ExecutionActivity {
            id: format!("act_{}", Local::now().timestamp_nanos_opt().unwrap_or(0)),
            timestamp: Local::now(),
            category,
            title,
            status,
            details,
        };
        self.execution_log.push(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_app_state_is_clean_and_truthful() {
        let state = AppState::new();
        // Conversation must start empty
        assert!(state.conversation.messages.is_empty());
        // Input must start empty
        assert!(state.input.is_empty());
        assert_eq!(state.input.cursor_char_offset(), 0);
        // Backend statuses must be truthfully NotConnected
        assert_eq!(
            state.backend.connection,
            BackendConnectionStatus::NotConnected
        );
        assert_eq!(state.backend.model, ModelStatus::NotConnected);
        assert_eq!(state.backend.mcp, McpStatus::NotConnected);
        assert_eq!(state.backend.sandbox, SandboxStatus::NotConnected);
    }

    #[test]
    fn test_conversation_scrolling() {
        let mut conv = ConversationState::new();
        assert!(conv.auto_scroll);
        assert_eq!(conv.scroll_offset, 0);

        for i in 1..=5 {
            conv.add_message(ConversationMessage::new_user(
                format!("Msg {i}"),
                Vec::new(),
            ));
            conv.add_message(ConversationMessage::new_crudo(format!("Resp {i}")));
        }

        let max_scroll = conv.max_scroll();
        assert!(conv.auto_scroll);
        assert_eq!(conv.scroll_offset, max_scroll);

        // Scroll up 5 lines
        conv.scroll_up(5);
        assert_eq!(conv.scroll_offset, max_scroll.saturating_sub(5));
        assert!(!conv.auto_scroll);

        // Add a message while scrolled up: auto_scroll must remain false
        conv.add_message(ConversationMessage::new_crudo("New msg".to_string()));
        assert!(!conv.auto_scroll);

        // Scroll back down to bottom: auto_scroll becomes true again
        conv.scroll_down(100);
        assert!(conv.auto_scroll);
        assert_eq!(conv.scroll_offset, conv.max_scroll());
    }
}
