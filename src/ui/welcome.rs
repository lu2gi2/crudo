use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::state::AppState;
use crate::ui::theme::{COLOR_BLACK, COLOR_CRUDO_PURPLE};

pub const CRUDO_LOGO: &[&str] = &[
    r#"        ███"#,
    r#"      ██    ███"#,
    r#"     ██████"#,
    r#"     █"#,
    r#"                                                                                                                                            ███"#,
    r#"    ████                                 ██████████████     ████████████         █████          █████     ██████████████           ████ ██████████ ████"#,
    r#"    ████                                ████████████████    ██████████████       █████          █████   █████████████████           ██████████████████"#,
    r#"    ████                               ██████████████████   ███████████████      █████          █████   █████  ███████████        ███████████ ███████████"#,
    r#"    ████                               ██████     ████████  ████      ███████    █████          █████   █████         █████        ████████     ████████"#,
    r#"    ████     ██      ██     ██         ██████        ████   ████         ████    █████          █████   █████         ██████    ███████              ██████"#,
    r#"    ████   ████    ████   ████         ██████               ████         ████    █████          █████   █████         ██████  █████████              ████████"#,
    r#"  ████████████████████████████         ██████               ████████████████     █████          █████   █████         ██████   ██████                  ██████"#,
    r#"  ████████  █████████  ███████         ██████               ██████████████       █████          █████   █████         ██████   ██████                  ██████"#,
    r#"  ███████ ██ ███████ ██ ██████         ██████               ████████████████     █████          █████   █████         ██████   █  ██████            ██████  █"#,
    r#"  ██████ ████ █████ ████ █████         ██████        ████   ████         ████    ██████        ██████   █████         ██████      █████████      █████████"#,
    r#"  ███████ ██ ███████ ██ ██████         ██████       █████   ████         ████    ███████      ███████   █████  ████████████        ███████████ ██████████"#,
    r#"██████████  ██     ██  ████████        █████████████████    ████         █████    ██████████████████    ███████████████████          ███████████████████"#,
    r#"███████████████   ██████████████        ████████████████    ████         █████     ████████████████     ██████████████████              █████████████"#,
    r#"█████████████████████████████████        █            ██    █    █       █    █      █            █      █              █                   ███  ███"#,
    r#"                                           ████████████      █████        █████         ███████████         █████████████                      █"#,
];

pub const WELCOME_MESSAGE: &str = "WELCOME TO CRUDO. WHAT WOULD YOU LIKE TO WORK ON TODAY?";
pub const COLOR_WELCOME_TEXT: Color = Color::Rgb(181, 156, 247); // #B59CF7
pub const COLOR_WELCOME_LOGO: Color = COLOR_CRUDO_PURPLE;

pub const LOGO_WIDTH: usize = 157;
pub const LOGO_HEIGHT: usize = 20;

/// Calculates the centered position offset (left_pad, top_pad, visible_w, visible_h)
/// for the big logo within the available central workspace without altering logo dimensions.
pub fn calculate_logo_position(
    workspace_width: u16,
    workspace_height: u16,
) -> (u16, u16, u16, u16) {
    let logo_w = (LOGO_WIDTH as u16).min(workspace_width);
    let left_pad = workspace_width.saturating_sub(LOGO_WIDTH as u16) / 2;

    let comp_height = (LOGO_HEIGHT as u16) + 3; // 20 logo + 2 gap + 1 message
    if workspace_height >= comp_height {
        let top_pad = (workspace_height - comp_height) / 2;
        let logo_h = LOGO_HEIGHT as u16;
        (left_pad, top_pad, logo_w, logo_h)
    } else {
        let msg_h = if workspace_height >= 3 { 1 } else { 0 };
        let gap_h = if workspace_height >= 4 { 2 } else { 0 };
        let logo_h = workspace_height
            .saturating_sub(msg_h + gap_h)
            .min(LOGO_HEIGHT as u16);
        (left_pad, 0, logo_w, logo_h)
    }
}

/// Wraps the welcome message cleanly across multiple lines when terminal width requires it.
pub fn wrap_welcome_message(text: &str, max_width: usize) -> Vec<String> {
    let char_count = text.chars().count();
    if char_count <= max_width {
        return vec![text.to_string()];
    }

    // Clean balanced two-line wrap for medium/narrow widths
    if (40..55).contains(&max_width) {
        return vec![
            "WELCOME TO CRUDO. WHAT WOULD YOU LIKE TO".to_string(),
            "WORK ON TODAY?".to_string(),
        ];
    }

    // Word wrapping fallback for very narrow terminals
    let words = text.split_whitespace();
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_len = 0;

    for word in words {
        let w_len = word.chars().count();
        if !current.is_empty() && current_len + 1 + w_len > max_width {
            lines.push(current.join(" "));
            current.clear();
            current.push(word);
            current_len = w_len;
        } else {
            if !current.is_empty() {
                current_len += 1;
            }
            current.push(word);
            current_len += w_len;
        }
    }

    if !current.is_empty() {
        lines.push(current.join(" "));
    }

    if lines.is_empty() {
        vec![text.to_string()]
    } else {
        lines
    }
}

/// Welcome widget displaying the large centered CRUDO logo and welcome text.
#[derive(Debug, Default, Clone, Copy)]
pub struct WelcomeWidget;

impl WelcomeWidget {
    pub fn new() -> Self {
        Self
    }

    /// Renders the welcome presentation inside the central workspace area.
    pub fn render(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        // Record metrics for state consistency
        state.conversation.viewport_height.set(area.height as usize);
        state
            .conversation
            .last_viewport_height
            .set(area.height as usize);

        // 1. Fill central workspace area with clean background
        let bg_block = Block::default().style(Style::default().bg(COLOR_BLACK));
        frame.render_widget(bg_block, area);

        // 2. Position the hardcoded big logo in the center of the available central workspace
        let (left_pad, top_pad, logo_w, logo_h) = calculate_logo_position(area.width, area.height);

        if logo_w > 0 && logo_h > 0 {
            let logo_rect = Rect::new(area.x + left_pad, area.y + top_pad, logo_w, logo_h);

            let logo_style = Style::default().fg(COLOR_WELCOME_LOGO);
            let logo_lines: Vec<Line<'static>> = CRUDO_LOGO
                .iter()
                .map(|line| Line::from(Span::styled(*line, logo_style)))
                .collect();
            let logo_p = Paragraph::new(logo_lines);
            frame.render_widget(logo_p, logo_rect);
        }

        // 3. Two additional blank lines before welcome text
        let gap = if area.height >= top_pad + logo_h + 3 {
            2
        } else if area.height >= top_pad + logo_h + 2 {
            1
        } else {
            0
        };

        // 4. Welcome text centered horizontally
        let msg_y = area.y + top_pad + logo_h + gap;
        if msg_y < area.y + area.height {
            let max_msg_w = (area.width as usize).saturating_sub(4).max(10);
            let wrapped_msg = wrap_welcome_message(WELCOME_MESSAGE, max_msg_w);
            let avail_msg_h = (area.y + area.height - msg_y).min(wrapped_msg.len() as u16);

            let msg_rect = Rect::new(area.x, msg_y, area.width, avail_msg_h);

            let msg_style = Style::default()
                .fg(COLOR_WELCOME_TEXT)
                .add_modifier(Modifier::BOLD);
            let msg_lines: Vec<Line<'static>> = wrapped_msg
                .into_iter()
                .map(|l| Line::from(Span::styled(l, msg_style)))
                .collect();
            let msg_p = Paragraph::new(msg_lines).alignment(Alignment::Center);
            frame.render_widget(msg_p, msg_rect);
        }
    }
}

/// Standalone helper to render welcome widget matching preferred function signature.
pub fn render_welcome(frame: &mut Frame, area: Rect, widget: &mut WelcomeWidget, state: &AppState) {
    widget.render(frame, area, state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logo_dimensions_and_count() {
        assert_eq!(
            CRUDO_LOGO.len(),
            20,
            "CRUDO_LOGO must have exactly 20 lines"
        );
        let max_w = CRUDO_LOGO
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        assert_eq!(
            max_w, LOGO_WIDTH,
            "LOGO_WIDTH must match maximum line length"
        );
    }

    #[test]
    fn test_calculate_logo_position() {
        // Wide and tall workspace
        let (left, top, w, h) = calculate_logo_position(200, 50);
        assert_eq!(left, (200 - LOGO_WIDTH as u16) / 2);
        assert_eq!(top, (50 - 23) / 2);
        assert_eq!(w, LOGO_WIDTH as u16);
        assert_eq!(h, LOGO_HEIGHT as u16);

        // Narrower workspace
        let (left_narrow, top_narrow, w_narrow, h_narrow) = calculate_logo_position(100, 20);
        assert_eq!(left_narrow, 0);
        assert_eq!(top_narrow, 0);
        assert_eq!(w_narrow, 100);
        assert_eq!(h_narrow, 17);
    }

    #[test]
    fn test_wrap_welcome_message() {
        // Wide: 1 line
        let wrapped_wide = wrap_welcome_message(WELCOME_MESSAGE, 100);
        assert_eq!(wrapped_wide.len(), 1);
        assert_eq!(wrapped_wide[0], WELCOME_MESSAGE);

        // Medium: 40..55 -> 2 lines
        let wrapped_med = wrap_welcome_message(WELCOME_MESSAGE, 45);
        assert_eq!(wrapped_med.len(), 2);
        assert_eq!(wrapped_med[0], "WELCOME TO CRUDO. WHAT WOULD YOU LIKE TO");
        assert_eq!(wrapped_med[1], "WORK ON TODAY?");

        // Very narrow: word wrap
        let wrapped_narrow = wrap_welcome_message(WELCOME_MESSAGE, 25);
        assert!(wrapped_narrow.len() >= 3);
        for line in &wrapped_narrow {
            assert!(line.chars().count() <= 25);
        }
    }
}
