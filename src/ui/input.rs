use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::state::AppState;
use crate::ui::theme::{Theme, COLOR_BLACK, COLOR_SECONDARY_PURPLE, COLOR_WHITE};

pub const PLACEHOLDER_TEXT: &str = "Type a message...  /help for commands";

pub struct InputWidget;

impl InputWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        let mut title_spans = Vec::new();
        if !state.attachments.is_empty() {
            title_spans.push(Span::styled(" Attached: ", Theme::status_label()));
            for att in &state.attachments.pending {
                title_spans.push(Span::styled(att.display_badge(), Theme::badge()));
                title_spans.push(Span::raw(" "));
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(COLOR_SECONDARY_PURPLE))
            .style(Style::default().bg(COLOR_BLACK))
            .title(Line::from(title_spans));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        if inner_area.height == 0 || inner_area.width == 0 {
            return;
        }

        if state.input.is_empty() {
            // Visual-only placeholder. Never part of buffer.
            let placeholder_line =
                Line::from(vec![Span::styled(PLACEHOLDER_TEXT, Theme::placeholder())]);
            let p = Paragraph::new(placeholder_line);
            frame.render_widget(p, inner_area);

            // Cursor positioned strictly at the start of actual input (offset 0)
            frame.set_cursor_position(Position::new(inner_area.x, inner_area.y));
        } else {
            // Render actual user buffer
            let input_line = Line::from(vec![Span::styled(
                state.input.buffer(),
                Style::default().fg(COLOR_WHITE),
            )]);
            let p = Paragraph::new(input_line);
            frame.render_widget(p, inner_area);

            // Cursor positioned according to actual cursor offset in user buffer
            let char_offset = state.input.cursor_char_offset();
            let max_offset = (inner_area.width as usize).saturating_sub(1);
            let visual_offset = char_offset.min(max_offset) as u16;

            frame.set_cursor_position(Position::new(inner_area.x + visual_offset, inner_area.y));
        }
    }
}
