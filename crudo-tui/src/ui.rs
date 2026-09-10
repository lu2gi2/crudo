use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

pub const CRUDO_LOGO_RAW: [(&str, &str); 8] = [
    ("      ৹ •        ", ""),
    ("   • ৹           ", ""),
    (
        "    ৹            ",
        "  ██████╗ ██████╗ ██╗   ██╗██████╗  ██████╗ ",
    ),
    (
        "   ▐▐            ",
        " ██╔════╝ ██╔══██╗██║   ██║██╔══██╗██╔═══██╗",
    ),
    (
        "   ▐▐ ╱║ ╱║      ",
        " ██║      ██████╔╝██║   ██║██║  ██║██║ ⚙ ██║",
    ),
    (
        "  ╱█████████     ",
        " ██║      ██╔══██╗██║   ██║██║  ██║██║   ██║",
    ),
    (
        "  ███⚙ ██⚙ ███   ",
        " ╚██████╗ ██║  ██║╚██████╔╝██████╔╝╚██████╔╝",
    ),
    (
        " ██████⚙ █████   ",
        "  ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚═════╝  ╚═════╝ ",
    ),
];

#[allow(dead_code)]
pub fn logo_dimensions() -> (u16, u16) {
    use unicode_width::UnicodeWidthStr;
    let height = CRUDO_LOGO_RAW.len() as u16;
    let mut max_width = 0;
    for (mascot, letters) in &CRUDO_LOGO_RAW {
        let w = (mascot.width() + letters.width()) as u16;
        if w > max_width {
            max_width = w;
        }
    }
    (max_width, height)
}

#[allow(dead_code)]
pub fn render_logo_lines<'a>() -> Vec<Line<'a>> {
    CRUDO_LOGO_RAW
        .iter()
        .map(|(mascot, letters)| {
            Line::from(vec![
                Span::styled(*mascot, Style::default().fg(crate::theme::PRIMARY_ACCENT)),
                Span::styled(*letters, Style::default().fg(crate::theme::PRIMARY_ACCENT)),
            ])
        })
        .collect()
}

pub fn compact_header_height(available_height: u16) -> u16 {
    let min_required_for_rest = 3 /* input */ + 1 /* status */ + 5 /* min chat */;
    let max_header_h = available_height.saturating_sub(min_required_for_rest);
    3u16.min(max_header_h)
}

pub fn draw(f: &mut Frame, _app: &mut App) {
    let header_height = if _app.messages.is_empty() {
        1
    } else {
        compact_header_height(f.size().height)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height), // Header
            Constraint::Min(5),                // Main area (Chat + Activity)
            Constraint::Length(3),             // Input area
            Constraint::Length(1),             // Status bar
        ])
        .split(f.size());

    let (status_icon, status_label, status_color, cancel_text) = match _app.agent_state {
        crate::app::AgentState::Idle => ("●", "READY", Color::Green, "Exit"),
        crate::app::AgentState::Thinking => ("●", "THINKING", Color::Yellow, "Cancel"),
        crate::app::AgentState::ExecutingTool => ("●", "EXECUTING", Color::Yellow, "Cancel"),
        crate::app::AgentState::Streaming => {
            ("●", "STREAMING", crate::theme::PRIMARY_ACCENT, "Cancel")
        }
        crate::app::AgentState::Cancelling => ("●", "CANCELLING", Color::Red, "Cancel"),
        crate::app::AgentState::Error => ("●", "ERROR", Color::Red, "Exit"),
        crate::app::AgentState::Offline => ("●", "OFFLINE", Color::DarkGray, "Exit"),
    };

    // Header
    if _app.messages.is_empty() {
        let header_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        let title =
            Paragraph::new("CRUDO").style(Style::default().fg(crate::theme::PRIMARY_ACCENT));

        let header_status = format!("{} LOCAL / {}", status_icon, status_label);
        let local_ready = Paragraph::new(header_status)
            .style(Style::default().fg(status_color))
            .alignment(ratatui::layout::Alignment::Right);

        f.render_widget(title, header_layout[0]);
        f.render_widget(local_ready, header_layout[1]);
    } else {
        f.render_widget(Clear, chunks[0]);

        let header_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(crate::theme::PRIMARY_ACCENT));

        let brand_text = Line::from(vec![Span::styled(
            "C R U D ⚙",
            Style::default()
                .fg(crate::theme::PRIMARY_ACCENT)
                .add_modifier(Modifier::BOLD),
        )]);

        let header_widget = Paragraph::new(brand_text)
            .block(header_block)
            .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(header_widget, chunks[0]);
    }

    // Main area
    let main_constraints = if _app.show_activity_panel {
        vec![Constraint::Percentage(70), Constraint::Percentage(30)]
    } else {
        vec![Constraint::Percentage(100)]
    };

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(main_constraints)
        .split(chunks[1]);

    // Chat
    let mut chat_title = String::from("CHAT");
    if _app.focus == crate::app::Focus::Chat {
        chat_title = format!("▶ {}", chat_title);
    }
    if _app.chat_scroll.unseen_items > 0 {
        chat_title.push_str(&format!(" [↓ {} new]", _app.chat_scroll.unseen_items));
    }
    let chat_block = Block::default()
        .title(chat_title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(crate::theme::PRIMARY_ACCENT));

    let chat_width = main_chunks[0].width.saturating_sub(2) as usize;
    let chat_lines = build_chat_lines(
        &_app.messages,
        chat_width,
        _app.agent_state,
        _app.spinner.frame(),
    );

    let chat_height = main_chunks[0].height.saturating_sub(2);
    let chat_content_lines = chat_lines.len() as u16;
    let chat_max_offset = chat_content_lines.saturating_sub(chat_height);
    _app.chat_scroll.set_viewport(chat_max_offset, chat_height);

    let chat = Paragraph::new(chat_lines)
        .block(chat_block)
        .scroll((_app.chat_scroll.offset, 0));
    f.render_widget(chat, main_chunks[0]);

    // Activity
    if _app.show_activity_panel {
        let mut act_title = String::from("ACTIVITY");
        if _app.focus == crate::app::Focus::Activity {
            act_title = format!("▶ {}", act_title);
        }
        if _app.activity_scroll.unseen_items > 0 {
            act_title.push_str(&format!(" [↓ {} new]", _app.activity_scroll.unseen_items));
        }
        let activity_block = Block::default()
            .title(act_title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(crate::theme::PRIMARY_ACCENT));
        let act_width = main_chunks[1].width.saturating_sub(2) as usize;
        let mut activity_lines = Vec::new();

        if _app.activities.is_empty() {
            activity_lines.push(Line::from(Span::styled(
                "No tool activity",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            let spinner_frame = _app.spinner.frame();
            for act in &_app.activities {
                let (icon, icon_color) = match act.status {
                    crate::activity::ActivityStatus::Pending => ("○", Color::DarkGray),
                    crate::activity::ActivityStatus::Running => {
                        (spinner_frame, crate::theme::PRIMARY_ACCENT)
                    }
                    crate::activity::ActivityStatus::Completed => ("✓", Color::Green),
                    crate::activity::ActivityStatus::Failed => ("✗", Color::Red),
                    crate::activity::ActivityStatus::Cancelled => ("⊘", Color::DarkGray),
                };

                let name = act.display_name();
                let status_str = act.status_text();
                let status_len = status_str.chars().count();
                let icon_len = 2; // icon + 1 trailing space
                let min_padding = 2;

                let max_name_len = act_width.saturating_sub(icon_len + min_padding + status_len);
                let (display_name, name_len) =
                    if max_name_len > 0 && name.chars().count() > max_name_len {
                        let mut trunc: String =
                            name.chars().take(max_name_len.saturating_sub(1)).collect();
                        trunc.push('…');
                        let count = trunc.chars().count();
                        (trunc, count)
                    } else {
                        let count = name.chars().count();
                        (name, count)
                    };

                let padding = act_width
                    .saturating_sub(icon_len + name_len + status_len)
                    .max(min_padding);
                let spacer = " ".repeat(padding);

                let (name_style, status_style) = match act.status {
                    crate::activity::ActivityStatus::Running => (
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                        Style::default()
                            .fg(crate::theme::PRIMARY_ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    crate::activity::ActivityStatus::Completed => (
                        Style::default().fg(Color::White),
                        Style::default().fg(Color::DarkGray),
                    ),
                    crate::activity::ActivityStatus::Failed => (
                        Style::default().fg(Color::Red),
                        Style::default().fg(Color::Red),
                    ),
                    crate::activity::ActivityStatus::Cancelled => (
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::DarkGray),
                    ),
                    crate::activity::ActivityStatus::Pending => (
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::DarkGray),
                    ),
                };

                let row_spans = vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                    Span::styled(display_name, name_style),
                    Span::raw(spacer),
                    Span::styled(status_str, status_style),
                ];

                activity_lines.push(Line::from(row_spans));

                // Sub-line details if available
                if act.status == crate::activity::ActivityStatus::Running {
                    if let Some(progress) = &act.progress {
                        let max_progress_len = act_width.saturating_sub(6);
                        let display_progress = if max_progress_len > 0
                            && progress.chars().count() > max_progress_len
                        {
                            let mut truncated: String = progress
                                .chars()
                                .take(max_progress_len.saturating_sub(3))
                                .collect();
                            truncated.push_str("...");
                            truncated
                        } else {
                            progress.clone()
                        };

                        activity_lines.push(Line::from(vec![
                            Span::styled(
                                "  └─ ",
                                Style::default().fg(crate::theme::PRIMARY_ACCENT),
                            ),
                            Span::styled(display_progress, Style::default().fg(Color::DarkGray)),
                        ]));
                    }
                } else if act.status == crate::activity::ActivityStatus::Failed {
                    if let Some(err) = &act.error {
                        let max_err_len = act_width.saturating_sub(6);
                        let display_err = if max_err_len > 0 && err.chars().count() > max_err_len {
                            let mut truncated: String =
                                err.chars().take(max_err_len.saturating_sub(3)).collect();
                            truncated.push_str("...");
                            truncated
                        } else {
                            err.clone()
                        };

                        activity_lines.push(Line::from(vec![
                            Span::styled("  └─ ", Style::default().fg(Color::Red)),
                            Span::styled(display_err, Style::default().fg(Color::Red)),
                        ]));
                    }
                } else if act.status == crate::activity::ActivityStatus::Cancelled {
                    if let Some(err) = &act.error {
                        let max_err_len = act_width.saturating_sub(6);
                        let display_err = if max_err_len > 0 && err.chars().count() > max_err_len {
                            let mut truncated: String =
                                err.chars().take(max_err_len.saturating_sub(3)).collect();
                            truncated.push_str("...");
                            truncated
                        } else {
                            err.clone()
                        };

                        activity_lines.push(Line::from(vec![
                            Span::styled("  └─ ", Style::default().fg(Color::DarkGray)),
                            Span::styled(display_err, Style::default().fg(Color::DarkGray)),
                        ]));
                    }
                }
            }
        }

        let act_height = main_chunks[1].height.saturating_sub(2);
        let act_content_lines = activity_lines.len() as u16;
        let act_max_offset = act_content_lines.saturating_sub(act_height);
        _app.activity_scroll
            .set_viewport(act_max_offset, act_height);

        let activity = Paragraph::new(activity_lines)
            .block(activity_block)
            .scroll((_app.activity_scroll.offset, 0));
        f.render_widget(activity, main_chunks[1]);
    }

    // Input area
    let mut input_title = String::from("INPUT");
    if _app.focus == crate::app::Focus::Input {
        input_title = format!("▶ {}", input_title);
    }
    let input_block = Block::default()
        .title(input_title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(crate::theme::PRIMARY_ACCENT));

    let input_text = if _app.input_state.value().is_empty() {
        Line::from(vec![Span::raw("> ")])
    } else {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::DarkGray)),
            Span::raw(_app.input_state.value()),
        ])
    };

    let input = Paragraph::new(input_text).block(input_block);
    f.render_widget(input, chunks[2]);

    // Set cursor
    if !_app.palette_state.is_open {
        f.set_cursor(
            chunks[2].x + 3 + _app.input_state.visual_cursor(), // +1 for border, +2 for "> "
            chunks[2].y + 1,                                    // +1 for border
        );
    }

    // Status bar
    let status_text = Line::from(vec![
        Span::raw("Model: Local │ "),
        Span::styled(
            format!("{} {}", status_icon, status_label),
            Style::default().fg(status_color),
        ),
        Span::raw(format!(" │ Ctrl+C {}", cancel_text)),
        Span::raw(" │ Ctrl+K Palette"),
    ]);
    let status_bar = Paragraph::new(status_text);
    f.render_widget(status_bar, chunks[3]);

    // Modal Overlays
    if let Some(perm) = &_app.pending_permission {
        render_permission_prompt(f, perm);
    } else if _app.palette_state.is_open {
        render_command_palette(f, _app);
    }
}

pub fn build_chat_lines<'a>(
    messages: &'a [crate::message::Message],
    chat_width: usize,
    agent_state: crate::app::AgentState,
    spinner_frame: &str,
) -> Vec<Line<'a>> {
    let mut chat_lines = Vec::new();

    if messages.is_empty() {
        chat_lines.push(Line::from(""));
        for (mascot, letters) in &CRUDO_LOGO_RAW {
            chat_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(*mascot, Style::default().fg(crate::theme::PRIMARY_ACCENT)),
                Span::styled(*letters, Style::default().fg(crate::theme::PRIMARY_ACCENT)),
            ]));
        }
    } else {
        for (i, msg) in messages.iter().enumerate() {
            // Check if we should render the animation before this message
            let is_last = i == messages.len() - 1;
            let is_assistant = msg.role == crate::message::Role::Assistant;
            let is_streaming = agent_state == crate::app::AgentState::Streaming;

            if is_last && is_assistant && is_streaming {
                chat_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner_frame),
                        Style::default().fg(crate::theme::PRIMARY_ACCENT),
                    ),
                    Span::raw("Streaming..."),
                ]));
                chat_lines.push(Line::from(""));
            }

            let (role_name, role_color) = match msg.role {
                crate::message::Role::User => ("You", Color::Green),
                crate::message::Role::Assistant => ("CRUDO", crate::theme::PRIMARY_ACCENT),
                crate::message::Role::System => ("SYSTEM", Color::Yellow),
            };

            chat_lines.push(Line::from(vec![Span::styled(
                role_name,
                Style::default().fg(role_color),
            )]));
            chat_lines.push(Line::from(vec![Span::styled(
                "────────────────────",
                Style::default().fg(Color::DarkGray),
            )]));

            if msg.role == crate::message::Role::Assistant {
                let md_lines = crate::markdown::render_markdown(&msg.content, chat_width);
                chat_lines.extend(md_lines);
            } else {
                for line in msg.content.lines() {
                    if chat_width > 0 && line.chars().count() > chat_width {
                        let chars: Vec<char> = line.chars().collect();
                        for chunk in chars.chunks(chat_width) {
                            let s: String = chunk.iter().collect();
                            chat_lines.push(Line::from(Span::raw(s)));
                        }
                    } else {
                        chat_lines.push(Line::from(Span::raw(line)));
                    }
                }
            }
            chat_lines.push(Line::from("")); // Spacing between messages
        }
    }

    // Add agent activity animation at the bottom of Chat if not streaming, or if streaming but no assistant message yet
    let has_streaming_assistant_msg = messages
        .last()
        .map_or(false, |m| m.role == crate::message::Role::Assistant)
        && agent_state == crate::app::AgentState::Streaming;

    if !has_streaming_assistant_msg {
        match agent_state {
            crate::app::AgentState::Thinking => {
                chat_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner_frame),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw("Thinking..."),
                ]));
                chat_lines.push(Line::from(""));
            }
            crate::app::AgentState::ExecutingTool => {
                chat_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner_frame),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw("Executing..."),
                ]));
                chat_lines.push(Line::from(""));
            }
            crate::app::AgentState::Streaming => {
                chat_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner_frame),
                        Style::default().fg(crate::theme::PRIMARY_ACCENT),
                    ),
                    Span::raw("Streaming..."),
                ]));
                chat_lines.push(Line::from(""));
            }
            crate::app::AgentState::Cancelling => {
                chat_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", spinner_frame),
                        Style::default().fg(Color::Red),
                    ),
                    Span::raw("Cancelling..."),
                ]));
                chat_lines.push(Line::from(""));
            }
            _ => {}
        }
    }

    chat_lines
}

pub fn render_command_palette(f: &mut Frame, app: &App) {
    use unicode_width::UnicodeWidthStr;

    let size = f.size();
    let area = centered_rect(65, 14, size);

    // Clear background
    f.render_widget(Clear, area);

    // Palette modal block
    let block = Block::default()
        .title(" COMMAND PALETTE ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(crate::theme::PRIMARY_ACCENT));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 15 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Search bar
            Constraint::Length(1), // Divider
            Constraint::Min(1),    // Commands list
        ])
        .split(inner);

    // 1. Search bar
    let search_line = Line::from(vec![
        Span::styled(
            "> ",
            Style::default()
                .fg(crate::theme::PRIMARY_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&app.palette_state.query, Style::default().fg(Color::White)),
    ]);
    f.render_widget(Paragraph::new(search_line), chunks[0]);

    // Position cursor in palette search bar
    let cursor_x = chunks[0].x + 2 + app.palette_state.visual_cursor();
    let cursor_x_clamped = cursor_x.min(chunks[0].x + chunks[0].width.saturating_sub(1));
    f.set_cursor(cursor_x_clamped, chunks[0].y);

    // 2. Divider
    let divider_char_count = chunks[1].width as usize;
    let divider = "─".repeat(divider_char_count);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            divider,
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[1],
    );

    // 3. Command list
    let commands = crate::palette::default_commands();
    let filtered = app.palette_state.filtered_commands(&commands);

    if filtered.is_empty() {
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            "  No matching commands",
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty_msg, chunks[2]);
    } else {
        let max_visible = chunks[2].height as usize;
        let selected = app.palette_state.selected_index;
        let scroll_start = if selected >= max_visible {
            selected.saturating_sub(max_visible - 1)
        } else {
            0
        };

        let mut lines = Vec::new();
        let total_width = chunks[2].width as usize;

        for (display_idx, cmd) in filtered
            .iter()
            .skip(scroll_start)
            .take(max_visible)
            .enumerate()
        {
            let actual_idx = scroll_start + display_idx;
            let is_selected = actual_idx == selected;

            let prefix = if is_selected { "▶ " } else { "  " };
            let name = cmd.name;
            let desc = format!(" - {}", cmd.description);
            let shortcut = match cmd.shortcut {
                Some(s) if !s.is_empty() => format!("[{}]", s),
                _ => String::new(),
            };

            let prefix_w = prefix.width();
            let name_w = name.width();
            let shortcut_w = shortcut.width();
            let fixed_w = prefix_w + name_w + shortcut_w + 1;

            let available_for_desc = total_width.saturating_sub(fixed_w);
            let truncated_desc: String = if available_for_desc > 0 {
                desc.chars().take(available_for_desc).collect()
            } else {
                String::new()
            };
            let desc_w = truncated_desc.as_str().width();

            let line_w = prefix_w + name_w + desc_w + shortcut_w;
            let padding_spaces = total_width.saturating_sub(line_w);
            let padding = " ".repeat(padding_spaces);

            if is_selected {
                lines.push(
                    Line::from(vec![
                        Span::styled(
                            prefix,
                            Style::default()
                                .fg(crate::theme::PRIMARY_ACCENT)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            name,
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(truncated_desc, Style::default().fg(Color::LightCyan)),
                        Span::raw(padding),
                        Span::styled(
                            shortcut,
                            Style::default()
                                .fg(crate::theme::PRIMARY_ACCENT)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                    .style(Style::default().bg(Color::Rgb(35, 30, 50))),
                );
            } else {
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(name, Style::default().fg(Color::White)),
                    Span::styled(truncated_desc, Style::default().fg(Color::DarkGray)),
                    Span::raw(padding),
                    Span::styled(shortcut, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }

        f.render_widget(Paragraph::new(lines), chunks[2]);
    }
}

pub fn render_permission_prompt(f: &mut Frame, perm: &crate::app::PendingPermission) {
    let size = f.size();
    let area = centered_rect(65, 13, size);

    // Clear background
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" PERMISSION REQUEST ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 5 || inner.width < 20 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tool & Mode
            Constraint::Length(1), // Summary
            Constraint::Length(1), // Reason
            Constraint::Length(1), // Blank separator
            Constraint::Length(1), // Buttons [ Allow ]  [ Deny ]
            Constraint::Length(1), // Divider / hint
            Constraint::Length(1), // Keyboard shortcuts hint
        ])
        .split(inner);

    // 1. Tool & Current Mode
    let tool_mode_line = Line::from(vec![
        Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            &perm.tool_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  │  "),
        Span::styled("Current Mode: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&perm.current_mode, Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(tool_mode_line), chunks[0]);

    // 2. Summary / Input
    let summary_line = Line::from(vec![
        Span::styled("Summary: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&perm.summary, Style::default().fg(Color::White)),
    ]);
    f.render_widget(Paragraph::new(summary_line), chunks[1]);

    // 3. Reason
    let reason_line = if let Some(reason) = &perm.reason {
        Line::from(vec![
            Span::styled("Reason: ", Style::default().fg(Color::DarkGray)),
            Span::styled(reason, Style::default().fg(Color::LightYellow)),
        ])
    } else {
        Line::from(vec![Span::styled(
            "Action requires authorization before proceeding.",
            Style::default().fg(Color::DarkGray),
        )])
    };
    f.render_widget(Paragraph::new(reason_line), chunks[2]);

    // 4. Blank separator handled by constraint

    // 5. Buttons
    let is_allow = perm.choice == crate::app::PermissionChoice::Allow;
    let allow_style = if is_allow {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let deny_style = if !is_allow {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let buttons_line = Line::from(vec![
        Span::raw("        "),
        Span::styled("  Allow  ", allow_style),
        Span::raw("        "),
        Span::styled("  Deny  ", deny_style),
    ]);
    f.render_widget(Paragraph::new(buttons_line), chunks[4]);

    // 6. Divider line
    let divider_char_count = chunks[5].width as usize;
    let divider = "─".repeat(divider_char_count);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            divider,
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[5],
    );

    // 7. Shortcut hints
    let hint_line = Line::from(vec![
        Span::styled("[←/→/Tab] ", Style::default().fg(Color::White)),
        Span::styled("Select  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Enter] ", Style::default().fg(Color::White)),
        Span::styled("Confirm  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[y/a] ", Style::default().fg(Color::Green)),
        Span::styled("Allow  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[n/d/Esc] ", Style::default().fg(Color::Red)),
        Span::styled("Deny", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(hint_line), chunks[6]);
}

pub fn centered_rect(width_pct: u16, height: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(r.height.saturating_sub(height) / 2),
            Constraint::Length(height.min(r.height)),
            Constraint::Min(0),
        ])
        .split(r);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AgentState;
    use crate::message::{Message, Role};

    #[test]
    fn test_wrapped_message_lines_contribute_to_scrollable_height() {
        let long_line = "A".repeat(50);
        let msg = Message::new(1, Role::User, long_line);
        let messages = vec![msg];

        // Chat width of 20 chars means 50 chars wrap into ceil(50/20) = 3 lines
        let width = 20;
        let lines = build_chat_lines(&messages, width, AgentState::Idle, "⠋");

        // Structure per message:
        // 1 header line (role)
        // 1 divider line ("─────...")
        // 3 content lines (20 + 20 + 10)
        // 1 spacing line ("")
        // Total = 6 lines
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn test_empty_state_chat_contains_logo() {
        let lines = build_chat_lines(&[], 80, AgentState::Idle, "⠋");
        // 1 blank spacer line + 8 logo lines = 9 lines
        assert_eq!(lines.len(), 9);
    }

    #[test]
    fn test_conversation_chat_does_not_contain_logo() {
        let msg = Message::new(1, Role::User, "hello".to_string());
        let messages = vec![msg];
        let lines = build_chat_lines(&messages, 80, AgentState::Idle, "⠋");
        // User message should have role "You", divider, content, spacer = 4 lines
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_logo_dimensions_and_artwork_integrity() {
        let (width, height) = logo_dimensions();
        assert_eq!(height, 8);
        assert_eq!(width, 61);

        // Verify gear characters are present in the art
        let gear_count = CRUDO_LOGO_RAW
            .iter()
            .map(|(m, l)| m.matches('⚙').count() + l.matches('⚙').count())
            .sum::<usize>();
        assert_eq!(gear_count, 4);
    }

    #[test]
    fn test_logo_horizontal_centering() {
        let (logo_width, _) = logo_dimensions();
        assert_eq!(logo_width, 61);

        // Normal wide terminal (e.g. 120 cols)
        let w = 120u16;
        let margin = w.saturating_sub(logo_width) / 2;
        assert_eq!(margin, 29);
        assert_eq!(margin + logo_width + (w - margin - logo_width), w);

        // Standard terminal (80 cols)
        let w = 80u16;
        let margin = w.saturating_sub(logo_width) / 2;
        assert_eq!(margin, 9);

        // Exactly logo width (61 cols)
        let w = 61u16;
        let margin = w.saturating_sub(logo_width) / 2;
        assert_eq!(margin, 0);

        // Narrower than logo (40 cols)
        let w = 40u16;
        let margin = w.saturating_sub(logo_width) / 2;
        assert_eq!(margin, 0);
        let render_width = logo_width.min(w.saturating_sub(margin));
        assert_eq!(render_width, 40);
    }

    #[test]
    fn test_compact_header_height() {
        // Tall terminal (30 rows)
        assert_eq!(compact_header_height(30), 3);

        // Standard terminal (24 rows)
        assert_eq!(compact_header_height(24), 3);

        // Constrained height terminal (11 rows)
        assert_eq!(compact_header_height(11), 2);

        // Tiny height terminal (8 rows)
        assert_eq!(compact_header_height(8), 0);
    }

    #[test]
    fn test_draw_empty_state_and_populated_state() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        // 1. In empty state, header has 1 row and contains "CRUDO" on the left
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row0_text: String = (0..100).map(|x| buffer.get(x, 0).symbol()).collect();
        assert!(row0_text.starts_with("CRUDO"));

        // 2. Add first valid message
        app.add_message(Role::User, "Hello CRUDO".to_string());
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer2 = terminal.backend().buffer().clone();

        // Compact header occupies rows 0..3 (height 3) and contains "C R U D ⚙"
        let row1_text: String = (0..100).map(|x| buffer2.get(x, 1).symbol()).collect();
        assert!(row1_text.contains("C R U D ⚙"));

        // Verify gear replaces the "O" and there is ONLY ONE gear
        assert_eq!(row1_text.matches('⚙').count(), 1);
        assert!(!row1_text.contains("⚙ C R U D O ⚙"));
        assert!(!row1_text.contains("⚙ CRUDO ⚙"));

        // Verify horizontal centering: "C R U D ⚙" should appear centered in the 100-col row
        let text_idx = row1_text.find("C R U D ⚙").expect("brand text found");
        // "│" is 3 bytes in UTF-8 + 45 spaces = byte index 48
        assert!(
            (46..=50).contains(&text_idx),
            "Expected centered index around 48, got {}",
            text_idx
        );

        // And chat area begins directly below the 3-row compact header (row 3 has top border of CHAT)
        let row3_text: String = (0..100).map(|x| buffer2.get(x, 3).symbol()).collect();
        assert!(row3_text.contains("CHAT"));

        // Chat content contains "You" and the user message
        let mut rows_text = String::new();
        for y in 3..20 {
            for x in 0..100 {
                rows_text.push_str(buffer2.get(x, y).symbol());
            }
        }
        assert!(rows_text.contains("You"));
        assert!(rows_text.contains("Hello CRUDO"));
    }

    #[test]
    fn test_render_permission_prompt_overlay() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();

        let (resp_tx, _resp_rx) = std::sync::mpsc::sync_channel(1);
        let responder = crate::agent::events::PermissionResponder::new(resp_tx);

        app.pending_permission = Some(crate::app::PendingPermission {
            id: 1,
            tool_name: "bash".to_string(),
            summary: "rm -rf /tmp/test".to_string(),
            current_mode: "workspace-write".to_string(),
            reason: Some("requires DangerFullAccess".to_string()),
            choice: crate::app::PermissionChoice::Allow,
            responder,
        });

        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        let mut all_text = String::new();
        for y in 0..30 {
            for x in 0..100 {
                all_text.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(all_text.contains("PERMISSION REQUEST"));
        assert!(all_text.contains("bash"));
        assert!(all_text.contains("workspace-write"));
        assert!(all_text.contains("rm -rf /tmp/test"));
        assert!(all_text.contains("requires DangerFullAccess"));
        assert!(all_text.contains("Allow"));
        assert!(all_text.contains("Deny"));
        assert!(all_text.contains("[←/→/Tab] Select"));
    }

    #[test]
    fn test_render_activity_panel_empty_state() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.show_activity_panel = true;

        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        let mut all_text = String::new();
        for y in 0..30 {
            for x in 0..100 {
                all_text.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(all_text.contains("ACTIVITY"));
        assert!(all_text.contains("No tool activity"));
    }

    #[test]
    fn test_render_activity_panel_populated_lifecycle() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.show_activity_panel = true;

        // 1. Completed
        let id1 = app.start_activity("search_files".to_string(), "query".to_string());
        app.finish_activity(id1, Some(1200));

        // 2. Running
        let _id2 = app.start_activity("execute_command".to_string(), "test".to_string());
        app.update_activity_progress(_id2, "Compiling crudo v0.1.0");

        // 3. Failed
        let id3 = app.start_activity("write_file".to_string(), "path".to_string());
        app.update_activity_progress(id3, "Error: permission denied");
        app.finish_activity(id3, Some(300));

        // 4. Cancelled
        let id4 = app.start_activity("shell_command".to_string(), "sleep".to_string());
        app.cancel_activity(id4, None);
        if let Some(act) = app.activities.iter_mut().find(|a| a.id == id4) {
            act.error = Some("cancelled by user".to_string());
        }

        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        let mut all_text = String::new();
        for y in 0..30 {
            for x in 0..100 {
                all_text.push_str(buffer.get(x, y).symbol());
            }
        }

        // Verify tool names, lifecycle symbols, and status/duration text
        assert!(all_text.contains("search_files"));
        assert!(all_text.contains("1.2s"));
        assert!(all_text.contains("✓"));

        assert!(all_text.contains("execute_command"));
        assert!(all_text.contains("running"));
        assert!(all_text.contains("└─ Compiling crudo v0.1.0"));

        assert!(all_text.contains("write_file"));
        assert!(all_text.contains("failed"));
        assert!(all_text.contains("✗"));
        assert!(all_text.contains("└─ permission denied"));

        assert!(all_text.contains("shell_command"));
        assert!(all_text.contains("cancelled"));
        assert!(all_text.contains("⊘"));
        assert!(all_text.contains("└─ cancelled by user"));
    }
}
