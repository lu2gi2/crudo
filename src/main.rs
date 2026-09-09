use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::{
    cursor::EnableBlinking,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Terminal,
};

use serde_json::json;

// ============================================================
// CRUDO THEME
// ============================================================

const CRUDO_PURPLE: Color = Color::Rgb(145, 72, 255);
const CRUDO_PURPLE_LIGHT: Color = Color::Rgb(175, 105, 255);
const CRUDO_WHITE: Color = Color::Rgb(225, 230, 245);
const CRUDO_GRAY: Color = Color::Rgb(145, 155, 180);
const CRUDO_DARK: Color = Color::Rgb(8, 9, 13);

// ============================================================
// HEADER
// ============================================================

const CRUDO_LOGO: [&str; 5] = [
    "       ██████╗██████╗ ██╗   ██╗██████╗  ██████╗ ",
    "      ██╔════╝██╔══██╗██║   ██║██╔══██╗██╔═══██╗",
    "      ██║     ██████╔╝██║   ██║██║  ██║██║   ██║",
    "      ██║     ██╔══██╗██║   ██║██║  ██║██║   ██║",
    "      ╚██████╗██║  ██║╚██████╔╝██████╔╝╚██████╔╝",
];

const CRUDO_TAGLINE: &str =
    "L o c a l   •   P r i v a t e   •   O f f l i n e";

// ============================================================
// MAIN
// ============================================================

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBlinking
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let mut input = String::new();
    let mut cursor_pos: usize = 0;
    let mut messages: Vec<String> = Vec::new();
    let mut scroll: u16 = 0;
    let mut status = String::from("Ready");
    let mut auto_scroll = true;
    let mut generation: Option<GenerationTask> = None;
    let mut menu = SuggestionMenu::default();
    let mut history = CommandHistory::default();
    let mut pending_confirmation: Option<PendingAction> = None;
    let mut needs_redraw = true;

    loop {
        let mut got_stream_update = false;

        if let Some(task) = generation.as_mut() {
            let mut is_done = false;
            let mut stream_error = None;
            let mut channel_closed = false;
            let mut got_chunk = false;

            while match task.rx.try_recv() {
                Ok(msg) => {
                    match msg {
                        Ok(line) => {
                            if !line.trim().is_empty() {
                                match serde_json::from_str::<serde_json::Value>(&line) {
                                    Ok(data) => {
                                        if let Some(text) = data["response"].as_str() {
                                            if !task.has_started_ai_message {
                                                messages.push(format!("AI: {}", text));
                                                task.has_started_ai_message = true;
                                            } else if let Some(last) = messages.last_mut() {
                                                last.push_str(text);
                                            }
                                            got_chunk = true;
                                        }

                                        if data["done"].as_bool() == Some(true) {
                                            is_done = true;
                                        }
                                    }
                                    Err(error) => {
                                        stream_error = Some(error.to_string());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            stream_error = Some(error);
                        }
                    }
                    !is_done && stream_error.is_none()
                }
                Err(mpsc::TryRecvError::Empty) => false,
                Err(mpsc::TryRecvError::Disconnected) => {
                    channel_closed = true;
                    false
                }
            } {}

            if let Some(error) = stream_error {
                task.cancel();
                generation = None;
                status = String::from("Error");
                if let Some(last) = messages.last_mut() {
                    if last == "AI: " {
                        *last = format!("AI: Error: {}", error);
                    } else {
                        messages.push(format!("AI: Error: {}", error));
                    }
                } else {
                    messages.push(format!("AI: Error: {}", error));
                }
                if auto_scroll {
                    scroll_to_bottom(&messages, &terminal, &mut scroll, false);
                }
                got_stream_update = true;
            } else if is_done || channel_closed {
                generation = None;
                status = String::from("Ready");
                if auto_scroll {
                    scroll_to_bottom(&messages, &terminal, &mut scroll, false);
                }
                got_stream_update = true;
            } else if got_chunk {
                if auto_scroll {
                    scroll_to_bottom(&messages, &terminal, &mut scroll, true);
                }
                got_stream_update = true;
            }
        }

        if got_stream_update {
            needs_redraw = true;
        }

        if needs_redraw {
            let is_responding = generation.is_some();
            draw_ui(
                &mut terminal,
                &input,
                cursor_pos,
                &messages,
                scroll,
                status.as_str(),
                is_responding,
                &menu,
                pending_confirmation.as_ref(),
            )?;
            needs_redraw = false;
        }

        if handle_event(
            &mut terminal,
            &mut input,
            &mut cursor_pos,
            &mut messages,
            &mut scroll,
            &mut status,
            &mut auto_scroll,
            &mut generation,
            &mut menu,
            &mut history,
            &mut pending_confirmation,
            &mut needs_redraw,
        )? {
            if let Some(task) = generation.take() {
                task.cancel();
            }
            break;
        }
    }

    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;

    Ok(())
}

// ============================================================
// COMMAND HISTORY & PERMISSION CONFIRMATION
// ============================================================

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CommandHistory {
    pub entries: Vec<String>,
    pub index: Option<usize>,
    pub draft: String,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: String) {
        let trimmed = entry.trim();
        if !trimmed.is_empty() {
            if self.entries.last().map(|s| s.as_str()) != Some(trimmed) {
                self.entries.push(trimmed.to_string());
            }
        }
        self.index = None;
        self.draft.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_navigating(&self) -> bool {
        self.index.is_some()
    }

    pub fn up(&mut self, current_input: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        match self.index {
            None => {
                self.draft = current_input.to_string();
                let last_idx = self.entries.len() - 1;
                self.index = Some(last_idx);
                Some(self.entries[last_idx].clone())
            }
            Some(idx) => {
                if idx > 0 {
                    let new_idx = idx - 1;
                    self.index = Some(new_idx);
                    Some(self.entries[new_idx].clone())
                } else {
                    None
                }
            }
        }
    }

    pub fn down(&mut self) -> Option<String> {
        match self.index {
            None => None,
            Some(idx) => {
                if idx + 1 < self.entries.len() {
                    let new_idx = idx + 1;
                    self.index = Some(new_idx);
                    Some(self.entries[new_idx].clone())
                } else {
                    self.index = None;
                    Some(self.draft.clone())
                }
            }
        }
    }

    pub fn reset_navigation(&mut self) {
        self.index = None;
        self.draft.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    Clear,
    Custom(String),
}

pub fn is_sensitive_action(command: &str) -> bool {
    let trimmed = command.trim();
    trimmed == "/clear" || trimmed.starts_with("/clear ")
}

// ============================================================
// SLASH COMMAND MENU
// ============================================================

pub const SLASH_COMMANDS: [&str; 6] = [
    "/help",
    "/clear",
    "/pwd",
    "/ls",
    "/read",
    "/exit",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionMenu {
    pub is_open: bool,
    pub selected: usize,
}

impl Default for SuggestionMenu {
    fn default() -> Self {
        Self {
            is_open: false,
            selected: 0,
        }
    }
}

pub fn filter_commands(input: &str) -> Vec<&'static str> {
    if !input.starts_with('/') {
        return Vec::new();
    }
    if input.contains(char::is_whitespace) {
        return Vec::new();
    }
    let query = input.to_lowercase();
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|cmd| cmd.to_lowercase().starts_with(&query))
        .collect()
}

fn draw_suggestion_menu(
    frame: &mut ratatui::Frame,
    area: Rect,
    filtered: &[&str],
    selected: usize,
) {
    if filtered.is_empty() || area.height < 3 || area.width < 10 {
        return;
    }

    let menu_height = (filtered.len() as u16 + 2).min(area.height);
    let menu_width = 24u16.min(area.width.saturating_sub(2)).max(12);

    let menu_y = area.bottom().saturating_sub(menu_height);
    let menu_x = area.x.saturating_add(1);

    let menu_area = Rect::new(menu_x, menu_y, menu_width, menu_height);

    let mut lines = Vec::new();
    let inner_width = menu_width.saturating_sub(2) as usize;

    for (i, cmd) in filtered.iter().enumerate() {
        let is_selected = i == selected;
        let prefix = if is_selected { "❯ " } else { "  " };
        let cmd_text = format!("{}{}", prefix, cmd);
        let padding = inner_width.saturating_sub(cmd_text.chars().count());
        let full_text = format!("{}{}", cmd_text, " ".repeat(padding));

        let style = if is_selected {
            Style::default()
                .fg(CRUDO_WHITE)
                .bg(CRUDO_PURPLE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(CRUDO_WHITE)
                .bg(CRUDO_DARK)
        };

        lines.push(Line::from(Span::styled(full_text, style)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CRUDO_PURPLE))
        .style(Style::default().bg(CRUDO_DARK));

    let menu_widget = Paragraph::new(Text::from(lines)).block(block);

    frame.render_widget(Clear, menu_area);
    frame.render_widget(menu_widget, menu_area);
}

// ============================================================
// INPUT EDITING
// ============================================================

pub fn insert_char_at_cursor(input: &mut String, cursor_pos: &mut usize, c: char) {
    let byte_offset = input
        .char_indices()
        .nth(*cursor_pos)
        .map(|(i, _)| i)
        .unwrap_or(input.len());
    input.insert(byte_offset, c);
    *cursor_pos += 1;
}

pub fn delete_char_before_cursor(input: &mut String, cursor_pos: &mut usize) {
    if *cursor_pos > 0 && !input.is_empty() {
        let char_idx = *cursor_pos - 1;
        if let Some((byte_start, c)) = input.char_indices().nth(char_idx) {
            let byte_end = byte_start + c.len_utf8();
            input.replace_range(byte_start..byte_end, "");
            *cursor_pos -= 1;
        }
    }
}

pub fn delete_word_before_cursor(input: &mut String, cursor_pos: &mut usize) {
    if *cursor_pos == 0 || input.is_empty() {
        return;
    }

    let chars: Vec<char> = input.chars().collect();
    let mut new_cursor = *cursor_pos;

    while new_cursor > 0 && chars[new_cursor - 1].is_whitespace() {
        new_cursor -= 1;
    }

    while new_cursor > 0 && !chars[new_cursor - 1].is_whitespace() {
        new_cursor -= 1;
    }

    let del_start_char = new_cursor;
    let del_end_char = *cursor_pos;

    if del_start_char < del_end_char {
        let byte_start = input
            .char_indices()
            .nth(del_start_char)
            .map(|(i, _)| i)
            .unwrap_or(input.len());
        let byte_end = input
            .char_indices()
            .nth(del_end_char)
            .map(|(i, _)| i)
            .unwrap_or(input.len());
        input.replace_range(byte_start..byte_end, "");
        *cursor_pos = new_cursor;
    }
}

pub fn move_cursor_left(cursor_pos: &mut usize) {
    *cursor_pos = cursor_pos.saturating_sub(1);
}

pub fn move_cursor_right(cursor_pos: &mut usize, input: &str) {
    *cursor_pos = (*cursor_pos + 1).min(input.chars().count());
}

// ============================================================
// MAIN UI
// ============================================================

fn draw_ui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    input: &str,
    cursor_pos: usize,
    messages: &[String],
    scroll: u16,
    status: &str,
    is_responding: bool,
    menu: &SuggestionMenu,
    confirmation: Option<&PendingAction>,
) -> Result<(), io::Error> {
    terminal.draw(|frame| {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .split(frame.area());

        // Header
        draw_header(frame, root[0]);

        // Full-width chat
        draw_chat(frame, root[1], messages, scroll, is_responding);

        // Suggestion menu
        let filtered = filter_commands(input);
        if menu.is_open && !filtered.is_empty() {
            let selected = menu.selected.min(filtered.len().saturating_sub(1));
            draw_suggestion_menu(frame, root[1], &filtered, selected);
        }

        // Input + status
        draw_input(frame, root[2], input, cursor_pos, status, confirmation.is_some());
    })?;

    Ok(())
}

// ============================================================
// HEADER
// ============================================================

fn draw_header(frame: &mut ratatui::Frame, area: Rect) {
    let mut lines = Vec::new();

    for logo in CRUDO_LOGO {
        lines.push(Line::from(Span::styled(
            logo,
            Style::default()
                .fg(CRUDO_PURPLE)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(Span::styled(
        CRUDO_TAGLINE,
        Style::default()
            .fg(CRUDO_PURPLE_LIGHT)
            .add_modifier(Modifier::BOLD),
    )));

    let header = Paragraph::new(Text::from(lines))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(
                    Style::default().fg(CRUDO_PURPLE)
                ),
        );

    frame.render_widget(header, area);
}

// ============================================================
// CHAT
// ============================================================

fn draw_chat(
    frame: &mut ratatui::Frame,
    area: Rect,
    messages: &[String],
    scroll: u16,
    is_responding: bool,
) {
    let usable_width = area.width.saturating_sub(2);

    let mut chat_lines = Vec::new();

    if messages.is_empty() && !is_responding {
        chat_lines.push(Line::from(""));
        chat_lines.push(Line::from(Span::styled(
            "Welcome to Crudo",
            Style::default().fg(CRUDO_GRAY),
        )));
        chat_lines.push(Line::from(Span::styled(
            "Type a message to begin.",
            Style::default().fg(CRUDO_GRAY),
        )));
    } else {
        for (i, message) in messages.iter().enumerate() {
            if let Some(content) = message.strip_prefix("You: ") {
                add_user_message(
                    &mut chat_lines,
                    content,
                    usable_width,
                );
            } else if let Some(content) = message.strip_prefix("AI: ") {
                add_crudo_message(
                    &mut chat_lines,
                    content,
                    usable_width,
                );
            } else {
                add_plain_message(
                    &mut chat_lines,
                    message,
                    usable_width,
                );
            }

            if i + 1 < messages.len() {
                chat_lines.push(Line::from(""));
            }
        }

        if is_responding {
            if !messages.is_empty() {
                chat_lines.push(Line::from(""));
            }
            chat_lines.push(Line::from(Span::styled(
                "● Agent is responding...",
                Style::default()
                    .fg(CRUDO_PURPLE_LIGHT)
                    .add_modifier(Modifier::BOLD),
            )));
        }
    }

    // Scrollbar
    let total = chat_lines.len();

    let visible = area
        .height
        .saturating_sub(2) as usize;

    let max_position =
        total.saturating_sub(visible);

    let current_scroll =
        (scroll as usize).min(max_position);

    let chat = Paragraph::new(Text::from(chat_lines.clone()))
        .wrap(Wrap { trim: false })
        .scroll((current_scroll as u16, 0))
        .style(Style::default().bg(CRUDO_DARK))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(
                    Style::default().fg(CRUDO_PURPLE)
                ),
        );

    frame.render_widget(chat, area);

    let mut scrollbar_state =
        ScrollbarState::new(max_position + 1)
            .position(current_scroll)
            .viewport_content_length(visible);

    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(
                ScrollbarOrientation::VerticalRight
            )
            .thumb_style(
                Style::default().fg(CRUDO_PURPLE)
            )
            .track_style(
                Style::default()
                    .fg(Color::Rgb(28, 28, 38))
            ),
        area,
        &mut scrollbar_state,
    );
}

// ============================================================
// USER MESSAGE
// ============================================================

fn add_user_message(
    lines: &mut Vec<Line>,
    content: &str,
    available_width: u16,
) {
    add_profile_message(
        lines,
        content,
        available_width,
        "You",
        "❯",
        CRUDO_GRAY,
    );
}

// ============================================================
// CRUDO MESSAGE
// ============================================================

fn add_crudo_message(
    lines: &mut Vec<Line>,
    content: &str,
    available_width: u16,
) {
    add_profile_message(
        lines,
        content,
        available_width,
        "CRUDO",
        "◆",
        CRUDO_PURPLE,
    );
}

// ============================================================
// SHARED MESSAGE
// ============================================================

fn add_profile_message(
    lines: &mut Vec<Line>,
    content: &str,
    available_width: u16,
    title: &str,
    symbol: &str,
    border_color: Color,
) {
    // Compact symbol area so the arrow/diamond sits close to the bubble.
    let symbol_width = 1usize;
    let gap = 1usize;

    let usable =
        available_width as usize;

    let bubble_max = usable
        .saturating_sub(symbol_width + gap)
        .max(24);

    let longest = content
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    // Compact bubbles for short messages.
    let min_bubble =
        bubble_max.min(44).max(24);

    let desired =
        longest.saturating_add(4);

    let bubble_width =
        desired.max(min_bubble).min(bubble_max);

    let content_width =
        bubble_width.saturating_sub(4).max(1);

    let wrapped =
        wrap_message_text(
            content,
            content_width,
        );

    // Top border
    let title_text =
        format!("┌─ {} ", title);

    let remaining =
        bubble_width.saturating_sub(
            title_text.chars().count() + 1
        );

    let top =
        format!(
            "{}{}┐",
            title_text,
            "─".repeat(remaining)
        );

    let mut bubble = Vec::new();

    bubble.push(
        Line::from(
            Span::styled(
                top,
                Style::default()
                    .fg(border_color)
                    .add_modifier(
                        Modifier::BOLD
                    ),
            ),
        )
    );

    // Content
    for text in wrapped {
        let padding =
            content_width
                .saturating_sub(
                    text.chars().count()
                );

        bubble.push(
            Line::from(vec![
                Span::styled(
                    "│ ",
                    Style::default()
                        .fg(border_color),
                ),
                Span::styled(
                    text,
                    Style::default()
                        .fg(CRUDO_WHITE),
                ),
                Span::raw(
                    " ".repeat(padding)
                ),
                Span::styled(
                    " │",
                    Style::default()
                        .fg(border_color),
                ),
            ])
        );
    }

    // Bottom border
    bubble.push(
        Line::from(
            Span::styled(
                format!(
                    "└{}┘",
                    "─".repeat(
                        bubble_width
                            .saturating_sub(2)
                    )
                ),
                Style::default()
                    .fg(border_color),
            ),
        )
    );

    // Draw symbol + bubble.
    for (row, bubble_line) in
        bubble.iter().enumerate()
    {
        let mut row_spans = Vec::new();

        if row == 1 {
            row_spans.push(
                Span::styled(
                    format!(
                        "{:<width$}",
                        symbol,
                        width = symbol_width
                    ),
                    Style::default()
                        .fg(border_color)
                        .add_modifier(
                            Modifier::BOLD
                        ),
                )
            );
        } else {
            row_spans.push(
                Span::raw(
                    " ".repeat(symbol_width)
                )
            );
        }

        row_spans.push(
            Span::raw(" ")
        );

        row_spans.extend(
            bubble_line
                .spans
                .iter()
                .cloned()
        );

        lines.push(
            Line::from(row_spans)
        );
    }
}

// ============================================================
// PLAIN MESSAGE
// ============================================================

fn add_plain_message(
    lines: &mut Vec<Line>,
    content: &str,
    width: u16,
) {
    for text in wrap_message_text(
        content,
        width.saturating_sub(2) as usize,
    ) {
        lines.push(
            Line::from(
                Span::styled(
                    text,
                    Style::default()
                        .fg(CRUDO_WHITE),
                )
            )
        );
    }
}

// ============================================================
// INPUT + STATUS
// ============================================================

fn draw_input(
    frame: &mut ratatui::Frame,
    area: Rect,
    input: &str,
    cursor_pos: usize,
    status: &str,
    is_confirming: bool,
) {
    // Status is shown ABOVE the input box, similar to an agent activity line.
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    // --------------------------------------------------------
    // STATUS / AGENT ACTIVITY
    // --------------------------------------------------------

    let status_style = if status == "Ready" {
        Style::default().fg(CRUDO_GRAY)
    } else if status == "Error" {
        Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(CRUDO_PURPLE_LIGHT)
            .add_modifier(Modifier::BOLD)
    };

    let status_text = format!("● {}", status);

    let status_line = Paragraph::new(status_text)
        .style(status_style);

    frame.render_widget(status_line, sections[0]);

    // --------------------------------------------------------
    // COMMAND INPUT
    // --------------------------------------------------------

    let (text, input_color, cursor_offset) = if is_confirming {
        (
            "❯  Do you want to proceed? [y/n]".to_string(),
            CRUDO_PURPLE_LIGHT,
            29usize,
        )
    } else if input.is_empty() {
        (
            "❯  Type your message here...".to_string(),
            CRUDO_GRAY,
            0usize,
        )
    } else {
        (
            format!("❯  {}", input),
            CRUDO_WHITE,
            cursor_pos.min(input.chars().count()),
        )
    };

    let input_box = Paragraph::new(text)
        .style(
            Style::default()
                .fg(input_color)
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(
                    Style::default().fg(CRUDO_PURPLE)
                ),
        );

    frame.render_widget(
        input_box,
        sections[1],
    );

    // Cursor
    let cursor_x =
        sections[1].x
            + 1
            + 3
            + cursor_offset as u16;

    let cursor_y =
        sections[1].y + 1;

    frame.set_cursor_position((
        cursor_x.min(
            sections[1]
                .right()
                .saturating_sub(1)
        ),
        cursor_y,
    ));
}

// ============================================================
// TEXT WRAPPING
// ============================================================
//
// Important:
// This wraps at WORD boundaries whenever possible.
// So a word like:
//
// "specific"
//
// will not become:
//
// "speci"
// "fic"
//
// unless the word itself is longer than the available width.
// ============================================================

fn wrap_message_text(
    text: &str,
    width: usize,
) -> Vec<String> {
    let width = width.max(1);

    let mut result = Vec::new();

    for source_line in text.lines() {
        if source_line.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current = String::new();

        for word in source_line.split_whitespace() {
            let word_len =
                word.chars().count();

            if current.is_empty() {
                if word_len <= width {
                    current.push_str(word);
                } else {
                    // Only break a word when
                    // the word itself is longer
                    // than the available width.
                    let chars:
                        Vec<char> =
                        word.chars().collect();

                    let mut start = 0usize;

                    while start < chars.len() {
                        let end =
                            (start + width)
                                .min(chars.len());

                        result.push(
                            chars[start..end]
                                .iter()
                                .collect()
                        );

                        start = end;
                    }
                }
            } else {
                let current_len =
                    current.chars().count();

                if current_len
                    + 1
                    + word_len
                    <= width
                {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    result.push(
                        current.clone()
                    );

                    current.clear();

                    if word_len <= width {
                        current.push_str(word);
                    } else {
                        let chars:
                            Vec<char> =
                            word.chars().collect();

                        let mut start = 0usize;

                        while start < chars.len() {
                            let end =
                                (start + width)
                                    .min(chars.len());

                            result.push(
                                chars[start..end]
                                    .iter()
                                    .collect()
                            );

                            start = end;
                        }
                    }
                }
            }
        }

        if !current.is_empty() {
            result.push(current);
        }
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

// ============================================================
// SCROLLING
// ============================================================

fn chat_dimensions(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
) -> (u16, u16) {
    let size =
        terminal.size().unwrap_or_default();

    // Header = 8
    // Input/status = 4
    // Chat borders = 2
    let chat_height =
        size.height
            .saturating_sub(14);

    // Full width chat now.
    let chat_width =
        size.width.saturating_sub(2);

    (chat_width, chat_height)
}

// ------------------------------------------------------------
// Rendered height of one message
// ------------------------------------------------------------

fn message_rendered_lines(
    message: &str,
    chat_width: u16,
) -> usize {
    let usable =
        chat_width as usize;

    if let Some(content) =
        message.strip_prefix("You: ")
    {
        profile_message_height(
            content,
            usable,
        )
    } else if let Some(content) =
        message.strip_prefix("AI: ")
    {
        profile_message_height(
            content,
            usable,
        )
    } else {
        wrap_message_text(
            message,
            usable.saturating_sub(2),
        )
        .len()
    }
}

// ------------------------------------------------------------
// Profile message height
// ------------------------------------------------------------

fn profile_message_height(
    content: &str,
    available_width: usize,
) -> usize {
    let symbol_width = 1usize;
    let gap = 1usize;

    let bubble_max =
        available_width
            .saturating_sub(
                symbol_width + gap
            )
            .max(24);

    let longest =
        content
            .lines()
            .map(|line|
                line.chars().count()
            )
            .max()
            .unwrap_or(0);

    let min_bubble =
        bubble_max.min(44).max(24);

    let bubble_width =
        longest
            .saturating_add(4)
            .max(min_bubble)
            .min(bubble_max);

    let content_width =
        bubble_width
            .saturating_sub(4)
            .max(1);

    let wrapped_lines =
        wrap_message_text(
            content,
            content_width,
        )
        .len();

    wrapped_lines + 2
}

// ------------------------------------------------------------
// Maximum scroll
// ------------------------------------------------------------

fn max_scroll(
    messages: &[String],
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    is_responding: bool,
) -> u16 {
    let (chat_width, chat_height) =
        chat_dimensions(terminal);

    let mut total = 0usize;

    for (index, message)
        in messages.iter().enumerate()
    {
        total +=
            message_rendered_lines(
                message,
                chat_width,
            );

        if index + 1
            < messages.len()
        {
            total += 1;
        }
    }

    if is_responding {
        if !messages.is_empty() {
            total += 1;
        }
        total += 1;
    }

    total
        .saturating_sub(
            chat_height as usize
        )
        .min(u16::MAX as usize)
        as u16
}

// ------------------------------------------------------------
// Scroll to bottom
// ------------------------------------------------------------

fn scroll_to_bottom(
    messages: &[String],
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    scroll: &mut u16,
    is_responding: bool,
) {
    *scroll =
        max_scroll(
            messages,
            terminal,
            is_responding,
        );
}

// ------------------------------------------------------------
// Handle scroll event
// ------------------------------------------------------------

fn handle_scroll_event(
    event: &Event,
    messages: &[String],
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    scroll: &mut u16,
    auto_scroll: &mut bool,
    is_responding: bool,
) -> bool {
    let max = max_scroll(messages, terminal, is_responding);
    match event {
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => {
                *scroll = scroll.saturating_sub(3);
                *auto_scroll = *scroll >= max;
                true
            }
            MouseEventKind::ScrollDown => {
                *scroll = (*scroll + 3).min(max);
                *auto_scroll = *scroll >= max;
                true
            }
            _ => false,
        },
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Up => {
                *scroll = scroll.saturating_sub(1);
                *auto_scroll = *scroll >= max;
                true
            }
            KeyCode::Down => {
                *scroll = (*scroll + 1).min(max);
                *auto_scroll = *scroll >= max;
                true
            }
            KeyCode::PageUp => {
                *scroll = scroll.saturating_sub(10);
                *auto_scroll = *scroll >= max;
                true
            }
            KeyCode::PageDown => {
                *scroll = (*scroll + 10).min(max);
                *auto_scroll = *scroll >= max;
                true
            }
            KeyCode::End => {
                *scroll = max;
                *auto_scroll = true;
                true
            }
            _ => false,
        },
        Event::Resize(_, _) => {
            if *auto_scroll {
                *scroll = max;
            } else {
                *scroll = (*scroll).min(max);
            }
            true
        }
        _ => false,
    }
}

// ============================================================
// COMMAND HANDLING
// ============================================================

fn handle_command(
    command: &str,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    messages: &mut Vec<String>,
    scroll: &mut u16,
    status: &mut String,
) -> Result<bool, io::Error> {
    match command {
        "/clear" => {
            messages.clear();
            *status = String::from("Ready");
            Ok(true)
        }

        "/help" => {
            messages.push(
                "Crudo Commands".to_string()
            );

            messages.push(
                "".to_string()
            );

            messages.push(
                "/help    Show help".to_string()
            );

            messages.push(
                "/clear   Clear chat".to_string()
            );

            messages.push(
                "/pwd     Show current directory"
                    .to_string()
            );

            messages.push(
                "/ls      List files".to_string()
            );

            messages.push(
                "/read    Read a file".to_string()
            );

            messages.push(
                "/exit    Exit Crudo".to_string()
            );

            *status = String::from("Ready");
            Ok(true)
        }

        "/pwd" => {
            *status = String::from("Checking current directory...");
            draw_ui(
                terminal,
                "",
                0,
                messages,
                *scroll,
                status.as_str(),
                false,
                &SuggestionMenu::default(),
                None,
            )?;

            match std::env::current_dir() {
                Ok(path) => {
                    messages.push(
                        "Current directory:"
                            .to_string()
                    );

                    messages.push(
                        path.display()
                            .to_string()
                    );
                }

                Err(error) => {
                    messages.push(
                        format!(
                            "Error getting directory: {}",
                            error
                        )
                    );
                }
            }

            *status = String::from("Ready");
            Ok(true)
        }

        "/ls" => {
            *status = String::from("Listing files...");
            draw_ui(
                terminal,
                "",
                0,
                messages,
                *scroll,
                status.as_str(),
                false,
                &SuggestionMenu::default(),
                None,
            )?;

            list_directory(messages);

            *status = String::from("Ready");
            Ok(true)
        }

        "/read" => {
            messages.push(
                "Usage: /read <file-path>"
                    .to_string()
            );

            messages.push(
                "Example: /read Cargo.toml"
                    .to_string()
            );

            *status = String::from("Ready");
            Ok(true)
        }

        _ => {
            if let Some(path) =
                command.strip_prefix("/read ")
            {
                let path = path.trim();

                *status = format!("Reading file: {}", path);

                // Draw once before the synchronous file operation so the
                // activity line is visible to the user.
                draw_ui(
                    terminal,
                    "",
                    0,
                    messages,
                    *scroll,
                    status.as_str(),
                    false,
                    &SuggestionMenu::default(),
                    None,
                )?;

                read_file(
                    path,
                    messages,
                );

                *status = format!("Read file: {}", path);

                return Ok(true);
            }

            Ok(false)
        }
    }
}

// ============================================================
// LS
// ============================================================

fn list_directory(
    messages: &mut Vec<String>,
) {
    messages.push(
        "Here are the files in the current directory:"
            .to_string()
    );

    match fs::read_dir(".") {
        Ok(entries) => {
            let mut items =
                Vec::new();

            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path =
                            entry.path();

                        let name =
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();

                        if path.is_dir() {
                            items.push(
                                format!(
                                    "├── {}/",
                                    name
                                )
                            );
                        } else {
                            items.push(
                                format!(
                                    "├── {}",
                                    name
                                )
                            );
                        }
                    }

                    Err(error) => {
                        messages.push(
                            format!(
                                "Error reading entry: {}",
                                error
                            )
                        );
                    }
                }
            }

            items.sort();

            messages.extend(items);
        }

        Err(error) => {
            messages.push(
                format!(
                    "Error listing directory: {}",
                    error
                )
            );
        }
    }
}

// ============================================================
// READ FILE
// ============================================================

fn read_file(
    file_path: &str,
    messages: &mut Vec<String>,
) {
    if file_path.is_empty() {
        messages.push(
            "Usage: /read <file-path>"
                .to_string()
        );

        return;
    }

    let path =
        Path::new(file_path);

    if path.is_absolute() {
        messages.push(
            "Error: Only relative file paths are allowed."
                .to_string()
        );

        return;
    }

    if path.components().any(
        |component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        },
    ) {
        messages.push(
            "Error: Path must stay inside the Crudo project."
                .to_string()
        );

        return;
    }

    match fs::read_to_string(path) {
        Ok(content) => {
            messages.push(
                format!(
                    "--- {} ---",
                    file_path
                )
            );

            messages.push(content);

            messages.push(
                format!(
                    "--- End of {} ---",
                    file_path
                )
            );
        }

        Err(error) => {
            messages.push(
                format!(
                    "Error reading '{}': {}",
                    file_path,
                    error
                )
            );
        }
    }
}

// ============================================================
// KEYBOARD / MOUSE EVENTS
// ============================================================

fn handle_event(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    input: &mut String,
    cursor_pos: &mut usize,
    messages: &mut Vec<String>,
    scroll: &mut u16,
    status: &mut String,
    auto_scroll: &mut bool,
    generation: &mut Option<GenerationTask>,
    menu: &mut SuggestionMenu,
    history: &mut CommandHistory,
    pending_confirmation: &mut Option<PendingAction>,
    needs_redraw: &mut bool,
) -> Result<bool, io::Error> {
    let poll_timeout = if generation.is_some() {
        Duration::from_millis(15)
    } else {
        Duration::from_millis(100)
    };

    if !event::poll(poll_timeout)? {
        return Ok(false);
    }

    let event = event::read()?;
    *needs_redraw = true;

    let filtered = filter_commands(input);
    let menu_active = menu.is_open && !filtered.is_empty();

    let is_responding = generation.is_some();
    let is_up_or_down = matches!(
        event,
        Event::Key(key) if key.kind == KeyEventKind::Press && (key.code == KeyCode::Up || key.code == KeyCode::Down)
    );

    if !is_up_or_down
        && handle_scroll_event(
            &event,
            messages,
            terminal,
            scroll,
            auto_scroll,
            is_responding,
        )
    {
        return Ok(false);
    }

    match event {
        // ----------------------------------------------------
        // MOUSE
        // ----------------------------------------------------

        Event::Mouse(_) => {
            Ok(false)
        }

        // ----------------------------------------------------
        // KEYBOARD
        // ----------------------------------------------------

        Event::Key(key) => {
            if key.kind
                != KeyEventKind::Press
            {
                return Ok(false);
            }

            // ----------------------------------------------------
            // CTRL+C HANDLING
            // ----------------------------------------------------
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            {
                if let Some(task) = generation.take() {
                    task.cancel();
                    if let Some(last) = messages.last() {
                        if last == "AI: " {
                            messages.pop();
                        }
                    }
                    *status = String::from("Ready");
                    if *auto_scroll {
                        scroll_to_bottom(messages, terminal, scroll, false);
                    }
                    return Ok(false);
                } else {
                    return Ok(true);
                }
            }

            // ----------------------------------------------------
            // PERMISSION CONFIRMATION PROMPT HANDLING
            // ----------------------------------------------------
            if pending_confirmation.is_some() {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        if let Some(action) = pending_confirmation.take() {
                            match action {
                                PendingAction::Clear => {
                                    messages.clear();
                                    *status = String::from("Ready");
                                    if *auto_scroll {
                                        scroll_to_bottom(messages, terminal, scroll, false);
                                    }
                                }
                                PendingAction::Custom(_) => {
                                    *status = String::from("Ready");
                                }
                            }
                        }
                        return Ok(false);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        pending_confirmation.take();
                        if let Some(last) = messages.last() {
                            if last == "Do you want to proceed? [y/n]" {
                                messages.pop();
                            }
                        }
                        messages.push("Action cancelled.".to_string());
                        *status = String::from("Ready");
                        if *auto_scroll {
                            scroll_to_bottom(messages, terminal, scroll, false);
                        }
                        return Ok(false);
                    }
                    KeyCode::Up => {
                        handle_scroll_event(
                            &event,
                            messages,
                            terminal,
                            scroll,
                            auto_scroll,
                            is_responding,
                        );
                        return Ok(false);
                    }
                    KeyCode::Down => {
                        handle_scroll_event(
                            &event,
                            messages,
                            terminal,
                            scroll,
                            auto_scroll,
                            is_responding,
                        );
                        return Ok(false);
                    }
                    _ => {
                        return Ok(false);
                    }
                }
            }

            match key.code {
                // ------------------------------------------------
                // NAVIGATION (Left / Right)
                // ------------------------------------------------

                KeyCode::Left => {
                    move_cursor_left(cursor_pos);
                    return Ok(false);
                }

                KeyCode::Right => {
                    move_cursor_right(cursor_pos, input);
                    return Ok(false);
                }

                // ------------------------------------------------
                // TYPE
                // ------------------------------------------------

                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) && (c == 'w' || c == 'W') {
                        delete_word_before_cursor(input, cursor_pos);
                        let new_filtered = filter_commands(input);
                        if !new_filtered.is_empty() {
                            menu.is_open = true;
                            menu.selected = menu.selected.min(new_filtered.len().saturating_sub(1));
                        } else {
                            menu.is_open = false;
                            menu.selected = 0;
                        }
                    } else if c == '\x08' || c == '\u{8}' || c == '\x17' || c == '\u{7f}' {
                        if key.modifiers.contains(KeyModifiers::CONTROL) || c == '\x17' {
                            delete_word_before_cursor(input, cursor_pos);
                        } else {
                            delete_char_before_cursor(input, cursor_pos);
                        }
                        let new_filtered = filter_commands(input);
                        if !new_filtered.is_empty() {
                            menu.is_open = true;
                            menu.selected = menu.selected.min(new_filtered.len().saturating_sub(1));
                        } else {
                            menu.is_open = false;
                            menu.selected = 0;
                        }
                    } else if !c.is_control() {
                        insert_char_at_cursor(input, cursor_pos, c);
                        let new_filtered = filter_commands(input);
                        if !new_filtered.is_empty() {
                            menu.is_open = true;
                            menu.selected = menu.selected.min(new_filtered.len().saturating_sub(1));
                        } else {
                            menu.is_open = false;
                            menu.selected = 0;
                        }
                    }
                }

                // ------------------------------------------------
                // BACKSPACE
                // ------------------------------------------------

                KeyCode::Backspace => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        delete_word_before_cursor(input, cursor_pos);
                    } else {
                        delete_char_before_cursor(input, cursor_pos);
                    }
                    let new_filtered = filter_commands(input);
                    if !new_filtered.is_empty() {
                        menu.is_open = true;
                        menu.selected = menu.selected.min(new_filtered.len().saturating_sub(1));
                    } else {
                        menu.is_open = false;
                        menu.selected = 0;
                    }
                }

                // ------------------------------------------------
                // NAVIGATION (Up / Down)
                // ------------------------------------------------

                KeyCode::Up => {
                    if menu_active {
                        if menu.selected > 0 {
                            menu.selected -= 1;
                        } else {
                            menu.selected = filtered.len().saturating_sub(1);
                        }
                        return Ok(false);
                    }

                    let max = max_scroll(messages, terminal, is_responding);
                    if *scroll < max {
                        handle_scroll_event(
                            &event,
                            messages,
                            terminal,
                            scroll,
                            auto_scroll,
                            is_responding,
                        );
                        return Ok(false);
                    }

                    if let Some(prev) = history.up(input) {
                        *input = prev;
                        *cursor_pos = input.chars().count();
                        menu.is_open = false;
                        menu.selected = 0;
                        return Ok(false);
                    }

                    handle_scroll_event(
                        &event,
                        messages,
                        terminal,
                        scroll,
                        auto_scroll,
                        is_responding,
                    );
                    return Ok(false);
                }

                KeyCode::Down => {
                    if menu_active {
                        menu.selected = (menu.selected + 1) % filtered.len();
                        return Ok(false);
                    }

                    let max = max_scroll(messages, terminal, is_responding);
                    if *scroll < max {
                        handle_scroll_event(
                            &event,
                            messages,
                            terminal,
                            scroll,
                            auto_scroll,
                            is_responding,
                        );
                        return Ok(false);
                    }

                    if let Some(next) = history.down() {
                        *input = next;
                        *cursor_pos = input.chars().count();
                        menu.is_open = false;
                        menu.selected = 0;
                        return Ok(false);
                    }

                    handle_scroll_event(
                        &event,
                        messages,
                        terminal,
                        scroll,
                        auto_scroll,
                        is_responding,
                    );
                    return Ok(false);
                }

                // ------------------------------------------------
                // ENTER
                // ------------------------------------------------

                KeyCode::Enter => {
                    if menu_active {
                        let selected_idx = menu.selected.min(filtered.len().saturating_sub(1));
                        *input = filtered[selected_idx].to_string();
                        *cursor_pos = input.chars().count();
                        menu.is_open = false;
                        menu.selected = 0;
                        return Ok(false);
                    }

                    if input.trim().is_empty() {
                        return Ok(false);
                    }

                    let prompt =
                        input.trim()
                            .to_string();

                    history.push(prompt.clone());

                    input.clear();
                    *cursor_pos = 0;
                    menu.is_open = false;
                    menu.selected = 0;

                    if is_sensitive_action(&prompt) {
                        *pending_confirmation = Some(PendingAction::Clear);
                        *status = String::from("Do you want to proceed? [y/n]");
                        messages.push("Do you want to proceed? [y/n]".to_string());
                        if *auto_scroll {
                            scroll_to_bottom(messages, terminal, scroll, false);
                        }
                        return Ok(false);
                    }

                    // Cancel ongoing generation immediately if any
                    if let Some(task) = generation.take() {
                        task.cancel();
                        if let Some(last) = messages.last() {
                            if last == "AI: " {
                                messages.pop();
                            }
                        }
                    }

                    // ------------------------------------------------
                    // COMMAND
                    // ------------------------------------------------

                    if prompt.starts_with('/') {
                        if prompt == "/exit" {
                            return Ok(true);
                        }

                        match handle_command(
                            &prompt,
                            terminal,
                            messages,
                            scroll,
                            status,
                        ) {
                            Ok(true) => {
                                scroll_to_bottom(
                                    messages,
                                    terminal,
                                    scroll,
                                    false,
                                );

                                return Ok(false);
                            }
                            Ok(false) => {}
                            Err(error) => {
                                *status = String::from("Error");
                                messages.push(
                                    format!(
                                        "AI: Error: {}",
                                        error
                                    )
                                );

                                scroll_to_bottom(
                                    messages,
                                    terminal,
                                    scroll,
                                    false,
                                );

                                return Ok(false);
                            }
                        }

                        messages.push(
                            format!(
                                "Unknown command: {}",
                                prompt
                            )
                        );

                        messages.push(
                            "Type /help to see available commands."
                                .to_string()
                        );

                        scroll_to_bottom(
                            messages,
                            terminal,
                            scroll,
                            false,
                        );

                        return Ok(false);
                    }

                    // ------------------------------------------------
                    // USER MESSAGE
                    // ------------------------------------------------

                    messages.push(
                        format!(
                            "You: {}",
                            prompt
                        )
                    );

                    *auto_scroll = true;
                    *status = String::from("Ready");
                    scroll_to_bottom(
                        messages,
                        terminal,
                        scroll,
                        true,
                    );

                    // ------------------------------------------------
                    // START STREAMING FROM MODEL
                    // ------------------------------------------------

                    *generation = Some(start_ollama_stream(messages));
                }

                // ------------------------------------------------
                // ESC
                // ------------------------------------------------

                KeyCode::Esc => {
                    if menu_active {
                        menu.is_open = false;
                        menu.selected = 0;
                        return Ok(false);
                    }

                    if let Some(task) = generation.take() {
                        task.cancel();
                    }
                    return Ok(true);
                }

                _ => {}
            }

            Ok(false)
        }

        Event::FocusGained
        | Event::FocusLost
        | Event::Paste(_)
        | Event::Resize(_, _) => {
            Ok(false)
        }
    }
}

// ============================================================
// OLLAMA STREAMING
// ============================================================

struct GenerationTask {
    rx: mpsc::Receiver<Result<String, String>>,
    cancel: Arc<AtomicBool>,
    has_started_ai_message: bool,
}

impl GenerationTask {
    fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn start_ollama_stream(
    messages: &[String],
) -> GenerationTask {
    let conversation =
        messages
            .iter()
            .filter(|message| {
                message.starts_with("You: ")
                    || (
                        message.starts_with("AI: ")
                            && message.len() > 4
                    )
            })
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");

    let prompt = format!(
        "You are Crudo, a local-first terminal AI agent.\n\n\
         You run locally through Ollama using the Qwen 2.5 7B model.\n\n\
         Your responsibilities:\n\
         - Be helpful and accurate.\n\
         - Give clear and concise answers.\n\
         - Remember that you are running inside the Crudo terminal.\n\
         - Do not claim to have internet access or external tools unless they are explicitly provided.\n\n\
         Conversation:\n{}\n\n\
         AI:",
        conversation
    );

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel);
    let (tx, rx) =
        mpsc::channel::<Result<String, String>>();

    thread::spawn(move || {
        let client =
            reqwest::blocking::Client::new();

        let body = json!({
            "model": "qwen2.5:7b",
            "prompt": prompt,
            "stream": true
        });

        if cancel_clone.load(Ordering::Relaxed) {
            return;
        }

        let response = match client
            .post("http://localhost:11434/api/generate")
            .json(&body)
            .send()
        {
            Ok(resp) => resp,
            Err(error) => {
                if !cancel_clone.load(Ordering::Relaxed) {
                    let _ = tx.send(Err(error.to_string()));
                }
                return;
            }
        };

        if cancel_clone.load(Ordering::Relaxed) {
            return;
        }

        if !response.status().is_success() {
            if !cancel_clone.load(Ordering::Relaxed) {
                let _ = tx.send(Err(format!(
                    "Ollama returned {}",
                    response.status()
                )));
            }
            return;
        }

        let reader =
            BufReader::new(response);

        for line in reader.lines() {
            if cancel_clone.load(Ordering::Relaxed) {
                break;
            }
            match line {
                Ok(l) => {
                    if tx.send(Ok(l)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    if !cancel_clone.load(Ordering::Relaxed) {
                        let _ = tx.send(Err(error.to_string()));
                    }
                    break;
                }
            }
        }
    });

    GenerationTask {
        rx,
        cancel,
        has_started_ai_message: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::StatefulWidget;

    #[test]
    fn test_scrollbar_reaches_true_bottom() {
        let area = Rect::new(0, 0, 10, 12);
        let visible = area.height.saturating_sub(2) as usize; // 10
        let total = 30;
        let max_position = total - visible; // 20

        // At bottom: scroll = max_position = 20
        let current_scroll = 20;
        let mut scrollbar_state = ScrollbarState::new(max_position + 1)
            .position(current_scroll)
            .viewport_content_length(visible);

        let mut buf = Buffer::empty(area);
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight);

        scrollbar.render(area, &mut buf, &mut scrollbar_state);

        // Column 9 is the scrollbar column.
        // Row 0 should be begin_symbol ("▲")
        // Row 11 should be end_symbol ("▼")
        // Row 10 (immediately above "▼") should be thumb ("█")
        let cell_bottom = buf.cell((9, 10)).unwrap();
        assert_eq!(
            cell_bottom.symbol(),
            "█",
            "Thumb must reach the true bottom touching ▼"
        );
    }

    #[test]
    fn test_scrollbar_at_top() {
        let area = Rect::new(0, 0, 10, 12);
        let visible = area.height.saturating_sub(2) as usize; // 10
        let total = 30;
        let max_position = total - visible; // 20

        // At top: scroll = 0
        let current_scroll = 0;
        let mut scrollbar_state = ScrollbarState::new(max_position + 1)
            .position(current_scroll)
            .viewport_content_length(visible);

        let mut buf = Buffer::empty(area);
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight);

        scrollbar.render(area, &mut buf, &mut scrollbar_state);

        // Row 1 (immediately below "▲") should be thumb ("█")
        let cell_top = buf.cell((9, 1)).unwrap();
        assert_eq!(
            cell_top.symbol(),
            "█",
            "Thumb must start at the top touching ▲"
        );
    }

    #[test]
    fn test_generation_task_cancel() {
        let (_tx, rx) = mpsc::channel::<Result<String, String>>();
        let cancel = Arc::new(AtomicBool::new(false));
        let task = GenerationTask {
            rx,
            cancel: Arc::clone(&cancel),
            has_started_ai_message: false,
        };

        assert!(!task.cancel.load(Ordering::Relaxed));
        task.cancel();
        assert!(task.cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn test_empty_ai_message_cleanup_on_interrupt() {
        let mut messages = vec![
            "You: First question".to_string(),
            "AI: First answer".to_string(),
            "You: Second question".to_string(),
            "AI: ".to_string(),
        ];

        // Simulate interrupt before chunks arrive
        if let Some(last) = messages.last() {
            if last == "AI: " {
                messages.pop();
            }
        }

        assert_eq!(messages.len(), 3);
        assert_eq!(messages.last().unwrap(), "You: Second question");
    }

    #[test]
    fn test_partial_ai_message_preserved_on_interrupt() {
        let mut messages = vec![
            "You: First question".to_string(),
            "AI: Partial answer".to_string(),
        ];

        // Simulate interrupt after chunks arrive
        if let Some(last) = messages.last() {
            if last == "AI: " {
                messages.pop();
            }
        }

        assert_eq!(messages.len(), 2);
        assert_eq!(messages.last().unwrap(), "AI: Partial answer");
    }

    #[test]
    fn test_slash_commands_available() {
        assert_eq!(
            SLASH_COMMANDS,
            ["/help", "/clear", "/pwd", "/ls", "/read", "/exit"]
        );
    }

    #[test]
    fn test_filter_commands_on_slash() {
        let commands = filter_commands("/");
        assert_eq!(
            commands,
            vec!["/help", "/clear", "/pwd", "/ls", "/read", "/exit"]
        );
    }

    #[test]
    fn test_filter_commands_as_typed() {
        assert_eq!(filter_commands("/h"), vec!["/help"]);
        assert_eq!(filter_commands("/c"), vec!["/clear"]);
        assert_eq!(filter_commands("/p"), vec!["/pwd"]);
        assert_eq!(filter_commands("/l"), vec!["/ls"]);
        assert_eq!(filter_commands("/r"), vec!["/read"]);
        assert_eq!(filter_commands("/e"), vec!["/exit"]);
        assert_eq!(filter_commands("/H"), vec!["/help"]);
        assert_eq!(filter_commands("/CLEAR"), vec!["/clear"]);
        assert!(filter_commands("/nonexistent").is_empty());
    }

    #[test]
    fn test_filter_commands_ignored_on_normal_input_or_arguments() {
        assert!(filter_commands("").is_empty());
        assert!(filter_commands("hello").is_empty());
        assert!(filter_commands("/read Cargo.toml").is_empty());
    }

    #[test]
    fn test_menu_navigation_both_directions() {
        let filtered = filter_commands("/");
        assert_eq!(filtered.len(), 6);
        let mut selected = 0;

        // Down in forward direction
        selected = (selected + 1) % filtered.len();
        assert_eq!(selected, 1);
        assert_eq!(filtered[selected], "/clear");

        selected = (selected + 1) % filtered.len();
        assert_eq!(selected, 2);
        assert_eq!(filtered[selected], "/pwd");

        // Up in reverse direction
        if selected > 0 {
            selected -= 1;
        } else {
            selected = filtered.len().saturating_sub(1);
        }
        assert_eq!(selected, 1);
        assert_eq!(filtered[selected], "/clear");

        // Up again to 0
        if selected > 0 {
            selected -= 1;
        } else {
            selected = filtered.len().saturating_sub(1);
        }
        assert_eq!(selected, 0);
        assert_eq!(filtered[selected], "/help");

        // Up wraps around to bottom
        if selected > 0 {
            selected -= 1;
        } else {
            selected = filtered.len().saturating_sub(1);
        }
        assert_eq!(selected, 5);
        assert_eq!(filtered[selected], "/exit");

        // Down wraps around to top
        selected = (selected + 1) % filtered.len();
        assert_eq!(selected, 0);
        assert_eq!(filtered[selected], "/help");
    }

    #[test]
    fn test_menu_enter_fills_selected_command() {
        let mut input = String::from("/");
        let filtered = filter_commands(&input);
        let mut menu = SuggestionMenu {
            is_open: true,
            selected: 4, // /read
        };

        // Simulate Enter when menu is active
        let selected_idx = menu.selected.min(filtered.len().saturating_sub(1));
        input = filtered[selected_idx].to_string();
        menu.is_open = false;
        menu.selected = 0;

        assert_eq!(input, "/read");
        assert!(!menu.is_open);
    }

    #[test]
    fn test_menu_esc_closes_menu() {
        let input = String::from("/");
        let filtered = filter_commands(&input);
        let mut menu = SuggestionMenu {
            is_open: true,
            selected: 1,
        };

        let menu_active = menu.is_open && !filtered.is_empty();
        assert!(menu_active);

        // Simulate Esc closing menu
        if menu_active {
            menu.is_open = false;
            menu.selected = 0;
        }

        assert!(!menu.is_open);
    }

    #[test]
    fn test_menu_rendering_highlight() {
        use ratatui::widgets::Widget;

        let area = Rect::new(0, 0, 24, 8);
        let mut buf = Buffer::empty(area);
        let filtered = filter_commands("/");
        let selected = 0;

        let mut lines = Vec::new();
        let inner_width = 22usize;

        for (i, cmd) in filtered.iter().enumerate() {
            let is_selected = i == selected;
            let prefix = if is_selected { "❯ " } else { "  " };
            let cmd_text = format!("{}{}", prefix, cmd);
            let padding = inner_width.saturating_sub(cmd_text.chars().count());
            let full_text = format!("{}{}", cmd_text, " ".repeat(padding));

            let style = if is_selected {
                Style::default()
                    .fg(CRUDO_WHITE)
                    .bg(CRUDO_PURPLE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(CRUDO_WHITE)
                    .bg(CRUDO_DARK)
            };

            lines.push(Line::from(Span::styled(full_text, style)));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(CRUDO_PURPLE))
            .style(Style::default().bg(CRUDO_DARK));

        let widget = Paragraph::new(Text::from(lines)).block(block);
        widget.render(area, &mut buf);

        // Row 1 is the first command row: "❯ /help"
        let cell = buf.cell((1, 1)).unwrap();
        assert_eq!(cell.symbol(), "❯");
        assert_eq!(cell.fg, CRUDO_WHITE);
        assert_eq!(cell.bg, CRUDO_PURPLE);

        // Row 2 is the second command row: "  /clear"
        let cell2 = buf.cell((1, 2)).unwrap();
        assert_eq!(cell2.symbol(), " ");
        assert_eq!(cell2.bg, CRUDO_DARK);
    }

    #[test]
    fn test_command_history_navigation() {
        let mut history = CommandHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.up(""), None);
        assert_eq!(history.down(), None);

        history.push("first command".to_string());
        history.push("second command".to_string());
        assert_eq!(history.len(), 2);

        // Up loads previous (most recent)
        assert_eq!(history.up(""), Some("second command".to_string()));
        // Up again loads older
        assert_eq!(history.up("second command"), Some("first command".to_string()));
        // Up at top returns None so scrolling can occur
        assert_eq!(history.up("first command"), None);

        // Down moves forward
        assert_eq!(history.down(), Some("second command".to_string()));
        // Down returns to draft
        assert_eq!(history.down(), Some("".to_string()));
        // Down at bottom returns None so scrolling can occur
        assert_eq!(history.down(), None);
    }

    #[test]
    fn test_command_history_preserves_draft() {
        let mut history = CommandHistory::new();
        history.push("cmd 1".to_string());
        history.push("cmd 2".to_string());

        // User typed something before pressing Up
        assert_eq!(history.up("my draft question"), Some("cmd 2".to_string()));
        assert_eq!(history.up("cmd 2"), Some("cmd 1".to_string()));
        assert_eq!(history.down(), Some("cmd 2".to_string()));
        // Restores draft question!
        assert_eq!(history.down(), Some("my draft question".to_string()));
    }

    #[test]
    fn test_command_history_consecutive_duplicates() {
        let mut history = CommandHistory::new();
        history.push("duplicate".to_string());
        history.push("duplicate".to_string());
        assert_eq!(history.len(), 1);

        history.push("other".to_string());
        assert_eq!(history.len(), 2);
        history.push("duplicate".to_string());
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_is_sensitive_action_detection() {
        // Harmless operations must NOT require confirmation
        assert!(!is_sensitive_action("/help"));
        assert!(!is_sensitive_action("/pwd"));
        assert!(!is_sensitive_action("/ls"));
        assert!(!is_sensitive_action("/read Cargo.toml"));
        assert!(!is_sensitive_action("/exit"));
        assert!(!is_sensitive_action("hello how are you?"));
        assert!(!is_sensitive_action("write a function to delete a node in linked list"));

        // Destructive operation /clear must require confirmation
        assert!(is_sensitive_action("/clear"));
        assert!(is_sensitive_action("/clear "));
    }

    #[test]
    fn test_confirmation_approval_clears_messages() {
        let mut messages = vec!["You: hello".to_string(), "AI: hi".to_string()];
        let mut pending = Some(PendingAction::Clear);
        let mut status = String::from("Do you want to proceed? [y/n]");
        assert_eq!(status, "Do you want to proceed? [y/n]");

        // Simulate approval (Y or Enter)
        if let Some(action) = pending.take() {
            match action {
                PendingAction::Clear => {
                    messages.clear();
                    status = String::from("Ready");
                }
                _ => {}
            }
        }

        assert!(messages.is_empty());
        assert_eq!(status, "Ready");
        assert!(pending.is_none());
    }

    #[test]
    fn test_confirmation_cancel_preserves_messages() {
        let mut messages = vec![
            "You: hello".to_string(),
            "AI: hi".to_string(),
            "Do you want to proceed? [y/n]".to_string(),
        ];
        let mut pending = Some(PendingAction::Clear);
        let mut status = String::from("Do you want to proceed? [y/n]");
        assert_eq!(status, "Do you want to proceed? [y/n]");

        // Simulate cancel (N or Esc)
        pending.take();
        if let Some(last) = messages.last() {
            if last == "Do you want to proceed? [y/n]" {
                messages.pop();
            }
        }
        messages.push("Action cancelled.".to_string());
        status = String::from("Ready");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], "You: hello");
        assert_eq!(messages[1], "AI: hi");
        assert_eq!(messages[2], "Action cancelled.");
        assert_eq!(status, "Ready");
        assert!(pending.is_none());
    }

    #[test]
    fn test_cursor_left_right_movement() {
        let input = "hello";
        let mut pos = 0;

        // Moving left at 0 stays at 0
        move_cursor_left(&mut pos);
        assert_eq!(pos, 0);

        // Moving right advances by 1
        move_cursor_right(&mut pos, input);
        assert_eq!(pos, 1);
        move_cursor_right(&mut pos, input);
        assert_eq!(pos, 2);

        // Advance to end
        move_cursor_right(&mut pos, input);
        move_cursor_right(&mut pos, input);
        move_cursor_right(&mut pos, input);
        assert_eq!(pos, 5);

        // Moving right at end stays at end
        move_cursor_right(&mut pos, input);
        assert_eq!(pos, 5);

        // Moving left steps back
        move_cursor_left(&mut pos);
        assert_eq!(pos, 4);
    }

    #[test]
    fn test_insert_char_at_cursor() {
        let mut input = String::new();
        let mut pos = 0;

        // Type at end
        insert_char_at_cursor(&mut input, &mut pos, 'a');
        insert_char_at_cursor(&mut input, &mut pos, 'c');
        assert_eq!(input, "ac");
        assert_eq!(pos, 2);

        // Move left and insert in middle
        move_cursor_left(&mut pos);
        assert_eq!(pos, 1);
        insert_char_at_cursor(&mut input, &mut pos, 'b');
        assert_eq!(input, "abc");
        assert_eq!(pos, 2);

        // Move to start and insert at beginning
        pos = 0;
        insert_char_at_cursor(&mut input, &mut pos, 'X');
        assert_eq!(input, "Xabc");
        assert_eq!(pos, 1);

        // Unicode multi-byte characters
        insert_char_at_cursor(&mut input, &mut pos, '🦀');
        assert_eq!(input, "X🦀abc");
        assert_eq!(pos, 2);
    }

    #[test]
    fn test_delete_char_before_cursor() {
        let mut input = String::from("hello");
        let mut pos = 5;

        // Backspace at end
        delete_char_before_cursor(&mut input, &mut pos);
        assert_eq!(input, "hell");
        assert_eq!(pos, 4);

        // Backspace in middle
        pos = 2; // after 'e'
        delete_char_before_cursor(&mut input, &mut pos);
        assert_eq!(input, "hll");
        assert_eq!(pos, 1);

        // Backspace at position 0 does nothing
        pos = 0;
        delete_char_before_cursor(&mut input, &mut pos);
        assert_eq!(input, "hll");
        assert_eq!(pos, 0);

        // Multi-byte Unicode handling
        let mut uni = String::from("a🦀b");
        let mut uni_pos = 2; // after '🦀'
        delete_char_before_cursor(&mut uni, &mut uni_pos);
        assert_eq!(uni, "ab");
        assert_eq!(uni_pos, 1);
    }

    #[test]
    fn test_delete_word_before_cursor() {
        // Simple word at end
        let mut input = String::from("hello world");
        let mut pos = 11;
        delete_word_before_cursor(&mut input, &mut pos);
        assert_eq!(input, "hello ");
        assert_eq!(pos, 6);

        // Trailing whitespace followed by word
        let mut input2 = String::from("hello world   ");
        let mut pos2 = 14;
        delete_word_before_cursor(&mut input2, &mut pos2);
        assert_eq!(input2, "hello ");
        assert_eq!(pos2, 6);

        // Cursor in middle of sentence after word
        let mut input3 = String::from("the quick brown fox");
        let mut pos3 = 15; // after "brown"
        delete_word_before_cursor(&mut input3, &mut pos3);
        assert_eq!(input3, "the quick  fox");
        assert_eq!(pos3, 10);

        // Single word
        let mut input4 = String::from("hello");
        let mut pos4 = 5;
        delete_word_before_cursor(&mut input4, &mut pos4);
        assert_eq!(input4, "");
        assert_eq!(pos4, 0);

        // Empty or 0 position does nothing
        let mut input5 = String::from("hello");
        let mut pos5 = 0;
        delete_word_before_cursor(&mut input5, &mut pos5);
        assert_eq!(input5, "hello");
        assert_eq!(pos5, 0);

        // Whitespace only
        let mut input6 = String::from("   ");
        let mut pos6 = 3;
        delete_word_before_cursor(&mut input6, &mut pos6);
        assert_eq!(input6, "");
        assert_eq!(pos6, 0);
    }

    #[test]
    fn test_edit_spelling_mistake_flow() {
        // Simulate typing: "What is the speeling mistake?"
        let mut input = String::from("What is the speeling mistake?");
        let mut pos = input.chars().count(); // 29

        // Move cursor back to right after the typo "speeling"
        // "What is the speeling" is 20 chars
        while pos > 20 {
            move_cursor_left(&mut pos);
        }
        assert_eq!(pos, 20);

        // Delete "speeling" with Ctrl+Backspace
        delete_word_before_cursor(&mut input, &mut pos);
        assert_eq!(input, "What is the  mistake?");
        assert_eq!(pos, 12);

        // Type the correct word "spelling"
        for c in "spelling".chars() {
            insert_char_at_cursor(&mut input, &mut pos, c);
        }
        assert_eq!(input, "What is the spelling mistake?");
        assert_eq!(pos, 20);

        // Or edit with single-character backspace in the middle:
        // Move to inside "spelling" (between 'l' and 'l')
        move_cursor_left(&mut pos); // after 'g' -> after 'n'
        move_cursor_left(&mut pos); // after 'i'
        move_cursor_left(&mut pos); // after 2nd 'l'
        delete_char_before_cursor(&mut input, &mut pos);
        assert_eq!(input, "What is the speling mistake?");
        // Re-insert 'l'
        insert_char_at_cursor(&mut input, &mut pos, 'l');
        assert_eq!(input, "What is the spelling mistake?");
    }
}