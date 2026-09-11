pub mod activity;
pub mod ansi;
pub mod chat;
pub mod header;
pub mod input;
pub mod progress;
pub mod status;
pub mod theme;

pub use activity::ActivityWidget;
pub use chat::ChatWidget;
pub use header::HeaderWidget;
pub use input::InputWidget;
pub use status::StatusBarWidget;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::state::AppState;
use crate::ui::theme::{COLOR_BLACK, COLOR_SECONDARY_PURPLE, COLOR_WHITE};

pub struct UI {
    pub header: HeaderWidget,
}

impl UI {
    pub fn new(logo_path: &str) -> Self {
        Self {
            header: HeaderWidget::new(logo_path),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, state: &AppState) {
        let full_area = frame.area();

        // 1. Fill entire screen with black background
        let bg_block = Block::default().style(Style::default().bg(COLOR_BLACK));
        frame.render_widget(bg_block, full_area);

        // Graceful degradation for critically small terminal dimensions
        if full_area.width < 40 || full_area.height < 8 {
            let msg = vec![
                Line::from(vec![Span::styled(
                    "Terminal too small for CRUDO workspace.",
                    Style::default().fg(COLOR_WHITE),
                )]),
                Line::from(vec![Span::styled(
                    format!(
                        "Current: {}x{}, Minimum: 80x24 recommended.",
                        full_area.width, full_area.height
                    ),
                    theme::Theme::placeholder(),
                )]),
            ];
            let p = Paragraph::new(msg);
            frame.render_widget(p, full_area);
            return;
        }

        // 2. Responsive outer margin calculation (~0.7cm visual approximation)
        let (margin_x, margin_y) = if full_area.width >= 120 && full_area.height >= 36 {
            (2, 1)
        } else if full_area.width >= 80 && full_area.height >= 24 {
            (1, 1)
        } else {
            (0, 0)
        };

        let outer_rect = Rect::new(
            full_area.x + margin_x,
            full_area.y + margin_y,
            full_area.width.saturating_sub(margin_x * 2),
            full_area.height.saturating_sub(margin_y * 2),
        );

        // 3. Application Frame with CRUDO purple border
        let app_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(COLOR_SECONDARY_PURPLE))
            .style(Style::default().bg(COLOR_BLACK));

        let app_inner = app_block.inner(outer_rect);
        frame.render_widget(app_block, outer_rect);

        if app_inner.height < 4 || app_inner.width < 10 {
            return;
        }

        // 4. Calculate heights inside application frame
        let header_height = if app_inner.height >= 20 {
            9 // 8 rows of logo + 1 bottom border
        } else if app_inner.height >= 16 {
            5 // 4 rows for system metrics + 1 bottom border
        } else {
            4.min(app_inner.height.saturating_sub(4))
        };

        let activity_height = if state.show_activity_drawer && app_inner.height >= 24 {
            6
        } else {
            0
        };

        let input_height = 3;
        let status_height = 1;

        // Vertical hierarchy: Header -> Chat Workspace (majority) -> [Activity] -> Input -> Status
        let mut constraints = vec![
            Constraint::Length(header_height),
            Constraint::Min(4), // Chat workspace consumes majority
        ];

        if activity_height > 0 {
            constraints.push(Constraint::Length(activity_height));
        }

        constraints.push(Constraint::Length(input_height));
        constraints.push(Constraint::Length(status_height));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(app_inner);

        let header_rect = chunks[0];
        let chat_rect = chunks[1];

        let (drawer_rect, input_rect, status_rect) = if activity_height > 0 {
            (Some(chunks[2]), chunks[3], chunks[4])
        } else {
            (None, chunks[2], chunks[3])
        };

        // Render Components
        self.header.render(frame, header_rect, state);
        ChatWidget::render(frame, chat_rect, state);

        if let Some(d_rect) = drawer_rect {
            ActivityWidget::render(frame, d_rect, state);
        }

        InputWidget::render(frame, input_rect, state);
        StatusBarWidget::render(frame, status_rect, state);
    }
}
