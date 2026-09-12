use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::events::{Actor, AgentActivity};
use crate::state::{AppState, ConversationMessage};
use crate::ui::progress::render_progress_bar;
use crate::ui::theme::{
    Theme, COLOR_BLACK, COLOR_PRIMARY_PURPLE, COLOR_WHITE, GLYPH_CRUDO_GEAR, GLYPH_USER_TRIANGLE,
};

pub struct ChatWidget;

impl ChatWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        // Distinct purple-bordered chat workspace box
        let mut chat_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(COLOR_PRIMARY_PURPLE))
            .style(Style::default().bg(COLOR_BLACK));

        let inner_area = chat_block.inner(area);

        if inner_area.height == 0 || inner_area.width == 0 {
            frame.render_widget(chat_block, area);
            return;
        }

        let lines = Self::build_chat_lines(
            &state.conversation.messages,
            state.conversation.active_document.as_ref(),
            state.conversation.activity,
            inner_area.width as usize,
        );

        let total_lines = lines.len();
        let visible_height = inner_area.height as usize;

        // Record metrics for scrolling calculations
        state.conversation.content_height.set(total_lines);
        state.conversation.viewport_height.set(visible_height);
        state.conversation.last_total_rows.set(total_lines);
        state.conversation.last_viewport_height.set(visible_height);

        let max_scroll = total_lines.saturating_sub(visible_height);

        // Determine starting visible row based on auto_scroll and scroll_offset
        let scroll_to_render = if state.conversation.auto_scroll {
            max_scroll
        } else {
            state.conversation.scroll_offset.min(max_scroll)
        };

        // Subtle scroll indicator on chat border if user has scrolled up
        if !state.conversation.auto_scroll && scroll_to_render < max_scroll {
            let lines_below = max_scroll.saturating_sub(scroll_to_render);
            let indicator = format!(" [↑ {lines_below} lines above bottom] ");
            chat_block = chat_block.title_bottom(
                Line::from(Span::styled(indicator, Theme::placeholder()))
                    .alignment(Alignment::Right),
            );
        }

        // Render the chat workspace frame
        frame.render_widget(chat_block, area);

        // Native Ratatui scrolling mechanism operating on full rendered chat content
        let paragraph = Paragraph::new(lines)
            .scroll((scroll_to_render.min(u16::MAX as usize) as u16, 0))
            .style(Style::default().bg(COLOR_BLACK));

        frame.render_widget(paragraph, inner_area);
    }

    pub fn build_chat_lines(
        messages: &[ConversationMessage],
        active_document: Option<&crate::state::DocumentProgressState>,
        activity: AgentActivity,
        inner_width: usize,
    ) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let avail = inner_width;
        let box_width = if avail >= 24 {
            avail.saturating_sub(4)
        } else {
            avail
        };
        let content_width = box_width.saturating_sub(4);

        // Breathing room at top of workspace
        lines.push(Line::raw(""));

        // Render Conversation Messages
        for (i, msg) in messages.iter().enumerate() {
            let is_last = i == messages.len() - 1;
            match msg.role {
                Actor::User => {
                    Self::render_user_message(&mut lines, msg, box_width, content_width);
                }
                Actor::Crudo => {
                    let msg_activity = if is_last && msg.is_streaming {
                        activity
                    } else {
                        AgentActivity::Idle
                    };
                    Self::render_crudo_message(&mut lines, msg, box_width, msg_activity);
                }
            }
            // Vertical breathing room between messages
            lines.push(Line::raw(""));
            lines.push(Line::raw(""));
        }

        // Render Active Document Progress if present
        if let Some(doc) = active_document {
            Self::render_document_progress(&mut lines, doc);
            lines.push(Line::raw(""));
        } else if activity != AgentActivity::Idle {
            let last_is_streaming_crudo = messages
                .last()
                .is_some_and(|m| m.role == Actor::Crudo && m.is_streaming);
            if !last_is_streaming_crudo {
                // Render standalone activity indicator for pending CRUDO reasoning/action
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{GLYPH_CRUDO_GEAR} "), Theme::crudo_marker()),
                    Span::styled(activity.display_label(), Theme::crudo_marker()),
                ]));
                lines.push(Line::raw(""));
            }
        }

        lines
    }

    fn render_user_message(
        lines: &mut Vec<Line<'static>>,
        msg: &ConversationMessage,
        box_width: usize,
        content_width: usize,
    ) {
        let border_style = Theme::user_box_border();
        let user_style = Theme::user_marker();

        // Top border with integrated triangle and USER:
        // ▶ USER ────────────────────────────────
        let prefix_len = 7; // "▶" (1) + " " (1) + "USER" (4) + " " (1)
        let dashes_len = box_width.saturating_sub(prefix_len + 1);
        let top_dashes = "─".repeat(dashes_len);

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{GLYPH_USER_TRIANGLE} "), user_style),
            Span::styled("USER ", user_style),
            Span::styled(format!("{top_dashes}┐"), border_style),
        ]));

        // Attached files badges inside the user box
        for att in &msg.attachments {
            let badge = att.display_badge();
            let pad = content_width.saturating_sub(badge.chars().count());
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("│ ", border_style),
                Span::styled(badge, Theme::badge()),
                Span::raw(" ".repeat(pad)),
                Span::styled(" │", border_style),
            ]));
        }

        // Box content
        for raw_line in msg.content.lines() {
            let wrapped = Self::wrap_line(raw_line, content_width);
            for w in wrapped {
                let padding = content_width.saturating_sub(w.chars().count());
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("│ ", border_style),
                    Span::styled(w, Style::default().fg(COLOR_WHITE)),
                    Span::raw(" ".repeat(padding)),
                    Span::styled(" │", border_style),
                ]));
            }
        }

        // If content was empty string
        if msg.content.is_empty() {
            let padding = content_width;
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("│ ", border_style),
                Span::raw(" ".repeat(padding)),
                Span::styled(" │", border_style),
            ]));
        }

        // Bottom border of user box
        let bottom_border = format!("  └{}┘", "─".repeat(box_width.saturating_sub(2)));
        lines.push(Line::from(vec![Span::styled(bottom_border, border_style)]));
    }

    fn render_crudo_message(
        lines: &mut Vec<Line<'static>>,
        msg: &ConversationMessage,
        box_width: usize,
        activity: AgentActivity,
    ) {
        let label = if activity != AgentActivity::Idle {
            activity.display_label()
        } else {
            "CRUDO"
        };

        // Line 1: ⚙ CRUDO (or dynamic activity label)
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{GLYPH_CRUDO_GEAR} "), Theme::crudo_marker()),
            Span::styled(label, Theme::crudo_marker()),
        ]));

        lines.push(Line::raw(""));

        // Unboxed response text in pure white, word-wrapped to box width
        let crudo_width = box_width.max(10);
        for line in msg.content.lines() {
            let wrapped = Self::wrap_line(line, crudo_width);
            for w in wrapped {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(w, Style::default().fg(COLOR_WHITE)),
                ]));
            }
        }

        if msg.is_streaming {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("...", Style::default().fg(COLOR_PRIMARY_PURPLE)),
            ]));
        }
    }

    fn render_document_progress(
        lines: &mut Vec<Line<'static>>,
        doc: &crate::state::DocumentProgressState,
    ) {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{GLYPH_CRUDO_GEAR} "), Theme::crudo_marker()),
            Span::styled(
                "CRUDO IS LOOKING THROUGH THE ATTACHMENT...",
                Theme::crudo_marker(),
            ),
        ]));

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("Reading {}", doc.filename),
                Style::default().fg(COLOR_WHITE),
            ),
        ]));

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{} document", doc.stage),
                Style::default().fg(COLOR_WHITE),
            ),
        ]));

        let bar = render_progress_bar(doc.percentage, 24);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(COLOR_PRIMARY_PURPLE)),
        ]));

        if doc.total > 0 {
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("Pages processed: {} / {}", doc.current, doc.total),
                    Style::default().fg(COLOR_WHITE),
                ),
            ]));
        }
    }

    fn wrap_line(line: &str, max_width: usize) -> Vec<String> {
        if max_width == 0 {
            return vec![line.to_string()];
        }

        let mut res = Vec::new();
        let mut current = String::new();
        let mut current_len = 0;

        for word in line.split_whitespace() {
            let word_len = word.chars().count();
            if current_len == 0 {
                if word_len <= max_width {
                    current.push_str(word);
                    current_len += word_len;
                } else {
                    let mut chunk = String::new();
                    let mut chunk_len = 0;
                    for ch in word.chars() {
                        if chunk_len + 1 > max_width {
                            res.push(std::mem::take(&mut chunk));
                            chunk_len = 0;
                        }
                        chunk.push(ch);
                        chunk_len += 1;
                    }
                    if !chunk.is_empty() {
                        current = chunk;
                        current_len = chunk_len;
                    }
                }
            } else if current_len + 1 + word_len <= max_width {
                current.push(' ');
                current.push_str(word);
                current_len += 1 + word_len;
            } else {
                res.push(std::mem::take(&mut current));
                if word_len <= max_width {
                    current.push_str(word);
                    current_len = word_len;
                } else {
                    let mut chunk = String::new();
                    let mut chunk_len = 0;
                    for ch in word.chars() {
                        if chunk_len + 1 > max_width {
                            res.push(std::mem::take(&mut chunk));
                            chunk_len = 0;
                        }
                        chunk.push(ch);
                        chunk_len += 1;
                    }
                    if !chunk.is_empty() {
                        current = chunk;
                        current_len = chunk_len;
                    }
                }
            }
        }

        if !current.is_empty() || res.is_empty() {
            res.push(current);
        }

        res
    }
}
