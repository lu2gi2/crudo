use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::state::AppState;
use crate::ui::theme::{Theme, COLOR_BLACK, COLOR_SECONDARY_PURPLE, COLOR_SUCCESS, COLOR_WHITE};

pub struct ActivityWidget;

impl ActivityWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        if !state.show_activity_drawer || area.height == 0 || area.width == 0 {
            return;
        }

        let title = Line::from(vec![
            Span::styled(" [ Execution Activity ] ", Theme::status_label()),
            Span::styled(" (Tab or Esc to close) ", Theme::placeholder()),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(COLOR_SECONDARY_PURPLE))
            .style(Style::default().bg(COLOR_BLACK))
            .title(title);

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = Vec::new();
        if state.execution_log.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "No background execution tasks or tool calls active.",
                Theme::placeholder(),
            )]));
        } else {
            for act in state
                .execution_log
                .iter()
                .rev()
                .take(inner_area.height as usize)
            {
                let status_style = match act.status {
                    "COMPLETED" => Style::default().fg(COLOR_SUCCESS),
                    "RUNNING" => Theme::status_label(),
                    _ => Style::default().fg(COLOR_WHITE),
                };

                let mut spans = vec![
                    Span::styled(format!("[{}] ", act.category), Theme::status_label()),
                    Span::styled(&act.title, Style::default().fg(COLOR_WHITE)),
                    Span::raw(" "),
                    Span::styled(act.status, status_style),
                ];

                if let Some(ref d) = act.details {
                    spans.push(Span::raw(" - "));
                    spans.push(Span::styled(d, Theme::placeholder()));
                }

                lines.push(Line::from(spans));
            }
        }

        let p = Paragraph::new(lines);
        frame.render_widget(p, inner_area);
    }
}
