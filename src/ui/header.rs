use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;
use std::path::Path;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::state::AppState;
use crate::system::{current_system_date, current_system_time};
use crate::ui::theme::{Theme, COLOR_BLACK, COLOR_SECONDARY_PURPLE};

const CRUDO_LOGO: &[&str] = &[
    r#"    ▄▄█ ▄"#,
    r#"   ▄▀▀▀            ▄▄▄▄▄▄▄   ▄▄▄▄▄▄▄▄▄    ▄▄▄    ▄▄▄  ▄▄▄▄▄▄▄▄▄        ███"#,
    r#"  ▄█▄  ▄  ▄  ▄   ████▀▀▀████ ████   ▀███  ███    ███  ███▄ ▀▀████   ███▀▀▀████"#,
    r#"  ██████▄██▄██   ████     ▀▀ ████▄▄▄▄██▀  ███    ███  ███    ████ ████     ▀████"#,
    r#"  ██▀▄▀███▀▄▀██  ████        █████████▀   ███    ███  ███    ████ ▀███     ▄██▀█"#,
    r#" ██▄▀█ ██▄▀█▀██  ████   ▄██▄ ████ ▀███▄   ███▄  ▄███  ███▄   ████   ████▄▄████"#,
    r#"▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀  ▀███████▀   ███   ▀███   ▀███▀▀█▀   ▀▀▀▀▀███▀        ▀██▀"#,
];

pub fn crudo_logo() -> Paragraph<'static> {
    let purple = Color::Rgb(170, 70, 255);

    let lines: Vec<Line<'static>> = CRUDO_LOGO
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(purple))))
        .collect();

    Paragraph::new(lines)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HeaderWidget;

impl HeaderWidget {
    pub fn new(_logo_path: impl AsRef<Path>) -> Self {
        Self
    }

    /// Renders the header:
    /// Top-left: Character-based CRUDO logo directly through Ratatui (~60-87 columns wide, 10 rows high)
    /// Top-right: Real-time system metrics: TIME, DATE, CPU, GPU
    pub fn render(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let is_welcome = state.is_welcome();

        let block = if is_welcome {
            Block::default().style(Style::default().bg(COLOR_BLACK))
        } else {
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(COLOR_SECONDARY_PURPLE))
                .style(Style::default().bg(COLOR_BLACK))
        };

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        if inner_area.height == 0 || inner_area.width == 0 {
            return;
        }

        // Split horizontally into Left (Logo) and Right (Time / Date / CPU / GPU)
        let sys_info_width = if inner_area.width >= 110 {
            22
        } else if inner_area.width >= 80 {
            20
        } else {
            18.min(inner_area.width / 2)
        };

        let logo_width = inner_area.width.saturating_sub(sys_info_width);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(logo_width),
                Constraint::Length(sys_info_width),
            ])
            .split(inner_area);

        let logo_area = cols[0];
        let clock_area = cols[1];

        // 1. Render character-based CRUDO logo directly through Ratatui only during chat/workbench
        if !is_welcome {
            let logo_widget = crudo_logo();
            frame.render_widget(logo_widget, logo_area);
        }

        // 2. Render System Information (TIME, DATE, CPU, GPU) in Top-Right
        let time_str = current_system_time();
        let date_str = current_system_date();
        let cpu_str = state.system.formatted_cpu_percent();
        let gpu_str = state.system.formatted_gpu();

        let lines = vec![
            Line::from(vec![
                Span::styled("TIME", Theme::status_label()),
                Span::raw(": "),
                Span::styled(time_str, Theme::header_time_value()),
            ]),
            Line::from(vec![
                Span::styled("DATE", Theme::status_label()),
                Span::raw(": "),
                Span::styled(date_str, Theme::header_time_value()),
            ]),
            Line::from(vec![
                Span::styled("CPU", Theme::status_label()),
                Span::raw(":  "),
                Span::styled(cpu_str, Theme::header_metric_value()),
            ]),
            Line::from(vec![
                Span::styled("GPU", Theme::status_label()),
                Span::raw(":  "),
                Span::styled(gpu_str, Theme::header_metric_value()),
            ]),
        ];

        let sys_p = Paragraph::new(lines).alignment(Alignment::Right);
        frame.render_widget(sys_p, clock_area);
    }
}
