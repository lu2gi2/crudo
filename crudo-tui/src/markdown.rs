use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    CodeBlock {
        language: Option<String>,
        lines: Vec<String>,
        is_closed: bool,
    },
    UnorderedListItem(String),
    OrderedListItem {
        number: usize,
        text: String,
    },
    Blockquote(String),
    HorizontalRule,
    BlankLine,
}

/// Parses raw Markdown text into a sequence of block elements.
pub fn parse_markdown(content: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if in_code_block {
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                blocks.push(MarkdownBlock::CodeBlock {
                    language: code_lang.take(),
                    lines: std::mem::take(&mut code_lines),
                    is_closed: true,
                });
                in_code_block = false;
            } else {
                code_lines.push(line.to_string());
            }
            continue;
        }

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = true;
            let lang_part = trimmed
                .trim_start_matches('`')
                .trim_start_matches('~')
                .trim();
            code_lang = if lang_part.is_empty() {
                None
            } else {
                Some(lang_part.to_string())
            };
            code_lines.clear();
            continue;
        }

        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            blocks.push(MarkdownBlock::HorizontalRule);
            continue;
        }

        if let Some(heading_text) = line.strip_prefix("# ") {
            blocks.push(MarkdownBlock::Heading {
                level: 1,
                text: heading_text.trim().to_string(),
            });
            continue;
        }
        if let Some(heading_text) = line.strip_prefix("## ") {
            blocks.push(MarkdownBlock::Heading {
                level: 2,
                text: heading_text.trim().to_string(),
            });
            continue;
        }
        if let Some(heading_text) = line.strip_prefix("### ") {
            blocks.push(MarkdownBlock::Heading {
                level: 3,
                text: heading_text.trim().to_string(),
            });
            continue;
        }
        if let Some(heading_text) = line.strip_prefix("#### ") {
            blocks.push(MarkdownBlock::Heading {
                level: 4,
                text: heading_text.trim().to_string(),
            });
            continue;
        }
        if let Some(heading_text) = line.strip_prefix("##### ") {
            blocks.push(MarkdownBlock::Heading {
                level: 5,
                text: heading_text.trim().to_string(),
            });
            continue;
        }
        if let Some(heading_text) = line.strip_prefix("###### ") {
            blocks.push(MarkdownBlock::Heading {
                level: 6,
                text: heading_text.trim().to_string(),
            });
            continue;
        }

        if let Some(quote_text) = line.strip_prefix("> ") {
            blocks.push(MarkdownBlock::Blockquote(quote_text.to_string()));
            continue;
        } else if line == ">" {
            blocks.push(MarkdownBlock::Blockquote(String::new()));
            continue;
        }

        if let Some(item_text) = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
        {
            blocks.push(MarkdownBlock::UnorderedListItem(item_text.to_string()));
            continue;
        }

        if let Some((num, item_text)) = parse_ordered_list_item(line) {
            blocks.push(MarkdownBlock::OrderedListItem {
                number: num,
                text: item_text,
            });
            continue;
        }

        if trimmed.is_empty() {
            blocks.push(MarkdownBlock::BlankLine);
            continue;
        }

        blocks.push(MarkdownBlock::Paragraph(line.to_string()));
    }

    if in_code_block {
        blocks.push(MarkdownBlock::CodeBlock {
            language: code_lang,
            lines: code_lines,
            is_closed: false,
        });
    }

    blocks
}

fn parse_ordered_list_item(line: &str) -> Option<(usize, String)> {
    let trimmed_start = line.trim_start();
    let dot_pos = trimmed_start.find(". ")?;
    let num_str = &trimmed_start[..dot_pos];
    let num = num_str.parse::<usize>().ok()?;
    let text = trimmed_start[dot_pos + 2..].to_string();
    Some((num, text))
}

/// Parses inline formatting: **bold**, *italic*, ***bold-italic***, and `code`.
pub fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut current_text = String::new();

    let flush_text = |spans: &mut Vec<Span<'static>>, current: &mut String| {
        if !current.is_empty() {
            spans.push(Span::raw(std::mem::take(current)));
        }
    };

    while i < len {
        // Check for inline code `...`
        if chars[i] == '`' {
            flush_text(&mut spans, &mut current_text);
            let start = i + 1;
            let mut end = start;
            while end < len && chars[end] != '`' {
                end += 1;
            }
            if end < len {
                // Closed backtick
                let code_content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(
                    code_content,
                    Style::default()
                        .fg(Color::LightYellow)
                        .bg(Color::Rgb(40, 34, 55)),
                ));
                i = end + 1;
            } else {
                // Unclosed backtick (e.g. streaming) - treat as inline code to the end
                let code_content: String = chars[start..len].iter().collect();
                spans.push(Span::styled(
                    code_content,
                    Style::default()
                        .fg(Color::LightYellow)
                        .bg(Color::Rgb(40, 34, 55)),
                ));
                i = len;
            }
            continue;
        }

        // Check for bold+italic ***...***
        if i + 2 < len && chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*' {
            flush_text(&mut spans, &mut current_text);
            let start = i + 3;
            let mut end = start;
            while end + 2 < len
                && !(chars[end] == '*' && chars[end + 1] == '*' && chars[end + 2] == '*')
            {
                end += 1;
            }
            if end + 2 < len {
                let bold_italic_content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(
                    bold_italic_content,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ));
                i = end + 3;
            } else {
                // Incomplete
                let bold_italic_content: String = chars[start..len].iter().collect();
                spans.push(Span::styled(
                    bold_italic_content,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ));
                i = len;
            }
            continue;
        }

        // Check for bold **...**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            flush_text(&mut spans, &mut current_text);
            let start = i + 2;
            let mut end = start;
            while end + 1 < len && !(chars[end] == '*' && chars[end + 1] == '*') {
                end += 1;
            }
            if end + 1 < len {
                let bold_content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(
                    bold_content,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
            } else {
                // Incomplete streaming
                let bold_content: String = chars[start..len].iter().collect();
                spans.push(Span::styled(
                    bold_content,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
                i = len;
            }
            continue;
        }

        // Check for italic *...*
        if chars[i] == '*' {
            flush_text(&mut spans, &mut current_text);
            let start = i + 1;
            let mut end = start;
            while end < len && chars[end] != '*' {
                end += 1;
            }
            if end < len {
                let italic_content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(
                    italic_content,
                    Style::default()
                        .fg(Color::Rgb(220, 215, 235))
                        .add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
            } else {
                // Incomplete streaming
                let italic_content: String = chars[start..len].iter().collect();
                spans.push(Span::styled(
                    italic_content,
                    Style::default()
                        .fg(Color::Rgb(220, 215, 235))
                        .add_modifier(Modifier::ITALIC),
                ));
                i = len;
            }
            continue;
        }

        current_text.push(chars[i]);
        i += 1;
    }

    flush_text(&mut spans, &mut current_text);
    spans
}

#[derive(Clone)]
struct StyledWord {
    text: String,
    style: Style,
    width: usize,
    is_whitespace: bool,
}

/// Wraps styled spans into multiple lines adhering to `width` and optional hanging `indent`.
pub fn wrap_spans(
    spans: Vec<Span<'static>>,
    width: usize,
    prefix_spans: Vec<Span<'static>>,
    indent_spaces: usize,
) -> Vec<Line<'static>> {
    if width == 0 {
        let mut full = prefix_spans;
        full.extend(spans);
        return vec![Line::from(full)];
    }

    let mut words: Vec<StyledWord> = Vec::new();
    for span in spans {
        let text = span.content.as_ref();
        let style = span.style;
        let mut cur = String::new();
        let mut in_ws = false;

        for ch in text.chars() {
            let is_ws = ch.is_whitespace();
            if cur.is_empty() {
                in_ws = is_ws;
                cur.push(ch);
            } else if in_ws == is_ws {
                cur.push(ch);
            } else {
                let w = UnicodeWidthStr::width(cur.as_str());
                words.push(StyledWord {
                    text: std::mem::take(&mut cur),
                    style,
                    width: w,
                    is_whitespace: in_ws,
                });
                in_ws = is_ws;
                cur.push(ch);
            }
        }
        if !cur.is_empty() {
            let w = UnicodeWidthStr::width(cur.as_str());
            words.push(StyledWord {
                text: cur,
                style,
                width: w,
                is_whitespace: in_ws,
            });
        }
    }

    let mut lines = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_line_w = 0;
    let mut is_first_line = true;

    // Start first line with prefix
    let prefix_w: usize = prefix_spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    current_spans.extend(prefix_spans);
    current_line_w += prefix_w;

    let indent_str = " ".repeat(indent_spaces);

    for word in words {
        if word.is_whitespace
            && current_line_w
                == if is_first_line {
                    prefix_w
                } else {
                    indent_spaces
                }
        {
            // Ignore leading whitespace at the beginning of wrapped lines
            continue;
        }

        if current_line_w + word.width <= width {
            current_spans.push(Span::styled(word.text, word.style));
            current_line_w += word.width;
        } else {
            // Line break needed
            if current_line_w
                > if is_first_line {
                    prefix_w
                } else {
                    indent_spaces
                }
            {
                lines.push(Line::from(std::mem::take(&mut current_spans)));
                is_first_line = false;
                if indent_spaces > 0 {
                    current_spans.push(Span::raw(indent_str.clone()));
                    current_line_w = indent_spaces;
                } else {
                    current_line_w = 0;
                }
            }

            if word.is_whitespace {
                continue;
            }

            // If word itself is longer than width
            if word.width > width {
                let chars: Vec<char> = word.text.chars().collect();
                let available = width.saturating_sub(current_line_w).max(1);
                let mut chunk_idx = 0;
                while chunk_idx < chars.len() {
                    let take_len = if chunk_idx == 0 {
                        available.min(chars.len() - chunk_idx)
                    } else {
                        width
                            .saturating_sub(indent_spaces)
                            .max(1)
                            .min(chars.len() - chunk_idx)
                    };
                    let chunk_text: String =
                        chars[chunk_idx..chunk_idx + take_len].iter().collect();
                    let chunk_w = UnicodeWidthStr::width(chunk_text.as_str());

                    current_spans.push(Span::styled(chunk_text, word.style));
                    current_line_w += chunk_w;
                    chunk_idx += take_len;

                    if chunk_idx < chars.len() {
                        lines.push(Line::from(std::mem::take(&mut current_spans)));
                        is_first_line = false;
                        if indent_spaces > 0 {
                            current_spans.push(Span::raw(indent_str.clone()));
                            current_line_w = indent_spaces;
                        } else {
                            current_line_w = 0;
                        }
                    }
                }
            } else {
                current_spans.push(Span::styled(word.text, word.style));
                current_line_w += word.width;
            }
        }
    }

    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

/// Lightweight syntax highlighting for common keywords, strings, comments, numbers.
pub fn highlight_code_line(line: &str, _lang: Option<&str>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let trimmed = line.trim_start();
    let leading_spaces = &line[..line.len() - trimmed.len()];
    let bg_style = Color::Rgb(26, 22, 36);

    if !leading_spaces.is_empty() {
        spans.push(Span::styled(
            leading_spaces.to_string(),
            Style::default().bg(bg_style),
        ));
    }

    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        spans.push(Span::styled(
            trimmed.to_string(),
            Style::default()
                .fg(Color::DarkGray)
                .bg(bg_style)
                .add_modifier(Modifier::ITALIC),
        ));
        return spans;
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut word = String::new();

    let flush_word = |spans: &mut Vec<Span<'static>>, word: &mut String| {
        if word.is_empty() {
            return;
        }
        let w = std::mem::take(word);
        let style = match w.as_str() {
            "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "type"
            | "match" | "if" | "else" | "while" | "for" | "in" | "loop" | "return" | "break"
            | "continue" | "use" | "mod" | "as" | "const" | "static" | "async" | "await"
            | "def" | "class" | "import" | "from" | "function" | "var" | "val" | "self"
            | "true" | "false" | "None" | "Some" | "Ok" | "Err" | "null" | "undefined"
            | "print" | "println" => Style::default()
                .fg(Color::Cyan)
                .bg(bg_style)
                .add_modifier(Modifier::BOLD),
            "String" | "Vec" | "Option" | "Result" | "i32" | "i64" | "u8" | "u16" | "u32"
            | "u64" | "usize" | "bool" | "str" | "char" => {
                Style::default().fg(Color::LightYellow).bg(bg_style)
            }
            s if s.chars().all(|c| c.is_ascii_digit()) => {
                Style::default().fg(Color::LightMagenta).bg(bg_style)
            }
            _ => Style::default().fg(Color::White).bg(bg_style),
        };
        spans.push(Span::styled(w, style));
    };

    while i < len {
        let ch = chars[i];

        // String literals
        if ch == '"' || ch == '\'' {
            flush_word(&mut spans, &mut word);
            let quote = ch;
            let mut str_val = String::new();
            str_val.push(quote);
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    str_val.push(chars[i]);
                    str_val.push(chars[i + 1]);
                    i += 2;
                } else {
                    str_val.push(chars[i]);
                    i += 1;
                }
            }
            if i < len {
                str_val.push(quote);
                i += 1;
            }
            spans.push(Span::styled(
                str_val,
                Style::default().fg(Color::Green).bg(bg_style),
            ));
            continue;
        }

        // Inline comment
        if ch == '/' && i + 1 < len && chars[i + 1] == '/' {
            flush_word(&mut spans, &mut word);
            let comment: String = chars[i..len].iter().collect();
            spans.push(Span::styled(
                comment,
                Style::default()
                    .fg(Color::DarkGray)
                    .bg(bg_style)
                    .add_modifier(Modifier::ITALIC),
            ));
            break;
        }

        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
            i += 1;
        } else {
            flush_word(&mut spans, &mut word);
            let mut sym = String::new();
            sym.push(ch);
            spans.push(Span::styled(
                sym,
                Style::default().fg(Color::Rgb(180, 180, 200)).bg(bg_style),
            ));
            i += 1;
        }
    }

    flush_word(&mut spans, &mut word);
    spans
}

/// Renders a full Markdown text document into a list of terminal `Line`s ready for Ratatui.
pub fn render_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    let blocks = parse_markdown(content);
    let mut lines = Vec::new();
    let content_width = if width > 2 { width } else { 40 };

    for block in blocks {
        match block {
            MarkdownBlock::Heading { level, text } => {
                let prefix_style = match level {
                    1 => Style::default()
                        .fg(crate::theme::PRIMARY_ACCENT)
                        .add_modifier(Modifier::BOLD),
                    2 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    3 => Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD),
                    _ => Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                };
                let prefix_hashes = "#".repeat(level as usize);
                let prefix_spans = vec![Span::styled(format!("{} ", prefix_hashes), prefix_style)];
                let mut text_spans = parse_inline(&text);
                // Apply heading style to plain text spans
                for span in &mut text_spans {
                    span.style = span.style.patch(prefix_style);
                }
                let wrapped = wrap_spans(text_spans, content_width, prefix_spans, 2);
                lines.extend(wrapped);
            }
            MarkdownBlock::Paragraph(text) => {
                let spans = parse_inline(&text);
                let wrapped = wrap_spans(spans, content_width, Vec::new(), 0);
                lines.extend(wrapped);
            }
            MarkdownBlock::Blockquote(text) => {
                let mut spans = parse_inline(&text);
                for span in &mut spans {
                    span.style = span.style.patch(
                        Style::default()
                            .fg(Color::Rgb(210, 205, 230))
                            .add_modifier(Modifier::ITALIC),
                    );
                }
                let prefix = vec![Span::styled(
                    "│ ",
                    Style::default().fg(crate::theme::PRIMARY_ACCENT),
                )];
                let wrapped = wrap_spans(spans, content_width, prefix, 2);
                lines.extend(wrapped);
            }
            MarkdownBlock::UnorderedListItem(text) => {
                let spans = parse_inline(&text);
                let prefix = vec![Span::styled(
                    "  • ",
                    Style::default().fg(crate::theme::PRIMARY_ACCENT),
                )];
                let wrapped = wrap_spans(spans, content_width, prefix, 4);
                lines.extend(wrapped);
            }
            MarkdownBlock::OrderedListItem { number, text } => {
                let spans = parse_inline(&text);
                let num_str = format!("  {}. ", number);
                let indent = num_str.len();
                let prefix = vec![Span::styled(
                    num_str,
                    Style::default().fg(crate::theme::PRIMARY_ACCENT),
                )];
                let wrapped = wrap_spans(spans, content_width, prefix, indent);
                lines.extend(wrapped);
            }
            MarkdownBlock::HorizontalRule => {
                let hr_len = content_width.min(40).max(10);
                lines.push(Line::from(Span::styled(
                    "─".repeat(hr_len),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            MarkdownBlock::CodeBlock {
                language,
                lines: code_lines,
                is_closed,
            } => {
                let block_w = content_width.min(65).max(15);
                let lang_tag = language.as_deref().unwrap_or("code");
                let label = format!("╭─ {} ", lang_tag);
                let fill_len = block_w.saturating_sub(UnicodeWidthStr::width(label.as_str()) + 1);
                let top_border = format!("{}{}╮", label, "─".repeat(fill_len));

                lines.push(Line::from(Span::styled(
                    top_border,
                    Style::default().fg(crate::theme::PRIMARY_ACCENT),
                )));

                for code_line in &code_lines {
                    let mut line_spans = vec![Span::styled(
                        "│ ",
                        Style::default().fg(crate::theme::PRIMARY_ACCENT),
                    )];
                    let highlighted = highlight_code_line(code_line, language.as_deref());
                    line_spans.extend(highlighted);
                    lines.push(Line::from(line_spans));
                }

                if is_closed {
                    let bot_fill = block_w.saturating_sub(2);
                    let bot_border = format!("╰{}╯", "─".repeat(bot_fill));
                    lines.push(Line::from(Span::styled(
                        bot_border,
                        Style::default().fg(crate::theme::PRIMARY_ACCENT),
                    )));
                }
            }
            MarkdownBlock::BlankLine => {
                lines.push(Line::from(""));
            }
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_parsing() {
        let md = "# Heading 1\n## Heading 2\n### Heading 3";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![
                MarkdownBlock::Heading {
                    level: 1,
                    text: "Heading 1".to_string()
                },
                MarkdownBlock::Heading {
                    level: 2,
                    text: "Heading 2".to_string()
                },
                MarkdownBlock::Heading {
                    level: 3,
                    text: "Heading 3".to_string()
                },
            ]
        );
    }

    #[test]
    fn test_bold_italic_parsing() {
        let spans = parse_inline("Normal **bold** and *italic* and ***both*** text");
        assert_eq!(spans[0].content, "Normal ");
        assert_eq!(spans[1].content, "bold");
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[2].content, " and ");
        assert_eq!(spans[3].content, "italic");
        assert!(spans[3].style.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(spans[4].content, " and ");
        assert_eq!(spans[5].content, "both");
        assert!(spans[5].style.add_modifier.contains(Modifier::BOLD));
        assert!(spans[5].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_inline_code() {
        let spans = parse_inline("Run `cargo test` now");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Run ");
        assert_eq!(spans[1].content, "cargo test");
        assert_eq!(spans[1].style.fg, Some(Color::LightYellow));
        assert_eq!(spans[2].content, " now");
    }

    #[test]
    fn test_unordered_and_ordered_lists() {
        let md = "- Item A\n* Item B\n1. First\n2. Second";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![
                MarkdownBlock::UnorderedListItem("Item A".to_string()),
                MarkdownBlock::UnorderedListItem("Item B".to_string()),
                MarkdownBlock::OrderedListItem {
                    number: 1,
                    text: "First".to_string()
                },
                MarkdownBlock::OrderedListItem {
                    number: 2,
                    text: "Second".to_string()
                },
            ]
        );
    }

    #[test]
    fn test_blockquotes() {
        let md = "> Quoted insight\n> Second line";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![
                MarkdownBlock::Blockquote("Quoted insight".to_string()),
                MarkdownBlock::Blockquote("Second line".to_string()),
            ]
        );
    }

    #[test]
    fn test_horizontal_separators() {
        let md = "Above\n---\nBelow";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![
                MarkdownBlock::Paragraph("Above".to_string()),
                MarkdownBlock::HorizontalRule,
                MarkdownBlock::Paragraph("Below".to_string()),
            ]
        );
    }

    #[test]
    fn test_fenced_code_blocks_with_language() {
        let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![MarkdownBlock::CodeBlock {
                language: Some("rust".to_string()),
                lines: vec![
                    "fn main() {".to_string(),
                    "    println!(\"Hello\");".to_string(),
                    "}".to_string()
                ],
                is_closed: true,
            }]
        );
    }

    #[test]
    fn test_fenced_code_blocks_without_language() {
        let md = "```\nplain code\n```";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![MarkdownBlock::CodeBlock {
                language: None,
                lines: vec!["plain code".to_string()],
                is_closed: true,
            }]
        );
    }

    #[test]
    fn test_incomplete_streaming_code_fences() {
        let md = "```rust\nfn main() {";
        let blocks = parse_markdown(md);
        assert_eq!(
            blocks,
            vec![MarkdownBlock::CodeBlock {
                language: Some("rust".to_string()),
                lines: vec!["fn main() {".to_string()],
                is_closed: false,
            }]
        );

        // Rendering incomplete fence does not crash
        let rendered = render_markdown(md, 50);
        assert!(!rendered.is_empty());
        // Top border present
        assert!(rendered[0].to_string().contains("rust"));
        // Code line present
        assert!(rendered[1].to_string().contains("fn main()"));
        // No closing border since not closed
        assert_eq!(rendered.len(), 2);
    }

    #[test]
    fn test_raw_message_content_preservation() {
        let raw = "## Hello\n**bold** `code`\n```rust\nlet x = 1;\n```";
        let msg = crate::message::Message::new(1, crate::message::Role::Assistant, raw.to_string());
        // Content must be unchanged
        assert_eq!(msg.content, raw);
        let rendered = render_markdown(&msg.content, 40);
        assert!(!rendered.is_empty());
        assert_eq!(msg.content, raw);
    }

    #[test]
    fn test_markdown_rendered_height() {
        let md = "# Title\nParagraph text\n- item 1\n- item 2";
        let lines = render_markdown(md, 60);
        // 1 heading + 1 paragraph + 2 list items = 4 lines
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_wrapped_text_height() {
        let long_text = "Word ".repeat(30); // ~150 chars
        let lines = render_markdown(&long_text, 30);
        // 150 chars in 30 col width wraps into 5-6 lines
        assert!(lines.len() >= 5);
    }

    #[test]
    fn test_code_block_height() {
        let md = "```python\nx = 1\ny = 2\n```";
        let lines = render_markdown(md, 50);
        // top border + 2 lines of code + bottom border = 4 lines
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_scroll_behavior_after_markdown_rendering() {
        let mut scroll = crate::scroll::ScrollState::new();
        let md = "```rust\nline 1\nline 2\nline 3\nline 4\nline 5\n```";
        let lines = render_markdown(md, 40); // 7 lines total (top border + 5 lines + bot border)
        assert_eq!(lines.len(), 7);

        let viewport_height = 4;
        let max_offset = (lines.len() as u16).saturating_sub(viewport_height);
        scroll.set_viewport(max_offset, viewport_height);

        // Auto-scroll default is at bottom
        assert_eq!(scroll.offset, 3);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_page_up_page_down_home_end_semantics() {
        let mut scroll = crate::scroll::ScrollState::new();
        let viewport_height = 5;
        let max_offset = 20;
        scroll.set_viewport(max_offset, viewport_height);
        assert_eq!(scroll.offset, 20);

        // PageUp scrolls one page upward (5 lines)
        scroll.scroll_by_page(-1);
        assert_eq!(scroll.offset, 15);
        assert!(!scroll.auto_scroll);

        // PageDown scrolls one page downward (5 lines)
        scroll.scroll_by_page(1);
        assert_eq!(scroll.offset, 20);
        assert!(scroll.auto_scroll);

        // Home scrolls to top (0)
        scroll.home();
        assert_eq!(scroll.offset, 0);
        assert!(!scroll.auto_scroll);

        // End scrolls to bottom (max_offset)
        scroll.end();
        assert_eq!(scroll.offset, 20);
        assert!(scroll.auto_scroll);
    }
}
