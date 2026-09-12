use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::sync::Arc;
use std::time::Duration;

use crate::attachments::Attachment;
use crate::backend::{DisconnectedBackend, SharedBackend};
use crate::commands::SlashCommand;
use crate::event::{AppEvent, EventHandler};
use crate::events::{Actor, AgentActivity, AgentEvent, CrudoEvent};
use crate::state::{AppState, ConversationMessage, DocumentProgressState};
use crate::ui::UI;

pub struct App {
    pub state: AppState,
    pub backend: SharedBackend,
    pub ui: UI,
}

impl App {
    pub fn new(backend: Option<SharedBackend>, logo_path: &str) -> Self {
        let backend = backend.unwrap_or_else(|| Arc::new(DisconnectedBackend::new()));
        let mut state = AppState::new();
        state.backend.connection = backend.connection_status();

        let ui = UI::new(logo_path);

        Self { state, backend, ui }
    }

    /// Primary event loop for the application.
    pub async fn run<B>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        B: Backend,
        <B as Backend>::Error: std::error::Error + 'static,
    {
        let backend_rx = self.backend.subscribe();
        let mut events = EventHandler::new(
            Duration::from_millis(250), // 4 Hz UI tick rate
            Duration::from_secs(1),     // 1 Hz CPU/GPU metrics update
            Some(backend_rx),
        );

        // Initial draw
        terminal.draw(|f| self.ui.render(f, &self.state))?;

        while !self.state.should_quit {
            if let Some(event) = events.next().await {
                self.process_event(event).await;

                // Drain and apply any pending events in the queue before redrawing
                // This coalesces high-frequency touchpad scroll bursts into a single render pass
                while let Some(pending) = events.try_recv() {
                    self.process_event(pending).await;
                    if self.state.should_quit {
                        break;
                    }
                }

                // Render latest state only once for the whole event batch
                terminal.draw(|f| self.ui.render(f, &self.state))?;
            }
        }

        Ok(())
    }

    /// Dispatches an application event to the appropriate handler.
    pub async fn process_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                self.handle_key(key).await;
            }
            AppEvent::Mouse(mouse) => {
                self.handle_mouse(mouse);
            }
            AppEvent::Resize(w, h) => {
                self.state.terminal_size = (w, h);
            }
            AppEvent::Tick => {
                // Triggers periodic re-render for real-time clock and animations
            }
            AppEvent::SystemUpdate(metrics) => {
                self.state.system = metrics;
            }
            AppEvent::Backend(crudo_event) => {
                self.handle_backend_event(crudo_event);
            }
        }
    }

    /// Handles touchpad and mouse scroll events for natural, smooth chat viewport scrolling.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.state.conversation.scroll_up(1);
            }
            MouseEventKind::ScrollDown => {
                self.state.conversation.scroll_down(1);
            }
            _ => {}
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) {
        // Global quit shortcut: Ctrl+C
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.state.should_quit = true;
            return;
        }

        // Ctrl+F: Quick file attachment command
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            self.state.input.set_text("/attach ");
            return;
        }

        // Ctrl+A: Home (start of line)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('a') {
            self.state.input.move_home();
            return;
        }

        // Ctrl+E: End of line
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('e') {
            self.state.input.move_end();
            return;
        }

        // Ctrl+K: Delete to end of line
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.state.input.delete_to_end();
            return;
        }

        // Ctrl+U: Delete to start of line
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            self.state.input.delete_to_start();
            return;
        }

        // Ctrl+W: Delete word
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
            self.state.input.delete_prev_word();
            return;
        }

        // Ctrl shortcuts for conversation scrolling
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Home => {
                    self.state.conversation.scroll_to_top();
                    return;
                }
                KeyCode::End => {
                    self.state.conversation.scroll_to_bottom();
                    return;
                }
                KeyCode::Up => {
                    self.state.conversation.scroll_up(1);
                    return;
                }
                KeyCode::Down => {
                    self.state.conversation.scroll_down(1);
                    return;
                }
                _ => {}
            }
        }

        // Shift shortcuts for conversation scrolling
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Up => {
                    self.state.conversation.scroll_up(1);
                    return;
                }
                KeyCode::Down => {
                    self.state.conversation.scroll_down(1);
                    return;
                }
                KeyCode::PageUp => {
                    let page = self.state.conversation.last_viewport_height.get().max(4);
                    self.state.conversation.scroll_page_up(page);
                    return;
                }
                KeyCode::PageDown => {
                    let page = self.state.conversation.last_viewport_height.get().max(4);
                    self.state.conversation.scroll_page_down(page);
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                self.handle_submit().await;
            }
            KeyCode::Esc => {
                if self.state.show_activity_drawer {
                    self.state.show_activity_drawer = false;
                } else if !self.state.input.is_empty() {
                    self.state.input.clear();
                } else if !self.state.attachments.is_empty() {
                    self.state.attachments.clear();
                }
            }
            KeyCode::Tab => {
                self.state.show_activity_drawer = !self.state.show_activity_drawer;
            }
            KeyCode::Backspace => {
                self.state.input.backspace();
            }
            KeyCode::Delete => {
                self.state.input.delete();
            }
            KeyCode::Left => {
                self.state.input.move_left();
            }
            KeyCode::Right => {
                self.state.input.move_right();
            }
            KeyCode::Home => {
                if self.state.input.is_empty() {
                    self.state.conversation.scroll_to_top();
                } else {
                    self.state.input.move_home();
                }
            }
            KeyCode::End => {
                if self.state.input.is_empty() {
                    self.state.conversation.scroll_to_bottom();
                } else {
                    self.state.input.move_end();
                }
            }
            KeyCode::PageUp => {
                let page = self.state.conversation.last_viewport_height.get().max(4);
                self.state.conversation.scroll_page_up(page);
            }
            KeyCode::PageDown => {
                let page = self.state.conversation.last_viewport_height.get().max(4);
                self.state.conversation.scroll_page_down(page);
            }
            KeyCode::Up => {
                if !self.state.input.is_empty() {
                    self.state.input.history_prev();
                } else {
                    self.state.conversation.scroll_up(1);
                }
            }
            KeyCode::Down => {
                if !self.state.input.is_empty() {
                    self.state.input.history_next();
                } else {
                    self.state.conversation.scroll_down(1);
                }
            }
            KeyCode::Char(c) => {
                self.state.input.insert_char(c);
            }
            _ => {}
        }
    }

    async fn handle_submit(&mut self) {
        let raw = self.state.input.submit();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }

        // Intercept Slash Commands
        if let Some(cmd_result) = SlashCommand::parse(trimmed) {
            match cmd_result {
                Ok(SlashCommand::Help) => {
                    let help = SlashCommand::help_text();
                    self.state
                        .conversation
                        .add_message(ConversationMessage::new_crudo(help));
                }
                Ok(SlashCommand::Clear) => {
                    self.state.conversation.clear();
                }
                Ok(SlashCommand::Quit) => {
                    self.state.should_quit = true;
                }
                Ok(SlashCommand::Status) => {
                    self.state.show_activity_drawer = true;
                    let status_report = format!(
                        "System & Subsystem Status:\n\n  Backend:  {}\n  Model:    {}\n  MCP:      {}\n  Sandbox:  {}\n  Network:  OFF (Sovereign Air-gap Policy)\n  CPU:      {}\n  GPU:      {}\n\nType /help for available commands.",
                        self.state.backend.connection,
                        self.state.backend.model,
                        self.state.backend.mcp,
                        self.state.backend.sandbox,
                        self.state.system.formatted_cpu(),
                        self.state.system.formatted_gpu(),
                    );
                    self.state
                        .conversation
                        .add_message(ConversationMessage::new_crudo(status_report));
                }
                Ok(SlashCommand::Attach(path)) => match Attachment::from_path(&path) {
                    Ok(att) => {
                        let msg = format!(
                            "Attached file: {} ({}, {})\nFile is queued for next prompt.",
                            att.filename,
                            att.formatted_size(),
                            att.kind
                        );
                        self.state.attachments.add(att);
                        self.state
                            .conversation
                            .add_message(ConversationMessage::new_crudo(msg));
                    }
                    Err(e) => {
                        let msg = format!("Attachment failed: {e}");
                        self.state
                            .conversation
                            .add_message(ConversationMessage::new_crudo(msg));
                    }
                },
                Err(err_msg) => {
                    self.state
                        .conversation
                        .add_message(ConversationMessage::new_crudo(err_msg));
                }
            }
            return;
        }

        // Regular prompt submission
        let pending_attachments = self.state.attachments.clear();
        self.state
            .conversation
            .add_message(ConversationMessage::new_user(
                trimmed.to_string(),
                pending_attachments.clone(),
            ));

        // State transition upon submission:
        if !pending_attachments.is_empty() {
            self.state.conversation.activity = AgentActivity::LookingThroughAttachment;
            self.state.conversation.is_coding_task = false;
        } else if is_coding_request(trimmed) {
            self.state.conversation.is_coding_task = true;
            self.state.conversation.activity = AgentActivity::Thinking;
        } else {
            self.state.conversation.is_coding_task = false;
            self.state.conversation.activity = AgentActivity::Thinking;
        }

        // Dispatch to backend
        match self
            .backend
            .submit_message(trimmed, &pending_attachments)
            .await
        {
            Ok(()) => {
                // Backend accepted prompt; will stream or send CrudoEvent responses
            }
            Err(crate::backend::BackendError::NotConnected) => {
                self.state.conversation.activity = AgentActivity::Idle;
                self.state.conversation.is_coding_task = false;
                // Truthful message when backend is not connected
                self.state.conversation.add_message(ConversationMessage::new_crudo(
                    "CRUDO backend is not connected. Connect a backend service or configure an endpoint to process requests.".to_string(),
                ));
            }
            Err(e) => {
                self.state.conversation.activity = AgentActivity::Idle;
                self.state.conversation.is_coding_task = false;
                self.state
                    .conversation
                    .add_message(ConversationMessage::new_crudo(format!(
                        "Backend error: {e}"
                    )));
            }
        }
    }

    pub fn handle_backend_event(&mut self, event: CrudoEvent) {
        match event {
            CrudoEvent::ActivityChanged(activity) => {
                self.state.conversation.activity = activity;
            }
            CrudoEvent::MessageCreated {
                role,
                content,
                timestamp: _,
            } => {
                let msg = match role {
                    Actor::User => ConversationMessage::new_user(content, Vec::new()),
                    Actor::Crudo => {
                        self.state.conversation.activity = AgentActivity::Idle;
                        self.state.conversation.is_coding_task = false;
                        ConversationMessage::new_crudo(content)
                    }
                };
                self.state.conversation.add_message(msg);
            }
            CrudoEvent::ResponseStarted { message_id } => {
                if self.state.conversation.is_coding_task {
                    self.state.conversation.activity = AgentActivity::WritingCode;
                }
                self.state
                    .conversation
                    .add_message(ConversationMessage::new_streaming(message_id));
            }
            CrudoEvent::ResponseDelta { message_id, delta } => {
                if self.state.conversation.is_coding_task {
                    self.state.conversation.activity = AgentActivity::WritingCode;
                }
                if let Some(msg) = self
                    .state
                    .conversation
                    .messages
                    .iter_mut()
                    .find(|m| m.id == message_id)
                {
                    msg.content.push_str(&delta);
                }
            }
            CrudoEvent::ResponseCompleted { message_id } => {
                self.state.conversation.activity = AgentActivity::Idle;
                self.state.conversation.is_coding_task = false;
                if let Some(msg) = self
                    .state
                    .conversation
                    .messages
                    .iter_mut()
                    .find(|m| m.id == message_id)
                {
                    msg.is_streaming = false;
                }
            }
            CrudoEvent::ResponseFailed { message_id, error } => {
                self.state.conversation.activity = AgentActivity::Idle;
                self.state.conversation.is_coding_task = false;
                if let Some(msg) = self
                    .state
                    .conversation
                    .messages
                    .iter_mut()
                    .find(|m| m.id == message_id)
                {
                    msg.is_streaming = false;
                    msg.content.push_str(&format!("\n[Error: {error}]"));
                }
            }
            CrudoEvent::DocumentStarted {
                document_id,
                filename,
            } => {
                self.state.conversation.activity = AgentActivity::LookingThroughAttachment;
                self.state.conversation.active_document =
                    Some(DocumentProgressState::new(document_id, filename.clone()));
                self.state
                    .log_activity("DOC", format!("Reading {filename}"), "RUNNING", None);
            }
            CrudoEvent::DocumentProgress {
                document_id: _,
                filename: _,
                stage,
                current,
                total,
                percentage,
            } => {
                self.state.conversation.activity = AgentActivity::LookingThroughAttachment;
                if let Some(ref mut doc) = self.state.conversation.active_document {
                    doc.stage = stage;
                    doc.current = current;
                    doc.total = total;
                    doc.percentage = percentage;
                }
            }
            CrudoEvent::DocumentCompleted {
                document_id: _,
                filename,
            } => {
                self.state.conversation.active_document = None;
                // Transition to Thinking state as per Section 14 & 15:
                self.state.conversation.activity = AgentActivity::Thinking;
                self.state
                    .log_activity("DOC", format!("Parsed {filename}"), "COMPLETED", None);
            }
            CrudoEvent::DocumentFailed {
                document_id: _,
                filename,
                error,
            } => {
                self.state.conversation.active_document = None;
                self.state.conversation.activity = AgentActivity::Idle;
                self.state.log_activity(
                    "DOC",
                    format!("Parsing failed {filename}"),
                    "FAILED",
                    Some(error),
                );
            }
            CrudoEvent::VisionStarted {
                image_id: _,
                filename,
            } => {
                self.state.conversation.activity = AgentActivity::LookingThroughAttachment;
                self.state.log_activity(
                    "VISION",
                    format!("Processing {filename}"),
                    "RUNNING",
                    None,
                );
            }
            CrudoEvent::VisionProgress {
                image_id: _,
                filename,
                stage,
                percentage,
            } => {
                self.state.conversation.activity = AgentActivity::LookingThroughAttachment;
                self.state.log_activity(
                    "VISION",
                    format!("{filename}: {stage} ({percentage}%)"),
                    "RUNNING",
                    None,
                );
            }
            CrudoEvent::VisionCompleted {
                image_id: _,
                filename,
            } => {
                self.state.conversation.activity = AgentActivity::Thinking;
                self.state.log_activity(
                    "VISION",
                    format!("Inspected {filename}"),
                    "COMPLETED",
                    None,
                );
            }
            CrudoEvent::VisionFailed {
                image_id: _,
                filename,
                error,
            } => {
                self.state.conversation.activity = AgentActivity::Idle;
                self.state.log_activity(
                    "VISION",
                    format!("Failed {filename}"),
                    "FAILED",
                    Some(error),
                );
            }
            CrudoEvent::AgentActivity(agent_event) => match agent_event {
                AgentEvent::CodingStarted => {
                    self.state.conversation.activity = AgentActivity::WritingCode;
                    self.state
                        .log_activity("CODE", "Generating code".to_string(), "RUNNING", None);
                }
                AgentEvent::ModelStarted { model } => {
                    if !self.state.conversation.is_coding_task
                        || self.state.conversation.activity == AgentActivity::Idle
                    {
                        self.state.conversation.activity = AgentActivity::Thinking;
                    }
                    self.state.log_activity(
                        "MODEL",
                        format!("Reasoning with {model}"),
                        "RUNNING",
                        None,
                    );
                }
                AgentEvent::ModelProgress { model, tokens } => {
                    self.state.log_activity(
                        "MODEL",
                        format!("{model}: {tokens} tokens"),
                        "RUNNING",
                        None,
                    );
                }
                AgentEvent::ModelCompleted { model } => {
                    self.state.log_activity(
                        "MODEL",
                        format!("Finished {model}"),
                        "COMPLETED",
                        None,
                    );
                }
                AgentEvent::ModelFailed { model, error } => {
                    self.state.conversation.activity = AgentActivity::Idle;
                    self.state.log_activity(
                        "MODEL",
                        format!("Failed {model}"),
                        "FAILED",
                        Some(error),
                    );
                }
                AgentEvent::McpStarted { server, method } => {
                    self.state
                        .log_activity("MCP", format!("{server}.{method}"), "RUNNING", None);
                }
                AgentEvent::McpCompleted {
                    server,
                    method,
                    summary,
                } => {
                    self.state.log_activity(
                        "MCP",
                        format!("{server}.{method}"),
                        "COMPLETED",
                        summary,
                    );
                }
                AgentEvent::McpFailed {
                    server,
                    method,
                    error,
                } => {
                    self.state.log_activity(
                        "MCP",
                        format!("{server}.{method}"),
                        "FAILED",
                        Some(error),
                    );
                }
                AgentEvent::ToolStarted { tool_name } => {
                    if self.state.conversation.is_coding_task
                        || tool_name.contains("code")
                        || tool_name.contains("writer")
                    {
                        self.state.conversation.activity = AgentActivity::WritingCode;
                    }
                    self.state.log_activity("TOOL", tool_name, "RUNNING", None);
                }
                AgentEvent::ToolProgress { tool_name, stage } => {
                    self.state
                        .log_activity("TOOL", tool_name, "RUNNING", Some(stage));
                }
                AgentEvent::ToolCompleted { tool_name } => {
                    self.state
                        .log_activity("TOOL", tool_name, "COMPLETED", None);
                }
                AgentEvent::ToolFailed { tool_name, error } => {
                    self.state
                        .log_activity("TOOL", tool_name, "FAILED", Some(error));
                }
                AgentEvent::SandboxStarted { task_id } => {
                    if self.state.conversation.is_coding_task {
                        self.state.conversation.activity = AgentActivity::WritingCode;
                    }
                    self.state
                        .log_activity("SANDBOX", format!("Task {task_id}"), "RUNNING", None);
                }
                AgentEvent::SandboxCompleted { task_id } => {
                    self.state.log_activity(
                        "SANDBOX",
                        format!("Task {task_id}"),
                        "COMPLETED",
                        None,
                    );
                }
                AgentEvent::SandboxFailed { task_id, error } => {
                    self.state.log_activity(
                        "SANDBOX",
                        format!("Task {task_id}"),
                        "FAILED",
                        Some(error),
                    );
                }
                AgentEvent::ArtifactCreated { title, path } => {
                    self.state
                        .log_activity("ARTIFACT", title, "COMPLETED", path);
                }
                AgentEvent::FileCreated { path, size_bytes } => {
                    self.state.log_activity(
                        "FILE",
                        format!("{path} ({size_bytes} B)"),
                        "COMPLETED",
                        None,
                    );
                }
                AgentEvent::SystemNotification(note) => {
                    self.state.log_activity("SYSTEM", note, "INFO", None);
                }
            },
            CrudoEvent::BackendStatusChanged(status) => {
                self.state.backend.connection = status;
            }
            CrudoEvent::ModelStatusChanged(status) => {
                self.state.backend.model = status;
            }
            CrudoEvent::McpStatusChanged(status) => {
                self.state.backend.mcp = status;
            }
            CrudoEvent::SandboxStatusChanged(status) => {
                self.state.backend.sandbox = status;
            }
        }
    }
}

/// Determines if a user prompt is requesting code generation or implementation.
pub fn is_coding_request(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    let lower_trim = lower.trim();

    if lower_trim.starts_with("def ")
        || lower_trim.starts_with("fn ")
        || lower_trim.starts_with("class ")
    {
        return true;
    }

    let code_terms = [
        "code",
        "program",
        "script",
        "function",
        "algorithm",
        "python",
        "rust",
        "c++",
        "javascript",
        "bash",
        "class",
        "method",
        "module",
        "sql",
    ];

    let action_terms = [
        "write",
        "implement",
        "code",
        "generate",
        "create",
        "build",
        "develop",
        "draft",
    ];

    for action in action_terms {
        if lower.contains(action) {
            for term in code_terms {
                if lower.contains(term) {
                    return true;
                }
            }
        }
    }

    lower.contains("coding") || lower.contains("write code") || lower.contains("implement")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_coding_request_detection() {
        assert!(is_coding_request(
            "Write a Python program that calculates fibonacci"
        ));
        assert!(is_coding_request("Write code for a quicksort function"));
        assert!(is_coding_request("Implement a thread-safe cache in Rust"));
        assert!(is_coding_request(
            "Can you write a script to process sensor logs?"
        ));
        assert!(is_coding_request("def calculate_pressure(p, v):"));

        assert!(!is_coding_request("What is a P&ID?"));
        assert!(!is_coding_request("Explain distillation columns."));
        assert!(!is_coding_request("Hello CRUDO!"));
    }
}
