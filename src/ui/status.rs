use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::events::{McpStatus, ModelStatus, SandboxStatus};
use crate::state::AppState;
use crate::ui::theme::{Theme, COLOR_BLACK};

pub struct StatusBarWidget;

impl StatusBarWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let width = area.width as usize;
        let spans = if width >= 82 {
            Self::build_full_spans(state)
        } else if width >= 50 {
            Self::build_compact_spans(state)
        } else {
            Self::build_minimal_spans(state)
        };

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(COLOR_BLACK));
        frame.render_widget(paragraph, area);
    }

    fn build_full_spans(state: &AppState) -> Vec<Span<'static>> {
        let model_val = match &state.backend.model {
            ModelStatus::NotConnected => "NOT CONNECTED".to_string(),
            ModelStatus::Loaded(m) => m.clone(),
            ModelStatus::Generating(m) => format!("{m} (BUSY)"),
            ModelStatus::Error(e) => format!("ERR ({e})"),
        };

        let mcp_val = match &state.backend.mcp {
            McpStatus::NotConnected => "NOT CONNECTED",
            McpStatus::Off => "OFF",
            McpStatus::Connecting => "CONNECTING",
            McpStatus::Ready => "READY",
            McpStatus::Busy => "BUSY",
            McpStatus::Error(_) => "ERROR",
        };

        let sandbox_val = match &state.backend.sandbox {
            SandboxStatus::NotConnected => "NOT CONNECTED",
            SandboxStatus::NotReady => "NOT READY",
            SandboxStatus::Starting => "STARTING",
            SandboxStatus::Ready => "READY",
            SandboxStatus::Busy => "BUSY",
            SandboxStatus::Error(_) => "ERROR",
        };

        vec![
            Span::raw(" "),
            Span::styled("MODEL", Theme::status_label()),
            Span::raw(": "),
            Span::styled(model_val, Theme::status_value()),
            Span::styled(" │ ", Theme::status_delimiter()),
            Span::styled("MCP", Theme::status_label()),
            Span::raw(": "),
            Span::styled(mcp_val, Theme::status_value()),
            Span::styled(" │ ", Theme::status_delimiter()),
            Span::styled("SANDBOX", Theme::status_label()),
            Span::raw(": "),
            Span::styled(sandbox_val, Theme::status_value()),
            Span::styled(" │ ", Theme::status_delimiter()),
            Span::styled("NET", Theme::status_label()),
            Span::raw(": "),
            Span::styled("OFF", Theme::status_value()),
        ]
    }

    fn build_compact_spans(state: &AppState) -> Vec<Span<'static>> {
        let model_val = match &state.backend.model {
            ModelStatus::NotConnected => "NOT CONNECTED".to_string(),
            ModelStatus::Loaded(m) => m.clone(),
            ModelStatus::Generating(m) => format!("{m}*"),
            ModelStatus::Error(_) => "ERROR".to_string(),
        };

        let mcp_val = match &state.backend.mcp {
            McpStatus::NotConnected => "OFF",
            McpStatus::Off => "OFF",
            McpStatus::Connecting => "CONN",
            McpStatus::Ready => "READY",
            McpStatus::Busy => "BUSY",
            McpStatus::Error(_) => "ERR",
        };

        let sandbox_val = match &state.backend.sandbox {
            SandboxStatus::NotConnected => "OFF",
            SandboxStatus::NotReady => "NOT RDY",
            SandboxStatus::Starting => "START",
            SandboxStatus::Ready => "READY",
            SandboxStatus::Busy => "BUSY",
            SandboxStatus::Error(_) => "ERR",
        };

        vec![
            Span::raw(" "),
            Span::styled("MODEL", Theme::status_label()),
            Span::raw(": "),
            Span::styled(model_val, Theme::status_value()),
            Span::styled(" │ ", Theme::status_delimiter()),
            Span::styled("MCP", Theme::status_label()),
            Span::raw(": "),
            Span::styled(mcp_val, Theme::status_value()),
            Span::styled(" │ ", Theme::status_delimiter()),
            Span::styled("SANDBOX", Theme::status_label()),
            Span::raw(": "),
            Span::styled(sandbox_val, Theme::status_value()),
            Span::styled(" │ ", Theme::status_delimiter()),
            Span::styled("NET", Theme::status_label()),
            Span::raw(": "),
            Span::styled("OFF", Theme::status_value()),
        ]
    }

    fn build_minimal_spans(state: &AppState) -> Vec<Span<'static>> {
        let model_val = match &state.backend.model {
            ModelStatus::NotConnected => "NONE".to_string(),
            ModelStatus::Loaded(m) => m.clone(),
            ModelStatus::Generating(m) => format!("{m}*"),
            ModelStatus::Error(_) => "ERR".to_string(),
        };

        vec![
            Span::raw(" "),
            Span::styled("M", Theme::status_label()),
            Span::raw(":"),
            Span::styled(model_val, Theme::status_value()),
            Span::styled("│", Theme::status_delimiter()),
            Span::styled("MCP", Theme::status_label()),
            Span::raw(":"),
            Span::styled("OFF", Theme::status_value()),
            Span::styled("│", Theme::status_delimiter()),
            Span::styled("SB", Theme::status_label()),
            Span::raw(":"),
            Span::styled("OFF", Theme::status_value()),
            Span::styled("│", Theme::status_delimiter()),
            Span::styled("NET", Theme::status_label()),
            Span::raw(":"),
            Span::styled("OFF", Theme::status_value()),
        ]
    }
}
