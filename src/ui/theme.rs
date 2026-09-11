use ratatui::style::{Color, Modifier, Style};

// Primary Palette
pub const COLOR_BLACK: Color = Color::Rgb(0x00, 0x00, 0x00);
pub const COLOR_WHITE: Color = Color::Rgb(0xFF, 0xFF, 0xFF);
pub const COLOR_PRIMARY_PURPLE: Color = Color::Rgb(0x7C, 0x5B, 0xD8); // #7C5BD8
pub const COLOR_SECONDARY_PURPLE: Color = Color::Rgb(0x6A, 0x4E, 0xBF); // #6A4EBF
pub const COLOR_CRUDO_PURPLE: Color = Color::Rgb(0x84, 0x61, 0xEF); // #8461EF
pub const COLOR_STATUS_LABEL: Color = Color::Rgb(0x83, 0x68, 0xFE); // #8368FE
pub const COLOR_USER: Color = Color::Rgb(193, 173, 249); // #C1ADF9
pub const COLOR_USER_BLUE: Color = COLOR_USER;

// Accent & Neutral Palette
pub const COLOR_MUTED: Color = Color::Rgb(0x80, 0x80, 0x80);
pub const COLOR_DARK_BORDER: Color = Color::Rgb(0x30, 0x20, 0x50);
pub const COLOR_SUCCESS: Color = Color::Rgb(0x50, 0xC8, 0x78);
pub const COLOR_ERROR: Color = Color::Rgb(0xDF, 0x46, 0x46);

// Glyphs (Unicode glyphs with reliable terminal rendering)
pub const GLYPH_USER_TRIANGLE: &str = "▶"; // \u{25B6}
pub const GLYPH_CRUDO_GEAR: &str = "⚙"; // \u{2699}
pub const GLYPH_PROGRESS_FULL: &str = "█"; // \u{2588}
pub const GLYPH_PROGRESS_EMPTY: &str = "░"; // \u{2591}
pub const GLYPH_ATTACHMENT: &str = "📎";
pub const GLYPH_DELIMITER: &str = "│";

pub struct Theme;

impl Theme {
    pub fn default_background() -> Style {
        Style::default().bg(COLOR_BLACK).fg(COLOR_WHITE)
    }

    pub fn user_marker() -> Style {
        Style::default().fg(COLOR_USER).add_modifier(Modifier::BOLD)
    }

    pub fn crudo_marker() -> Style {
        Style::default()
            .fg(COLOR_CRUDO_PURPLE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn user_box_border() -> Style {
        Style::default().fg(COLOR_USER)
    }

    pub fn input_box_border() -> Style {
        Style::default().fg(COLOR_SECONDARY_PURPLE)
    }

    pub fn status_label() -> Style {
        Style::default()
            .fg(COLOR_STATUS_LABEL)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_value() -> Style {
        Style::default().fg(COLOR_WHITE)
    }

    pub fn status_delimiter() -> Style {
        Style::default().fg(COLOR_SECONDARY_PURPLE)
    }

    pub fn placeholder() -> Style {
        Style::default().fg(COLOR_MUTED)
    }

    pub fn progress_bar() -> Style {
        Style::default().fg(COLOR_PRIMARY_PURPLE)
    }

    pub fn progress_bg() -> Style {
        Style::default().fg(COLOR_DARK_BORDER)
    }

    pub fn timestamp() -> Style {
        Style::default().fg(COLOR_WHITE)
    }

    pub fn badge() -> Style {
        Style::default()
            .fg(COLOR_PRIMARY_PURPLE)
            .bg(COLOR_BLACK)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_time_label() -> Style {
        Style::default()
            .fg(COLOR_STATUS_LABEL)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_time_value() -> Style {
        Style::default()
            .fg(COLOR_WHITE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_metric_label() -> Style {
        Style::default()
            .fg(COLOR_STATUS_LABEL)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_metric_value() -> Style {
        Style::default().fg(COLOR_WHITE)
    }

    pub fn app_frame_border() -> Style {
        Style::default().fg(COLOR_SECONDARY_PURPLE)
    }

    pub fn chat_box_border() -> Style {
        Style::default().fg(COLOR_PRIMARY_PURPLE)
    }
}
