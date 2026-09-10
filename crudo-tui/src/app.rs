use crate::activity::{Activity, ActivityStatus};
use crate::command::AppCommand;
use crate::history::InputHistory;
use crate::input::InputState;
use crate::message::{Message, Role};
use crate::palette::{default_commands, CommandPaletteState};
use crate::scroll::ScrollState;
use crate::spinner::Spinner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Input,
    Chat,
    Activity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Thinking,
    ExecutingTool,
    Streaming,
    Cancelling,
    Error,
    Offline,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::Idle
    }
}

pub enum UserEvent {
    SendMessage(String),
    CancelAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionChoice {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub struct PendingPermission {
    pub id: usize,
    pub tool_name: String,
    pub summary: String,
    pub current_mode: String,
    pub reason: Option<String>,
    pub choice: PermissionChoice,
    pub responder: crate::agent::events::PermissionResponder,
}

pub struct App {
    pub should_quit: bool,
    pub input_state: InputState,
    pub input_history: InputHistory,
    pub focus: Focus,
    pub chat_scroll: ScrollState,
    pub activity_scroll: ScrollState,
    pub events: Vec<UserEvent>,
    pub messages: Vec<Message>,
    pub agent_state: AgentState,
    pub spinner: Spinner,
    pub activities: Vec<Activity>,
    pub max_activities: usize,
    pub show_activity_panel: bool,
    pub palette_state: CommandPaletteState,
    pub pending_permission: Option<PendingPermission>,
    pub active_run_id: Option<usize>,
    pub cancelled_run_id: Option<usize>,
    next_id: usize,
    next_activity_id: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            input_state: InputState::new(),
            input_history: InputHistory::new(),
            focus: Focus::Input,
            chat_scroll: ScrollState::new(),
            activity_scroll: ScrollState::new(),
            events: Vec::new(),
            messages: Vec::new(),
            agent_state: AgentState::Idle,
            spinner: Spinner::default_dots(),
            activities: Vec::new(),
            max_activities: crate::activity::DEFAULT_MAX_ACTIVITIES,
            show_activity_panel: false,
            palette_state: CommandPaletteState::new(),
            pending_permission: None,
            active_run_id: None,
            cancelled_run_id: None,
            next_id: 1,
            next_activity_id: 1,
        }
    }

    pub fn set_active_run_id(&mut self, run_id: Option<usize>) {
        self.active_run_id = run_id;
        if run_id.is_some() {
            self.cancelled_run_id = None;
        }
    }

    pub fn cancel_active_run(&mut self) {
        if let Some(perm) = self.pending_permission.take() {
            perm.responder
                .respond(runtime::PermissionPromptDecision::Deny {
                    reason: "Agent run was cancelled".to_string(),
                });
        }
        for act in &mut self.activities {
            if act.status == ActivityStatus::Running {
                act.status = ActivityStatus::Cancelled;
            }
        }
        if let Some(last_msg) = self.messages.last_mut() {
            if last_msg.role == Role::Assistant
                && self.agent_state == AgentState::Streaming
                && !last_msg.content.ends_with("[Cancelled]")
            {
                last_msg.content.push_str("\n\n[Cancelled]");
            }
        }
        self.set_agent_state(AgentState::Idle);
        self.cancelled_run_id = self.active_run_id.take();
        self.events.push(UserEvent::CancelAgent);
    }

    pub fn toggle_activity_panel(&mut self) {
        self.show_activity_panel = !self.show_activity_panel;
        if !self.show_activity_panel && self.focus == Focus::Activity {
            self.focus = Focus::Input;
        }
    }

    pub fn set_agent_state(&mut self, state: AgentState) {
        self.agent_state = state;
    }

    pub fn tick(&mut self) {
        self.spinner.tick();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn is_agent_active(&self) -> bool {
        self.agent_state != AgentState::Idle
            && self.agent_state != AgentState::Offline
            && self.agent_state != AgentState::Error
    }

    pub fn return_to_idle(&mut self) {
        if self.agent_state == AgentState::Error || self.agent_state == AgentState::Cancelling {
            self.set_agent_state(AgentState::Idle);
        }
    }

    pub fn handle_command(&mut self, cmd: AppCommand) {
        match cmd {
            AppCommand::Quit => self.quit(),
            AppCommand::Escape => {
                self.return_to_idle();
            }
            AppCommand::ToggleActivity => self.toggle_activity_panel(),
            AppCommand::FocusNext => {
                self.focus = match self.focus {
                    Focus::Input => Focus::Chat,
                    Focus::Chat => {
                        if self.show_activity_panel {
                            Focus::Activity
                        } else {
                            Focus::Input
                        }
                    }
                    Focus::Activity => Focus::Input,
                };
            }
            AppCommand::FocusPrevious => {
                self.focus = match self.focus {
                    Focus::Input => {
                        if self.show_activity_panel {
                            Focus::Activity
                        } else {
                            Focus::Chat
                        }
                    }
                    Focus::Chat => Focus::Input,
                    Focus::Activity => Focus::Chat,
                };
            }
            AppCommand::PageUp => match self.focus {
                Focus::Activity => self.activity_scroll.scroll_by_page(-1),
                _ => self.chat_scroll.scroll_by_page(-1),
            },
            AppCommand::PageDown => match self.focus {
                Focus::Activity => self.activity_scroll.scroll_by_page(1),
                _ => self.chat_scroll.scroll_by_page(1),
            },
            AppCommand::ScrollUp => match self.focus {
                Focus::Activity => self.activity_scroll.scroll_up(1),
                _ => self.chat_scroll.scroll_up(1),
            },
            AppCommand::ScrollDown => match self.focus {
                Focus::Activity => self.activity_scroll.scroll_down(1),
                _ => self.chat_scroll.scroll_down(1),
            },
            AppCommand::ScrollToTop => match self.focus {
                Focus::Activity => self.activity_scroll.scroll_to_top(),
                _ => self.chat_scroll.scroll_to_top(),
            },
            AppCommand::ScrollToBottom => match self.focus {
                Focus::Activity => self.activity_scroll.scroll_to_bottom(),
                _ => self.chat_scroll.scroll_to_bottom(),
            },
            AppCommand::Submit => {
                let value = self.input_state.value().trim().to_string();
                if !value.is_empty() {
                    self.return_to_idle();
                    self.input_history.push(value.clone());
                    self.events.push(UserEvent::SendMessage(value));
                    self.input_state.clear();
                }
            }
            AppCommand::InsertChar(c) => {
                self.input_state.insert_char(c);
            }
            AppCommand::Paste(text) => {
                if self.focus == Focus::Input
                    && !self.is_agent_active()
                    && self.active_run_id.is_none()
                    && self.pending_permission.is_none()
                    && !self.palette_state.is_open
                {
                    self.input_state.insert_str(&text);
                }
            }
            AppCommand::CursorLeft => {
                self.input_state.move_cursor_left();
            }
            AppCommand::CursorRight => {
                self.input_state.move_cursor_right();
            }
            AppCommand::CursorHome => {
                self.input_state.move_cursor_home();
            }
            AppCommand::CursorEnd => {
                self.input_state.move_cursor_end();
            }
            AppCommand::DeleteBackward => {
                self.input_state.delete_backward();
            }
            AppCommand::DeleteForward => {
                self.input_state.delete_forward();
            }
            AppCommand::ClearInput => {
                self.input_state.clear();
            }
            AppCommand::InputHistoryPrevious => {
                if let Some(entry) = self.input_history.previous(self.input_state.value()) {
                    let entry_str = entry.to_string();
                    self.input_state.set_value(entry_str);
                }
            }
            AppCommand::InputHistoryNext => {
                if let Some(entry) = self.input_history.next() {
                    let entry_str = entry.to_string();
                    self.input_state.set_value(entry_str);
                }
            }
            AppCommand::OpenCommandPalette => self.palette_state.open(),
            AppCommand::ToggleCommandPalette => self.palette_state.toggle(),
            AppCommand::CloseCommandPalette => self.palette_state.close(),
            AppCommand::PalettePrevious => {
                let count = self
                    .palette_state
                    .filtered_commands(&default_commands())
                    .len();
                self.palette_state.select_previous(count);
            }
            AppCommand::PaletteNext => {
                let count = self
                    .palette_state
                    .filtered_commands(&default_commands())
                    .len();
                self.palette_state.select_next(count);
            }
            AppCommand::PaletteInsertChar(c) => {
                self.palette_state.insert_char(c);
            }
            AppCommand::PaletteDeleteBackward => {
                self.palette_state.delete_backward();
            }
            AppCommand::CancelAgent => {
                if self.is_agent_active()
                    || self.pending_permission.is_some()
                    || self.active_run_id.is_some()
                {
                    self.cancel_active_run();
                }
            }
            AppCommand::PaletteSelect => {
                let commands = default_commands();
                if let Some(cmd) = self.palette_state.selected_command(&commands) {
                    let action = cmd.action.clone();
                    self.palette_state.close();
                    self.handle_command(action);
                }
            }
            AppCommand::PermissionToggleChoice => {
                if let Some(perm) = &mut self.pending_permission {
                    perm.choice = match perm.choice {
                        PermissionChoice::Allow => PermissionChoice::Deny,
                        PermissionChoice::Deny => PermissionChoice::Allow,
                    };
                }
            }
            AppCommand::PermissionSelectAllow => {
                if let Some(perm) = self.pending_permission.take() {
                    perm.responder
                        .respond(runtime::PermissionPromptDecision::Allow);
                }
            }
            AppCommand::PermissionSelectDeny => {
                if let Some(perm) = self.pending_permission.take() {
                    perm.responder
                        .respond(runtime::PermissionPromptDecision::Deny {
                            reason: "User denied permission".to_string(),
                        });
                }
            }
            AppCommand::PermissionConfirm => {
                if let Some(perm) = self.pending_permission.take() {
                    let decision = match perm.choice {
                        PermissionChoice::Allow => runtime::PermissionPromptDecision::Allow,
                        PermissionChoice::Deny => runtime::PermissionPromptDecision::Deny {
                            reason: "User denied permission".to_string(),
                        },
                    };
                    perm.responder.respond(decision);
                }
            }
        }
    }

    pub fn add_message(&mut self, role: Role, content: String) {
        let msg = Message::new(self.next_id, role, content);
        self.next_id += 1;
        self.messages.push(msg);
        self.chat_scroll.notify_new_item();
    }

    pub fn start_activity(&mut self, tool: String, summary: String) -> usize {
        let id = self.next_activity_id;
        self.next_activity_id += 1;
        let mut activity = Activity::new(id, tool, summary);
        activity.status = ActivityStatus::Running;

        // Bounded activity history to prevent unbounded memory growth
        if self.max_activities > 0 && self.activities.len() >= self.max_activities {
            let remove_count = self.activities.len() - self.max_activities + 1;
            self.activities.drain(0..remove_count);
        }

        self.activities.push(activity);
        self.activity_scroll.notify_new_item();
        id
    }

    fn find_activity_index(&self, id: usize) -> Option<usize> {
        // 1. Exact match by ID
        if let Some(pos) = self.activities.iter().rposition(|a| a.id == id) {
            return Some(pos);
        }
        // 2. Fall back to the currently running activity if ID didn't match any known activity
        self.activities
            .iter()
            .rposition(|a| a.status == ActivityStatus::Running)
    }

    #[allow(dead_code)]
    pub fn update_activity(&mut self, id: usize, summary: String) {
        if let Some(idx) = self.find_activity_index(id) {
            self.activities[idx].summary = summary;
        }
    }

    pub fn update_activity_progress(&mut self, id: usize, output: &str) {
        if let Some(idx) = self.find_activity_index(id) {
            self.activities[idx].update_progress(output);
        }
    }

    pub fn finish_activity(&mut self, id: usize, duration_ms: Option<u64>) {
        if let Some(idx) = self.find_activity_index(id) {
            let activity = &mut self.activities[idx];
            if activity.status == ActivityStatus::Cancelled {
                return;
            }
            if activity.error.is_some() {
                activity.status = ActivityStatus::Failed;
            } else {
                activity.status = ActivityStatus::Completed;
            }
            activity.duration_ms = duration_ms;
            activity.progress = None;
        }
    }

    #[allow(dead_code)]
    pub fn fail_activity(&mut self, id: usize, duration_ms: Option<u64>) {
        if let Some(idx) = self.find_activity_index(id) {
            let activity = &mut self.activities[idx];
            if activity.status == ActivityStatus::Cancelled {
                return;
            }
            activity.status = ActivityStatus::Failed;
            activity.duration_ms = duration_ms;
            activity.progress = None;
        }
    }

    #[allow(dead_code)]
    pub fn cancel_activity(&mut self, id: usize, duration_ms: Option<u64>) {
        if let Some(idx) = self.find_activity_index(id) {
            let activity = &mut self.activities[idx];
            activity.status = ActivityStatus::Cancelled;
            activity.duration_ms = duration_ms;
            activity.progress = None;
        }
    }

    pub fn handle_run_event(&mut self, run_id: usize, event: crate::agent::events::AgentEvent) {
        use crate::agent::events::AgentEvent;

        // STALE EVENT RULE:
        // At the UI boundary:
        //     event.run_id != active_run_id
        // must cause the event to be ignored.
        // This filtering must happen BEFORE the event can mutate:
        // - current assistant message
        // - streaming text
        // - thinking state
        // - activity state
        // - app status
        // - completion state
        // - error state
        if self.active_run_id != Some(run_id) {
            // When Run A is cancelled:
            // Run A may emit exactly one Cancelled event.
            // After that, all further Run A events must be ignored.
            // Run B must never consume Run A's events.
            if self.cancelled_run_id == Some(run_id) && matches!(event, AgentEvent::Cancelled) {
                self.cancelled_run_id = None;
                self.pending_permission = None;
                for act in &mut self.activities {
                    if act.status == ActivityStatus::Running {
                        act.status = ActivityStatus::Cancelled;
                    }
                }
                if let Some(last_msg) = self.messages.last_mut() {
                    if last_msg.role == Role::Assistant
                        && !last_msg.content.ends_with("[Cancelled]")
                    {
                        last_msg.content.push_str("\n\n[Cancelled]");
                    }
                }
                self.set_agent_state(AgentState::Idle);
            }
            return;
        }

        match event {
            AgentEvent::Started => {
                self.set_agent_state(AgentState::Thinking);
            }
            AgentEvent::Thinking => {
                self.set_agent_state(AgentState::Thinking);
            }
            AgentEvent::ToolStarted { tool, summary } => {
                self.start_activity(tool, summary);
                self.set_agent_state(AgentState::ExecutingTool);
            }
            AgentEvent::ToolOutput { id, output } => {
                self.update_activity_progress(id, &output);
            }
            AgentEvent::ToolFinished { id, duration_ms } => {
                self.finish_activity(id, Some(duration_ms));
            }
            AgentEvent::TextChunk(chunk) => {
                self.set_agent_state(AgentState::Streaming);
                self.add_assistant_chunk(chunk);
            }
            AgentEvent::PermissionRequested {
                id,
                tool_name,
                summary,
                current_mode,
                reason,
                responder,
            } => {
                self.pending_permission = Some(PendingPermission {
                    id,
                    tool_name,
                    summary,
                    current_mode,
                    reason,
                    choice: PermissionChoice::Allow,
                    responder,
                });
            }
            AgentEvent::PermissionResolved { id, allowed: _ } => {
                if self.pending_permission.as_ref().map(|p| p.id) == Some(id) {
                    self.pending_permission = None;
                }
            }
            AgentEvent::Completed => {
                self.active_run_id = None;
                self.pending_permission = None;
                // Ensure no activities remain stuck as Running
                for act in &mut self.activities {
                    if act.status == ActivityStatus::Running {
                        act.status = ActivityStatus::Completed;
                        act.progress = None;
                    }
                }
                self.set_agent_state(AgentState::Idle);
            }
            AgentEvent::Error(err) => {
                self.active_run_id = None;
                self.pending_permission = None;
                // Mark any currently running activities as Failed
                for act in &mut self.activities {
                    if act.status == ActivityStatus::Running {
                        act.status = ActivityStatus::Failed;
                        if !err.is_empty() {
                            act.error = Some(err.clone());
                        }
                    }
                }
                let err_text = if err.trim().is_empty() {
                    "An error occurred while processing the request.".to_string()
                } else {
                    err.clone()
                };
                if let Some(last_msg) = self.messages.last_mut() {
                    if last_msg.role == Role::Assistant {
                        if !last_msg.content.contains("[Error") {
                            if last_msg.content.trim().is_empty() {
                                last_msg.content = format!("[Error: {}]", err_text);
                            } else {
                                last_msg
                                    .content
                                    .push_str(&format!("\n\n[Error: {}]", err_text));
                            }
                        }
                    } else {
                        self.add_message(Role::Assistant, format!("[Error: {}]", err_text));
                    }
                } else {
                    self.add_message(Role::Assistant, format!("[Error: {}]", err_text));
                }
                self.set_agent_state(AgentState::Error);
            }
            AgentEvent::Cancelled => {
                self.active_run_id = None;
                self.pending_permission = None;
                for act in &mut self.activities {
                    if act.status == ActivityStatus::Running {
                        act.status = ActivityStatus::Cancelled;
                    }
                }
                if let Some(last_msg) = self.messages.last_mut() {
                    if last_msg.role == Role::Assistant
                        && !last_msg.content.ends_with("[Cancelled]")
                    {
                        last_msg.content.push_str("\n\n[Cancelled]");
                    }
                }
                self.set_agent_state(AgentState::Idle);
            }
        }
    }

    #[allow(dead_code)]
    pub fn handle_agent_event(&mut self, event: crate::agent::events::AgentEvent) {
        let run_id = self.active_run_id.unwrap_or(0);
        if self.active_run_id.is_none() && self.cancelled_run_id.is_none() {
            self.active_run_id = Some(run_id);
        }
        self.handle_run_event(run_id, event);
    }

    pub fn add_assistant_chunk(&mut self, text: String) {
        if let Some(last_msg) = self.messages.last_mut() {
            if last_msg.role == Role::Assistant {
                // If message was marked cancelled, do not append further chunks
                if last_msg.content.ends_with("[Cancelled]") {
                    return;
                }
                last_msg.content.push_str(&text);
                return;
            }
        }

        self.add_message(Role::Assistant, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Role;

    #[test]
    fn test_empty_conversation() {
        let app = App::new();
        assert!(app.messages.is_empty());
    }

    #[test]
    fn test_add_user_message() {
        let mut app = App::new();
        app.add_message(Role::User, "Hello".to_string());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, Role::User);
        assert_eq!(app.messages[0].content, "Hello");
    }

    #[test]
    fn test_add_assistant_message() {
        let mut app = App::new();
        app.add_message(Role::Assistant, "I can help".to_string());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, Role::Assistant);
        assert_eq!(app.messages[0].content, "I can help");
    }

    #[test]
    fn test_add_system_message() {
        let mut app = App::new();
        app.add_message(Role::System, "System booted".to_string());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, Role::System);
        assert_eq!(app.messages[0].content, "System booted");
    }

    #[test]
    fn test_multiple_messages_and_ordering() {
        let mut app = App::new();
        app.add_message(Role::User, "Hello".to_string());
        app.add_message(Role::Assistant, "Hi there!".to_string());
        app.add_message(Role::User, "How are you?".to_string());

        assert_eq!(app.messages.len(), 3);
        assert_eq!(app.messages[0].id, 1);
        assert_eq!(app.messages[0].role, Role::User);
        assert_eq!(app.messages[1].id, 2);
        assert_eq!(app.messages[1].role, Role::Assistant);
        assert_eq!(app.messages[2].id, 3);
        assert_eq!(app.messages[2].role, Role::User);
    }

    #[test]
    fn test_message_history_not_deleted_when_many_messages_added() {
        let mut app = App::new();
        for i in 0..50 {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            app.add_message(role, format!("Message number {}", i));
        }
        assert_eq!(app.messages.len(), 50);
        for i in 0..50 {
            assert_eq!(app.messages[i].content, format!("Message number {}", i));
        }
    }

    #[test]
    fn test_unicode_message_content() {
        let mut app = App::new();
        app.add_message(Role::User, "こんにちは 😀 é".to_string());
        assert_eq!(app.messages[0].content, "こんにちは 😀 é");
    }

    #[test]
    fn test_default_state() {
        let app = App::new();
        assert_eq!(app.agent_state, AgentState::Idle);
    }

    #[test]
    fn test_state_transitions() {
        let mut app = App::new();
        app.set_agent_state(AgentState::Thinking);
        assert_eq!(app.agent_state, AgentState::Thinking);
        app.set_agent_state(AgentState::ExecutingTool);
        assert_eq!(app.agent_state, AgentState::ExecutingTool);
        app.set_agent_state(AgentState::Streaming);
        assert_eq!(app.agent_state, AgentState::Streaming);
        app.set_agent_state(AgentState::Cancelling);
        assert_eq!(app.agent_state, AgentState::Cancelling);
        app.set_agent_state(AgentState::Error);
        assert_eq!(app.agent_state, AgentState::Error);
        app.set_agent_state(AgentState::Offline);
        assert_eq!(app.agent_state, AgentState::Offline);
    }

    #[test]
    fn test_spinner_tick() {
        let mut app = App::new();
        let frame1 = app.spinner.frame();
        app.tick();
        let frame2 = app.spinner.frame();
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn test_activity_lifecycle() {
        let mut app = App::new();
        let id1 = app.start_activity("Read file".to_string(), "src/main.rs".to_string());
        assert_eq!(app.activities.len(), 1);
        assert_eq!(app.activities[0].id, id1);
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.update_activity(id1, "src/lib.rs".to_string());
        assert_eq!(app.activities[0].summary, "src/lib.rs");

        app.finish_activity(id1, Some(100));
        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
        assert_eq!(app.activities[0].duration_ms, Some(100));

        let id2 = app.start_activity("Search".to_string(), "auth".to_string());
        app.fail_activity(id2, Some(50));
        assert_eq!(app.activities[1].status, ActivityStatus::Failed);

        let id3 = app.start_activity("Cancel me".to_string(), "please".to_string());
        app.cancel_activity(id3, None);
        assert_eq!(app.activities[2].status, ActivityStatus::Cancelled);

        assert_eq!(app.activities.len(), 3);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_agent_started() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::Started);
        assert_eq!(app.agent_state, AgentState::Thinking);
    }

    #[test]
    fn test_agent_thinking() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::Thinking);
        assert_eq!(app.agent_state, AgentState::Thinking);
    }

    #[test]
    fn test_agent_tool_started_and_finished() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "TestTool".to_string(),
            summary: "Testing".to_string(),
        });
        assert_eq!(app.agent_state, AgentState::ExecutingTool);
        assert_eq!(app.activities.len(), 1);
        assert_eq!(app.activities[0].tool, "TestTool");
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        let activity_id = app.activities[0].id;
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id: activity_id,
            duration_ms: 500,
        });

        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
        assert_eq!(app.activities[0].duration_ms, Some(500));
    }

    #[test]
    fn test_agent_text_chunk_updates_one_message() {
        let mut app = App::new();

        app.handle_agent_event(crate::agent::events::AgentEvent::TextChunk(
            "Hello".to_string(),
        ));
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Hello");
        assert_eq!(app.messages[0].role, Role::Assistant);

        app.handle_agent_event(crate::agent::events::AgentEvent::TextChunk(
            " World".to_string(),
        ));
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.messages.len(), 1); // Should still be 1
        assert_eq!(app.messages[0].content, "Hello World");
    }

    #[test]
    fn test_agent_completed() {
        let mut app = App::new();
        app.set_agent_state(AgentState::Streaming);
        app.handle_agent_event(crate::agent::events::AgentEvent::Completed);
        assert_eq!(app.agent_state, AgentState::Idle); // This represents READY in our mapping
    }

    #[test]
    fn test_agent_error() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::Error("fail".to_string()));
        assert_eq!(app.agent_state, AgentState::Error);
    }
    #[test]
    fn test_activity_panel_hidden_by_default() {
        let app = App::new();
        assert!(!app.show_activity_panel);
    }

    #[test]
    fn test_toggle_activity_panel() {
        let mut app = App::new();
        assert!(!app.show_activity_panel);

        app.toggle_activity_panel();
        assert!(app.show_activity_panel);

        app.toggle_activity_panel();
        assert!(!app.show_activity_panel);
    }

    #[test]
    fn test_activity_data_remains_intact_when_hidden() {
        let mut app = App::new();
        // Hidden by default
        assert!(!app.show_activity_panel);

        app.start_activity("Test".to_string(), "Activity".to_string());
        assert_eq!(app.activities.len(), 1);

        app.toggle_activity_panel();
        assert!(app.show_activity_panel);
        assert_eq!(app.activities.len(), 1); // Still there
    }

    #[test]
    fn test_complete_lifecycle_and_scrolling_flow() {
        let mut app = App::new();

        // 1. STARTUP: Conversation is empty
        assert!(app.messages.is_empty());
        assert_eq!(app.agent_state, AgentState::Idle);
        assert_eq!(app.chat_scroll.offset, 0);

        // 2. User types message & submits
        app.add_message(Role::User, "Hello CRUDO".to_string());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "Hello CRUDO");

        // 3. Mock Agent runs through thinking, tool execution, and streaming
        app.handle_agent_event(crate::agent::events::AgentEvent::Started);
        assert_eq!(app.agent_state, AgentState::Thinking);

        app.handle_agent_event(crate::agent::events::AgentEvent::Thinking);
        assert_eq!(app.agent_state, AgentState::Thinking);

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "Read".to_string(),
            summary: "file".to_string(),
        });
        assert_eq!(app.agent_state, AgentState::ExecutingTool);

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id: 1,
            duration_ms: 100,
        });

        // 4. Assistant response streams
        app.handle_agent_event(crate::agent::events::AgentEvent::TextChunk(
            "Hello! ".to_string(),
        ));
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[1].content, "Hello! ");

        app.handle_agent_event(crate::agent::events::AgentEvent::TextChunk(
            "I am CRUDO.".to_string(),
        ));
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[1].content, "Hello! I am CRUDO.");

        // 5. READY
        app.handle_agent_event(crate::agent::events::AgentEvent::Completed);
        assert_eq!(app.agent_state, AgentState::Idle);

        // 6. Additional messages can be added exceeding the viewport
        for i in 1..=20 {
            app.add_message(Role::User, format!("Question {}", i));
            app.add_message(Role::Assistant, format!("Answer {}", i));
        }
        assert_eq!(app.messages.len(), 42);

        // Viewport height of 15 lines with content of 150 lines
        let chat_height = 15;
        let content_lines = 150;
        let max_offset = content_lines - chat_height; // 135

        app.chat_scroll.set_max_offset(max_offset);
        assert_eq!(app.chat_scroll.offset, 135);
        assert!(app.chat_scroll.auto_scroll);

        // 7. Scroll upward -> older messages become visible
        app.chat_scroll.scroll_up(10);
        assert_eq!(app.chat_scroll.offset, 125);
        assert!(!app.chat_scroll.auto_scroll);

        // 8. Continue scrolling upward until reaching oldest message (offset = 0)
        app.chat_scroll.home();
        assert_eq!(app.chat_scroll.offset, 0);
        assert!(!app.chat_scroll.auto_scroll);

        // Cannot become negative
        app.chat_scroll.scroll_up(5);
        assert_eq!(app.chat_scroll.offset, 0);

        // 9. Adding a message while scrolled up preserves user scroll position
        app.add_message(Role::Assistant, "New background message".to_string());
        app.chat_scroll.set_max_offset(max_offset + 5);
        assert_eq!(app.chat_scroll.offset, 0);
        assert_eq!(app.chat_scroll.unseen_items, 1);

        // 10. Scroll downward -> newer messages become visible again
        app.chat_scroll.scroll_down(50);
        assert_eq!(app.chat_scroll.offset, 50);

        // Continue downward to bottom -> auto_scroll restored, unseen cleared
        app.chat_scroll.end();
        assert_eq!(app.chat_scroll.offset, max_offset + 5);
        assert!(app.chat_scroll.auto_scroll);
        assert_eq!(app.chat_scroll.unseen_items, 0);
    }

    #[test]
    fn test_is_agent_active() {
        let mut app = App::new();
        assert!(!app.is_agent_active());

        app.set_agent_state(AgentState::Thinking);
        assert!(app.is_agent_active());

        app.set_agent_state(AgentState::ExecutingTool);
        assert!(app.is_agent_active());

        app.set_agent_state(AgentState::Streaming);
        assert!(app.is_agent_active());

        app.set_agent_state(AgentState::Cancelling);
        assert!(app.is_agent_active());

        app.set_agent_state(AgentState::Error);
        assert!(!app.is_agent_active());

        app.set_agent_state(AgentState::Offline);
        assert!(!app.is_agent_active());

        app.set_agent_state(AgentState::Idle);
        assert!(!app.is_agent_active());
    }

    #[test]
    fn test_handle_command_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_command(AppCommand::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_command_toggle_activity() {
        let mut app = App::new();
        assert!(!app.show_activity_panel);
        app.handle_command(AppCommand::ToggleActivity);
        assert!(app.show_activity_panel);
        app.handle_command(AppCommand::ToggleActivity);
        assert!(!app.show_activity_panel);
    }

    #[test]
    fn test_handle_command_focus_cycle() {
        let mut app = App::new();
        assert_eq!(app.focus, Focus::Input);

        // Forward without activity panel: Input -> Chat -> Input
        app.handle_command(AppCommand::FocusNext);
        assert_eq!(app.focus, Focus::Chat);
        app.handle_command(AppCommand::FocusNext);
        assert_eq!(app.focus, Focus::Input);

        // Forward with activity panel: Input -> Chat -> Activity -> Input
        app.show_activity_panel = true;
        app.handle_command(AppCommand::FocusNext);
        assert_eq!(app.focus, Focus::Chat);
        app.handle_command(AppCommand::FocusNext);
        assert_eq!(app.focus, Focus::Activity);
        app.handle_command(AppCommand::FocusNext);
        assert_eq!(app.focus, Focus::Input);

        // Backward with activity panel: Input -> Activity -> Chat -> Input
        app.handle_command(AppCommand::FocusPrevious);
        assert_eq!(app.focus, Focus::Activity);
        app.handle_command(AppCommand::FocusPrevious);
        assert_eq!(app.focus, Focus::Chat);
        app.handle_command(AppCommand::FocusPrevious);
        assert_eq!(app.focus, Focus::Input);
    }

    #[test]
    fn test_handle_command_scrolling() {
        let mut app = App::new();
        app.chat_scroll.set_viewport(100, 15);
        assert_eq!(app.chat_scroll.offset, 100);

        // PageUp moves up by page (15 lines)
        app.handle_command(AppCommand::PageUp);
        assert_eq!(app.chat_scroll.offset, 85);

        // ScrollUp moves up by 1 line
        app.handle_command(AppCommand::ScrollUp);
        assert_eq!(app.chat_scroll.offset, 84);

        // ScrollDown moves down by 1 line
        app.handle_command(AppCommand::ScrollDown);
        assert_eq!(app.chat_scroll.offset, 85);

        // PageDown moves down by page (15 lines)
        app.handle_command(AppCommand::PageDown);
        assert_eq!(app.chat_scroll.offset, 100);

        // ScrollToTop jumps directly to top (0)
        app.handle_command(AppCommand::ScrollToTop);
        assert_eq!(app.chat_scroll.offset, 0);

        // ScrollToBottom jumps directly to bottom (100)
        app.handle_command(AppCommand::ScrollToBottom);
        assert_eq!(app.chat_scroll.offset, 100);
    }

    #[test]
    fn test_handle_command_input_and_submit() {
        let mut app = App::new();
        app.handle_command(AppCommand::InsertChar('h'));
        app.handle_command(AppCommand::InsertChar('i'));
        assert_eq!(app.input_state.value(), "hi");

        app.handle_command(AppCommand::DeleteBackward);
        assert_eq!(app.input_state.value(), "h");

        app.handle_command(AppCommand::InsertChar('e'));
        app.handle_command(AppCommand::InsertChar('y'));
        assert_eq!(app.input_state.value(), "hey");

        app.handle_command(AppCommand::Submit);
        assert_eq!(app.input_state.value(), "");
        assert_eq!(app.events.len(), 1);
        match &app.events[0] {
            UserEvent::SendMessage(msg) => assert_eq!(msg, "hey"),
            _ => panic!("Expected SendMessage"),
        }

        // Submitting empty or whitespace does not produce event
        app.handle_command(AppCommand::Submit);
        assert_eq!(app.events.len(), 1);
    }

    #[test]
    fn test_handle_command_paste_and_submit() {
        let mut app = App::new();

        // 1. Paste into empty input
        app.handle_command(AppCommand::Paste("Hello".to_string()));
        assert_eq!(app.input_state.value(), "Hello");
        assert_eq!(app.input_state.cursor(), 5);

        // 2. Paste followed by normal typing
        app.handle_command(AppCommand::InsertChar(' '));
        assert_eq!(app.input_state.value(), "Hello ");

        // 3. Paste in middle
        app.handle_command(AppCommand::CursorLeft);
        app.handle_command(AppCommand::Paste("World".to_string()));
        assert_eq!(app.input_state.value(), "HelloWorld ");

        // 4. Paste at end
        app.handle_command(AppCommand::CursorEnd);
        app.handle_command(AppCommand::Paste("from termina".to_string()));
        assert_eq!(app.input_state.value(), "HelloWorld from termina");

        // 5. Submit after paste
        app.handle_command(AppCommand::Submit);
        assert_eq!(app.input_state.value(), "");
        assert_eq!(app.events.len(), 1);
        match &app.events[0] {
            UserEvent::SendMessage(msg) => assert_eq!(msg, "HelloWorld from termina"),
            _ => panic!("Expected SendMessage"),
        }
        assert_eq!(
            app.input_history.previous(""),
            Some("HelloWorld from termina")
        );

        // 6. Multiline paste followed by submit
        app.handle_command(AppCommand::Paste("first line\nsecond line".to_string()));
        assert_eq!(app.input_state.value(), "first line\nsecond line");
        app.handle_command(AppCommand::Submit);
        assert_eq!(app.events.len(), 2);
        match &app.events[1] {
            UserEvent::SendMessage(msg) => assert_eq!(msg, "first line\nsecond line"),
            _ => panic!("Expected SendMessage"),
        }
    }

    #[test]
    fn test_handle_command_paste_blocked_when_not_input() {
        let mut app = App::new();

        // Blocked when focus is Chat
        app.focus = Focus::Chat;
        app.handle_command(AppCommand::Paste("ignored".to_string()));
        assert_eq!(app.input_state.value(), "");

        // Blocked when focus is Activity
        app.focus = Focus::Activity;
        app.handle_command(AppCommand::Paste("ignored".to_string()));
        assert_eq!(app.input_state.value(), "");

        // Reset to input
        app.focus = Focus::Input;

        // Blocked when agent is active
        app.agent_state = AgentState::Thinking;
        app.handle_command(AppCommand::Paste("ignored".to_string()));
        assert_eq!(app.input_state.value(), "");

        app.agent_state = AgentState::Idle;
        app.active_run_id = Some(1);
        app.handle_command(AppCommand::Paste("ignored".to_string()));
        assert_eq!(app.input_state.value(), "");

        app.active_run_id = None;

        // Blocked when command palette is open
        app.palette_state.is_open = true;
        app.handle_command(AppCommand::Paste("ignored".to_string()));
        assert_eq!(app.input_state.value(), "");

        // Works when idle, palette closed, focus input
        app.palette_state.is_open = false;
        app.handle_command(AppCommand::Paste("accepted".to_string()));
        assert_eq!(app.input_state.value(), "accepted");
    }

    #[test]
    fn test_handle_command_cursor_navigation() {
        let mut app = App::new();
        app.handle_command(AppCommand::InsertChar('a'));
        app.handle_command(AppCommand::InsertChar('b'));
        app.handle_command(AppCommand::InsertChar('c'));
        assert_eq!(app.input_state.cursor(), 3);

        app.handle_command(AppCommand::CursorLeft);
        assert_eq!(app.input_state.cursor(), 2);

        app.handle_command(AppCommand::CursorHome);
        assert_eq!(app.input_state.cursor(), 0);

        app.handle_command(AppCommand::CursorRight);
        assert_eq!(app.input_state.cursor(), 1);

        app.handle_command(AppCommand::CursorEnd);
        assert_eq!(app.input_state.cursor(), 3);

        app.handle_command(AppCommand::ClearInput);
        assert_eq!(app.input_state.value(), "");
        assert_eq!(app.input_state.cursor(), 0);
    }

    #[test]
    fn test_end_to_end_ux_shortcut_lifecycle() {
        let mut app = App::new();

        // 1. Type: "hello"
        for c in "hello".chars() {
            app.handle_command(AppCommand::InsertChar(c));
        }
        assert_eq!(app.input_state.value(), "hello");

        // 2. Press Backspace -> "hell"
        app.handle_command(AppCommand::DeleteBackward);
        assert_eq!(app.input_state.value(), "hell");

        // 3. Press Enter -> exactly one message submitted
        app.handle_command(AppCommand::Submit);
        assert_eq!(app.input_state.value(), "");
        assert_eq!(app.events.len(), 1);
        let msg = match app.events.pop().unwrap() {
            UserEvent::SendMessage(m) => m,
            _ => panic!("Expected SendMessage"),
        };
        assert_eq!(msg, "hell");
        app.add_message(Role::User, msg);
        assert_eq!(app.messages.len(), 1);

        // 4. Press F2 -> Activity Panel toggles visible
        app.handle_command(AppCommand::ToggleActivity);
        assert!(app.show_activity_panel);

        // 5. Press F2 again -> Activity Panel hides
        app.handle_command(AppCommand::ToggleActivity);
        assert!(!app.show_activity_panel);

        // 6. Generate conversation history to make Chat scrollable
        for i in 1..=30 {
            app.add_message(Role::User, format!("Prompt {}", i));
            app.add_message(Role::Assistant, format!("Response {}", i));
        }
        app.chat_scroll.set_max_offset(100);
        assert_eq!(app.chat_scroll.offset, 100);

        // 7. Test PageUp -> Chat scrolls upward
        app.handle_command(AppCommand::PageUp);
        assert_eq!(app.chat_scroll.offset, 90);

        // 8. Test PageDown -> Chat scrolls downward
        app.handle_command(AppCommand::PageDown);
        assert_eq!(app.chat_scroll.offset, 100);

        // 9. Test Home/End scrolling
        app.handle_command(AppCommand::ScrollToTop);
        assert_eq!(app.chat_scroll.offset, 0);
        app.handle_command(AppCommand::ScrollToBottom);
        assert_eq!(app.chat_scroll.offset, 100);

        // 10. Agent behavior unchanged
        app.handle_agent_event(crate::agent::events::AgentEvent::Started);
        assert_eq!(app.agent_state, AgentState::Thinking);
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "Search".to_string(),
            summary: "query".to_string(),
        });
        assert_eq!(app.agent_state, AgentState::ExecutingTool);
        app.handle_agent_event(crate::agent::events::AgentEvent::TextChunk(
            "Done".to_string(),
        ));
        assert_eq!(app.agent_state, AgentState::Streaming);
        app.handle_agent_event(crate::agent::events::AgentEvent::Completed);
        assert_eq!(app.agent_state, AgentState::Idle);

        // 11. Ctrl+C quits
        app.handle_command(AppCommand::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_app_input_history_navigation_and_editing() {
        let mut app = App::new();

        // Submit 3 messages
        for msg in ["hello", "explain rust", "show me the architecture"] {
            for c in msg.chars() {
                app.handle_command(AppCommand::InsertChar(c));
            }
            app.handle_command(AppCommand::Submit);
        }
        assert_eq!(app.input_history.len(), 3);
        assert_eq!(app.input_state.value(), "");

        // Up -> "show me the architecture"
        app.handle_command(AppCommand::InputHistoryPrevious);
        assert_eq!(app.input_state.value(), "show me the architecture");

        // Up -> "explain rust"
        app.handle_command(AppCommand::InputHistoryPrevious);
        assert_eq!(app.input_state.value(), "explain rust");

        // Up -> "hello"
        app.handle_command(AppCommand::InputHistoryPrevious);
        assert_eq!(app.input_state.value(), "hello");

        // Up at boundary -> "hello"
        app.handle_command(AppCommand::InputHistoryPrevious);
        assert_eq!(app.input_state.value(), "hello");

        // Down -> "explain rust"
        app.handle_command(AppCommand::InputHistoryNext);
        assert_eq!(app.input_state.value(), "explain rust");

        // Down -> "show me the architecture"
        app.handle_command(AppCommand::InputHistoryNext);
        assert_eq!(app.input_state.value(), "show me the architecture");

        // Down beyond newest -> empty draft
        app.handle_command(AppCommand::InputHistoryNext);
        assert_eq!(app.input_state.value(), "");

        // Test draft preservation
        for c in "draft text".chars() {
            app.handle_command(AppCommand::InsertChar(c));
        }
        assert_eq!(app.input_state.value(), "draft text");

        // Up into history
        app.handle_command(AppCommand::InputHistoryPrevious);
        assert_eq!(app.input_state.value(), "show me the architecture");

        // Down back to draft
        app.handle_command(AppCommand::InputHistoryNext);
        assert_eq!(app.input_state.value(), "draft text");

        // Recall and edit without corrupting original
        app.handle_command(AppCommand::InputHistoryPrevious); // "show me the architecture"
        app.handle_command(AppCommand::InputHistoryPrevious); // "explain rust"
        assert_eq!(app.input_state.value(), "explain rust");

        for c in " ownership".chars() {
            app.handle_command(AppCommand::InsertChar(c));
        }
        assert_eq!(app.input_state.value(), "explain rust ownership");

        // Submit edited message
        app.handle_command(AppCommand::Submit);
        assert_eq!(app.input_state.value(), "");

        // Check history entries: original "explain rust" is preserved, new "explain rust ownership" added
        assert_eq!(
            app.input_history.entries(),
            &[
                "hello",
                "explain rust",
                "show me the architecture",
                "explain rust ownership"
            ]
        );
    }

    #[test]
    fn test_app_palette_toggle_and_close() {
        let mut app = App::new();
        assert!(!app.palette_state.is_open);

        app.handle_command(AppCommand::OpenCommandPalette);
        assert!(app.palette_state.is_open);

        app.handle_command(AppCommand::CloseCommandPalette);
        assert!(!app.palette_state.is_open);

        app.handle_command(AppCommand::ToggleCommandPalette);
        assert!(app.palette_state.is_open);

        app.handle_command(AppCommand::ToggleCommandPalette);
        assert!(!app.palette_state.is_open);
    }

    #[test]
    fn test_app_palette_navigation_and_query() {
        let mut app = App::new();
        app.handle_command(AppCommand::ToggleCommandPalette);
        assert!(app.palette_state.is_open);

        // Type query
        app.handle_command(AppCommand::PaletteInsertChar('p'));
        app.handle_command(AppCommand::PaletteInsertChar('a'));
        app.handle_command(AppCommand::PaletteInsertChar('g'));
        app.handle_command(AppCommand::PaletteInsertChar('e'));
        assert_eq!(app.palette_state.query, "page");

        // Navigate
        assert_eq!(app.palette_state.selected_index, 0);
        app.handle_command(AppCommand::PaletteNext);
        assert_eq!(app.palette_state.selected_index, 1);

        app.handle_command(AppCommand::PalettePrevious);
        assert_eq!(app.palette_state.selected_index, 0);

        // Backspace
        app.handle_command(AppCommand::PaletteDeleteBackward);
        assert_eq!(app.palette_state.query, "pag");
    }

    #[test]
    fn test_app_palette_selection_executes_action() {
        let mut app = App::new();
        assert!(!app.show_activity_panel);

        // Open palette
        app.handle_command(AppCommand::ToggleCommandPalette);
        assert!(app.palette_state.is_open);

        // Type query for activity
        for c in "activity".chars() {
            app.handle_command(AppCommand::PaletteInsertChar(c));
        }

        // Selected command should be Toggle Activity Panel
        let commands = default_commands();
        let selected = app.palette_state.selected_command(&commands).unwrap();
        assert_eq!(selected.action, AppCommand::ToggleActivity);

        // Execute selection
        app.handle_command(AppCommand::PaletteSelect);

        // Palette is closed and activity panel was toggled!
        assert!(!app.palette_state.is_open);
        assert!(app.show_activity_panel);
    }

    #[test]
    fn test_cancel_agent_when_idle_is_noop() {
        let mut app = App::new();
        assert_eq!(app.agent_state, AgentState::Idle);
        assert!(!app.is_agent_active());

        app.handle_command(AppCommand::CancelAgent);
        assert_eq!(app.agent_state, AgentState::Idle);
        assert!(app.events.is_empty());
    }

    #[test]
    fn test_cancel_agent_when_running() {
        let mut app = App::new();
        app.set_agent_state(AgentState::Streaming);
        app.add_message(Role::Assistant, "Streaming partial response".to_string());
        app.start_activity("Read file".to_string(), "src/main.rs".to_string());

        assert!(app.is_agent_active());
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_command(AppCommand::CancelAgent);

        assert_eq!(app.agent_state, AgentState::Idle);
        assert!(!app.is_agent_active());
        assert_eq!(app.activities[0].status, ActivityStatus::Cancelled);
        assert!(app.messages[0].content.ends_with("[Cancelled]"));

        // UserEvent::CancelAgent pushed to events
        assert_eq!(app.events.len(), 1);
        assert!(matches!(app.events[0], UserEvent::CancelAgent));
    }

    #[test]
    fn test_tool_started_creates_running_activity() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "Read".to_string(),
            summary: "src/main.rs".to_string(),
        });
        assert_eq!(app.activities.len(), 1);
        assert_eq!(app.activities[0].status, ActivityStatus::Running);
        assert_eq!(app.activities[0].tool, "Read");
        assert_eq!(app.activities[0].summary, "src/main.rs");
        assert_eq!(app.activities[0].display_title(), "Read src/main.rs");
    }

    #[test]
    fn test_tool_finished_changes_running_to_completed() {
        let mut app = App::new();
        let id = app.start_activity("Read".to_string(), "src/main.rs".to_string());
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id,
            duration_ms: 1200,
        });
        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
        assert_eq!(app.activities[0].duration_ms, Some(1200));
        assert!(app.activities[0].progress.is_none());
    }

    #[test]
    fn test_error_changes_relevant_activity_to_failed() {
        let mut app = App::new();
        app.start_activity("cargo check".to_string(), "termina".to_string());
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_agent_event(crate::agent::events::AgentEvent::Error(
            "build error".to_string(),
        ));
        assert_eq!(app.activities[0].status, ActivityStatus::Failed);
        assert_eq!(app.activities[0].error.as_deref(), Some("build error"));
    }

    #[test]
    fn test_cancellation_changes_running_to_cancelled() {
        let mut app = App::new();
        app.start_activity("Read".to_string(), "src/main.rs".to_string());
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_agent_event(crate::agent::events::AgentEvent::Cancelled);
        assert_eq!(app.activities[0].status, ActivityStatus::Cancelled);
    }

    #[test]
    fn test_running_activity_does_not_remain_stuck_after_completion() {
        let mut app = App::new();
        app.start_activity("Running cargo check...".to_string(), "".to_string());
        assert_eq!(app.activities[0].status, ActivityStatus::Running);
        assert_eq!(app.activities[0].display_title(), "Running cargo check...");

        // On Completed event, any running activities must be marked Completed
        app.handle_agent_event(crate::agent::events::AgentEvent::Completed);
        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
        // Strips "Running " prefix when finished
        assert_eq!(app.activities[0].display_title(), "cargo check");
    }

    #[test]
    fn test_multiple_activities_can_coexist() {
        let mut app = App::new();
        let id1 = app.start_activity("Read".to_string(), "src/main.rs".to_string());
        app.finish_activity(id1, Some(500));

        let id2 = app.start_activity("Read".to_string(), "src/app.rs".to_string());
        app.finish_activity(id2, Some(300));

        let id3 = app.start_activity("Search project".to_string(), "".to_string());
        app.finish_activity(id3, Some(150));

        let id4 = app.start_activity("Running cargo check...".to_string(), "".to_string());

        assert_eq!(app.activities.len(), 4);
        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
        assert_eq!(app.activities[1].status, ActivityStatus::Completed);
        assert_eq!(app.activities[2].status, ActivityStatus::Completed);
        assert_eq!(app.activities[3].status, ActivityStatus::Running);

        // Verify order: newest is at bottom (index 3)
        assert_eq!(app.activities[0].id, id1);
        assert_eq!(app.activities[3].id, id4);
    }

    #[test]
    fn test_activity_history_remains_bounded() {
        let mut app = App::new();
        app.max_activities = 5;

        for i in 0..10 {
            app.start_activity(format!("Tool {}", i), format!("file_{}.rs", i));
        }

        assert_eq!(app.activities.len(), 5);
        // Should contain the last 5 activities (5 to 9)
        assert_eq!(app.activities[0].tool, "Tool 5");
        assert_eq!(app.activities[4].tool, "Tool 9");
    }

    #[test]
    fn test_tool_output_updates_progress_without_unbounded_raw_storage() {
        let mut app = App::new();
        let id = app.start_activity("cargo build".to_string(), "".to_string());

        // Simulate huge 1,000 line compiler output
        let mut large_output = String::new();
        for i in 0..1000 {
            large_output.push_str(&format!("warning: unused variable in file_{}.rs\n", i));
        }
        large_output.push_str("   Compiling termina v0.1.0 (checking project)\n\n");

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolOutput {
            id,
            output: large_output,
        });

        assert_eq!(app.activities.len(), 1);
        let progress = app.activities[0].progress.as_ref().unwrap();
        // Concise last non-empty line extracted and bounded to <= 80 characters
        assert!(progress.contains("checking project"));
        assert!(progress.len() <= 80);
    }

    #[test]
    fn test_hidden_activity_panel_does_not_prevent_state_updating() {
        let mut app = App::new();
        assert!(!app.show_activity_panel);

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "Read".to_string(),
            summary: "src/main.rs".to_string(),
        });
        assert_eq!(app.activities.len(), 1);
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolOutput {
            id: app.activities[0].id,
            output: "reading...".to_string(),
        });
        assert_eq!(app.activities[0].progress.as_deref(), Some("reading..."));

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id: app.activities[0].id,
            duration_ms: 250,
        });
        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
    }

    #[test]
    fn test_f2_still_toggles_visibility() {
        let mut app = App::new();
        assert!(!app.show_activity_panel);

        app.handle_command(AppCommand::ToggleActivity);
        assert!(app.show_activity_panel);

        app.handle_command(AppCommand::ToggleActivity);
        assert!(!app.show_activity_panel);
    }

    #[test]
    fn test_permission_lifecycle_and_confirmation() {
        let mut app = App::new();
        assert!(app.pending_permission.is_none());

        let (resp_tx, resp_rx) = std::sync::mpsc::sync_channel(1);
        let responder = crate::agent::events::PermissionResponder::new(resp_tx);

        app.handle_agent_event(crate::agent::events::AgentEvent::PermissionRequested {
            id: 1,
            tool_name: "bash".to_string(),
            summary: "cargo build".to_string(),
            current_mode: "workspace-write".to_string(),
            reason: Some("requires DangerFullAccess".to_string()),
            responder,
        });

        assert!(app.pending_permission.is_some());
        let perm = app.pending_permission.as_ref().unwrap();
        assert_eq!(perm.id, 1);
        assert_eq!(perm.tool_name, "bash");
        assert_eq!(perm.choice, PermissionChoice::Allow);

        // Toggle choice to Deny
        app.handle_command(AppCommand::PermissionToggleChoice);
        assert_eq!(
            app.pending_permission.as_ref().unwrap().choice,
            PermissionChoice::Deny
        );

        // Toggle back to Allow
        app.handle_command(AppCommand::PermissionToggleChoice);
        assert_eq!(
            app.pending_permission.as_ref().unwrap().choice,
            PermissionChoice::Allow
        );

        // Confirm choice
        app.handle_command(AppCommand::PermissionConfirm);
        assert!(app.pending_permission.is_none());

        let decision = resp_rx.try_recv().expect("decision should be received");
        assert_eq!(decision, runtime::PermissionPromptDecision::Allow);
    }

    #[test]
    fn test_permission_direct_allow_and_deny() {
        let mut app = App::new();

        // Direct Allow
        let (tx1, rx1) = std::sync::mpsc::sync_channel(1);
        let resp1 = crate::agent::events::PermissionResponder::new(tx1);
        app.handle_agent_event(crate::agent::events::AgentEvent::PermissionRequested {
            id: 1,
            tool_name: "write_file".to_string(),
            summary: "test.txt".to_string(),
            current_mode: "read-only".to_string(),
            reason: None,
            responder: resp1,
        });
        app.handle_command(AppCommand::PermissionSelectAllow);
        assert!(app.pending_permission.is_none());
        assert_eq!(
            rx1.try_recv().unwrap(),
            runtime::PermissionPromptDecision::Allow
        );

        // Direct Deny
        let (tx2, rx2) = std::sync::mpsc::sync_channel(1);
        let resp2 = crate::agent::events::PermissionResponder::new(tx2);
        app.handle_agent_event(crate::agent::events::AgentEvent::PermissionRequested {
            id: 2,
            tool_name: "bash".to_string(),
            summary: "rm -rf".to_string(),
            current_mode: "workspace-write".to_string(),
            reason: None,
            responder: resp2,
        });
        app.handle_command(AppCommand::PermissionSelectDeny);
        assert!(app.pending_permission.is_none());
        match rx2.try_recv().unwrap() {
            runtime::PermissionPromptDecision::Deny { reason } => {
                assert!(reason.contains("denied"));
            }
            other => panic!("Expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn test_permission_cancelled_on_agent_cancel() {
        let mut app = App::new();
        app.set_agent_state(AgentState::ExecutingTool);

        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let resp = crate::agent::events::PermissionResponder::new(tx);
        app.handle_agent_event(crate::agent::events::AgentEvent::PermissionRequested {
            id: 10,
            tool_name: "bash".to_string(),
            summary: "make all".to_string(),
            current_mode: "workspace-write".to_string(),
            reason: None,
            responder: resp,
        });

        assert!(app.pending_permission.is_some());
        app.handle_command(AppCommand::CancelAgent);
        assert!(app.pending_permission.is_none());

        match rx.try_recv().unwrap() {
            runtime::PermissionPromptDecision::Deny { reason } => {
                assert!(reason.contains("cancelled"));
            }
            other => panic!("Expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_starts_as_running() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "search_files".to_string(),
            summary: "query".to_string(),
        });

        assert_eq!(app.activities.len(), 1);
        let act = &app.activities[0];
        assert_eq!(act.tool, "search_files");
        assert_eq!(act.display_name(), "search_files");
        assert_eq!(act.status, ActivityStatus::Running);
        assert_eq!(act.status_text(), "running");
        assert_eq!(app.agent_state, AgentState::ExecutingTool);
    }

    #[test]
    fn test_tool_transitions_to_completed() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "search_files".to_string(),
            summary: "query".to_string(),
        });
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        let id = app.activities[0].id;
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id,
            duration_ms: 1200,
        });

        let act = &app.activities[0];
        assert_eq!(act.status, ActivityStatus::Completed);
        assert_eq!(act.duration_ms, Some(1200));
        assert_eq!(act.status_text(), "1.2s");
    }

    #[test]
    fn test_tool_transitions_to_failed_on_error_output() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "write_file".to_string(),
            summary: "path".to_string(),
        });
        let id = app.activities[0].id;

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolOutput {
            id,
            output: "Error: permission denied".to_string(),
        });
        assert_eq!(
            app.activities[0].error,
            Some("permission denied".to_string())
        );

        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id,
            duration_ms: 300,
        });

        let act = &app.activities[0];
        assert_eq!(act.status, ActivityStatus::Failed);
        assert_eq!(act.status_text(), "failed");
        assert_eq!(act.error.as_deref(), Some("permission denied"));
    }

    #[test]
    fn test_tool_transitions_to_failed_on_agent_error() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "write_file".to_string(),
            summary: "path".to_string(),
        });
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_agent_event(crate::agent::events::AgentEvent::Error(
            "Disk full".to_string(),
        ));

        let act = &app.activities[0];
        assert_eq!(act.status, ActivityStatus::Failed);
        assert_eq!(act.status_text(), "failed");
        assert_eq!(act.error.as_deref(), Some("Disk full"));
        assert_eq!(app.agent_state, AgentState::Error);
    }

    #[test]
    fn test_tool_transitions_to_cancelled_on_cancellation() {
        let mut app = App::new();
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "shell_command".to_string(),
            summary: "sleep 100".to_string(),
        });
        assert_eq!(app.activities[0].status, ActivityStatus::Running);

        app.handle_command(AppCommand::CancelAgent);

        let act = &app.activities[0];
        assert_eq!(act.status, ActivityStatus::Cancelled);
        assert_eq!(act.status_text(), "cancelled");
        assert_ne!(act.status, ActivityStatus::Running);
    }

    #[test]
    fn test_multiple_tools_remain_separate_and_ordered() {
        let mut app = App::new();

        // Tool 1
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "search_files".to_string(),
            summary: "find main".to_string(),
        });
        let id1 = app.activities[0].id;
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id: id1,
            duration_ms: 800,
        });

        // Tool 2
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "read_file".to_string(),
            summary: "src/main.rs".to_string(),
        });
        let id2 = app.activities[1].id;
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolFinished {
            id: id2,
            duration_ms: 300,
        });

        // Tool 3
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "write_file".to_string(),
            summary: "src/main.rs".to_string(),
        });

        assert_eq!(app.activities.len(), 3);
        assert_eq!(app.activities[0].tool, "search_files");
        assert_eq!(app.activities[0].status, ActivityStatus::Completed);
        assert_eq!(app.activities[0].status_text(), "0.8s");

        assert_eq!(app.activities[1].tool, "read_file");
        assert_eq!(app.activities[1].status, ActivityStatus::Completed);
        assert_eq!(app.activities[1].status_text(), "0.3s");

        assert_eq!(app.activities[2].tool, "write_file");
        assert_eq!(app.activities[2].status, ActivityStatus::Running);
        assert_eq!(app.activities[2].status_text(), "running");
    }

    #[test]
    fn test_stale_cancelled_previous_run_events_do_not_corrupt_current_activity() {
        let mut app = App::new();

        // Run 1 starts and is cancelled
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "long_task".to_string(),
            summary: "run 1".to_string(),
        });
        let run1_tool_id = app.activities[0].id;
        app.handle_agent_event(crate::agent::events::AgentEvent::Cancelled);
        assert_eq!(app.activities[0].status, ActivityStatus::Cancelled);

        // Run 2 starts
        app.handle_agent_event(crate::agent::events::AgentEvent::ToolStarted {
            tool: "new_task".to_string(),
            summary: "run 2".to_string(),
        });
        assert_eq!(app.activities[1].status, ActivityStatus::Running);

        // A stale finish event for run1_tool_id should NOT corrupt the running activity
        // In our robust finder, if target ID is already Cancelled and not running,
        // it updates the cancelled activity's duration or ignores it, and leaves run2 running
        app.finish_activity(run1_tool_id, Some(999));
        assert_eq!(app.activities[1].status, ActivityStatus::Running);
    }

    #[test]
    fn test_empty_activity_state() {
        let app = App::new();
        assert!(app.activities.is_empty());
    }

    #[test]
    fn test_run_isolation_when_run_a_cancelled_and_run_b_starts_immediately() {
        let mut app = App::new();

        // 1. Run A starts
        let run_a = 1;
        app.set_active_run_id(Some(run_a));
        app.add_message(Role::User, "Prompt A".to_string());
        app.handle_run_event(run_a, crate::agent::events::AgentEvent::Started);
        app.handle_run_event(run_a, crate::agent::events::AgentEvent::Thinking);
        app.handle_run_event(
            run_a,
            crate::agent::events::AgentEvent::TextChunk("Partial response from A...".to_string()),
        );
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[1].role, Role::Assistant);
        assert_eq!(app.messages[1].content, "Partial response from A...");

        // 2. Run A is cancelled
        app.handle_command(AppCommand::CancelAgent);
        assert_eq!(app.agent_state, AgentState::Idle);
        assert_eq!(app.active_run_id, None);
        assert_eq!(app.cancelled_run_id, Some(run_a));
        assert!(app.messages[1].content.ends_with("[Cancelled]"));

        // 3. Run B starts immediately
        let run_b = 2;
        app.set_active_run_id(Some(run_b));
        app.add_message(Role::User, "Prompt B".to_string());
        app.handle_run_event(run_b, crate::agent::events::AgentEvent::Started);
        app.handle_run_event(
            run_b,
            crate::agent::events::AgentEvent::TextChunk("Response B part 1".to_string()),
        );
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.messages.len(), 4);
        assert_eq!(app.messages[2].role, Role::User);
        assert_eq!(app.messages[2].content, "Prompt B");
        assert_eq!(app.messages[3].role, Role::Assistant);
        assert_eq!(app.messages[3].content, "Response B part 1");

        // 4. Run A emits a late TextChunk
        app.handle_run_event(
            run_a,
            crate::agent::events::AgentEvent::TextChunk("LATE TEXT FROM A".to_string()),
        );
        assert_eq!(app.messages[3].content, "Response B part 1");
        assert_eq!(app.messages.len(), 4);
        assert_eq!(app.agent_state, AgentState::Streaming);

        // 5. Run A emits a late Thinking event
        app.handle_run_event(run_a, crate::agent::events::AgentEvent::Thinking);
        assert_eq!(app.agent_state, AgentState::Streaming); // Must remain Streaming, not revert to Thinking

        // 6. Run A emits a late ToolStarted event
        app.handle_run_event(
            run_a,
            crate::agent::events::AgentEvent::ToolStarted {
                tool: "late_tool_a".to_string(),
                summary: "late tool from A".to_string(),
            },
        );
        assert!(app.activities.is_empty());
        assert_eq!(app.agent_state, AgentState::Streaming);

        // 7. Run A emits a late Completed event
        app.handle_run_event(run_a, crate::agent::events::AgentEvent::Completed);
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.active_run_id, Some(run_b));

        // 8. Run A emits a late Error event
        app.handle_run_event(
            run_a,
            crate::agent::events::AgentEvent::Error("Late error from A".to_string()),
        );
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.active_run_id, Some(run_b));
        assert_eq!(app.messages.len(), 4); // No error message added

        // 9. None of these events modified Run B's UI state.
        // Now Run B produces another chunk and completes normally.
        app.handle_run_event(
            run_b,
            crate::agent::events::AgentEvent::TextChunk(" and part 2".to_string()),
        );
        assert_eq!(app.messages[3].content, "Response B part 1 and part 2");
        app.handle_run_event(run_b, crate::agent::events::AgentEvent::Completed);
        assert_eq!(app.agent_state, AgentState::Idle);
        assert_eq!(app.active_run_id, None);
    }

    #[test]
    fn test_run_a_cancelled_run_b_active_run_a_emits_cancelled_run_b_remains_streaming() {
        let mut app = App::new();

        let run_a = 1;
        let run_b = 2;

        // Run A cancelled
        app.set_active_run_id(Some(run_a));
        app.handle_command(AppCommand::CancelAgent);
        assert_eq!(app.active_run_id, None);
        assert_eq!(app.cancelled_run_id, Some(run_a));

        // Run B active and streaming
        app.set_active_run_id(Some(run_b));
        assert_eq!(app.cancelled_run_id, None); // Cleared on starting Run B
        app.add_message(Role::User, "Prompt B".to_string());
        app.handle_run_event(
            run_b,
            crate::agent::events::AgentEvent::TextChunk("Streaming response B".to_string()),
        );
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.active_run_id, Some(run_b));

        // Run A emits Cancelled
        app.handle_run_event(run_a, crate::agent::events::AgentEvent::Cancelled);

        // Run B must remain Streaming and active!
        assert_eq!(app.agent_state, AgentState::Streaming);
        assert_eq!(app.active_run_id, Some(run_b));
        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[1].content, "Streaming response B");
    }

    #[test]
    fn test_exact_real_world_rust_and_ocean_sequence_in_app() {
        let mut app = App::new();

        // 1. User submits "what is rust in single line"
        let run_rust = 1;
        app.set_active_run_id(Some(run_rust));
        app.add_message(Role::User, "what is rust in single line".to_string());
        app.handle_run_event(run_rust, crate::agent::events::AgentEvent::Started);
        app.handle_run_event(
            run_rust,
            crate::agent::events::AgentEvent::TextChunk(
                "Rust is a fast, memory-safe systems language".to_string(),
            ),
        );
        assert_eq!(app.agent_state, AgentState::Streaming);

        // 2. User presses Ctrl+C to cancel it
        app.handle_command(AppCommand::CancelAgent);
        assert_eq!(app.agent_state, AgentState::Idle);
        assert_eq!(app.active_run_id, None);
        assert!(app.messages[1].content.ends_with("[Cancelled]"));

        // 3. User immediately submits "can you build an html page to explain importants of ocean"
        let run_ocean = 2;
        app.set_active_run_id(Some(run_ocean));
        app.add_message(
            Role::User,
            "can you build an html page to explain importants of ocean".to_string(),
        );
        app.handle_run_event(run_ocean, crate::agent::events::AgentEvent::Started);

        // 4. Stale Run A (Rust) generation produces delayed chunk from background task
        let stale_rust_chunk =
            "Here's a clear, friendly response to your \"what is rust\" question first..."
                .to_string();
        app.handle_run_event(
            run_rust,
            crate::agent::events::AgentEvent::TextChunk(stale_rust_chunk),
        );
        app.handle_run_event(run_rust, crate::agent::events::AgentEvent::Cancelled);

        // 5. Ocean run streams its actual answer
        app.handle_run_event(
            run_ocean,
            crate::agent::events::AgentEvent::TextChunk(
                "<!DOCTYPE html><html><head><title>Importance of Oceans</title></head>".to_string(),
            ),
        );
        app.handle_run_event(run_ocean, crate::agent::events::AgentEvent::Completed);

        // Verify:
        // Messages:
        // [0] User: what is rust in single line
        // [1] Assistant: Rust is a fast, memory-safe systems language\n\n[Cancelled]
        // [2] User: can you build an html page to explain importants of ocean
        // [3] Assistant: <!DOCTYPE html><html><head><title>Importance of Oceans</title></head>
        assert_eq!(app.messages.len(), 4);
        assert_eq!(app.messages[0].content, "what is rust in single line");
        assert!(app.messages[1].content.contains("memory-safe"));
        assert!(app.messages[1].content.ends_with("[Cancelled]"));
        assert_eq!(
            app.messages[2].content,
            "can you build an html page to explain importants of ocean"
        );
        assert_eq!(
            app.messages[3].content,
            "<!DOCTYPE html><html><head><title>Importance of Oceans</title></head>"
        );
        // Stale rust response MUST NOT appear in message 3
        assert!(!app.messages[3].content.contains("what is rust"));
        assert!(!app.messages[3].content.contains("friendly response"));
    }
}
