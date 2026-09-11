use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

/// Parses ANSI text (including 24-bit RGB, 256-color, 16-color, bold, and reset codes)
/// into a Ratatui `Text<'static>` structure suitable for native buffer rendering.
pub fn parse_ansi_to_text(ansi: &str) -> Text<'static> {
    let mut lines = Vec::new();

    for raw_line in ansi.lines() {
        let trimmed_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let mut spans = Vec::new();
        let mut current_style = Style::default();
        let bytes = trimmed_line.as_bytes();
        let mut i = 0;
        let mut text_start = 0;

        while i < bytes.len() {
            // Check for ESC [ (CSI sequence)
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                // Flush preceding plain text span
                if i > text_start {
                    let text = std::str::from_utf8(&bytes[text_start..i]).unwrap_or("");
                    if !text.is_empty() {
                        spans.push(Span::styled(text.to_string(), current_style));
                    }
                }

                // Search for terminating command character (0x40..=0x7E, e.g. 'm')
                let seq_start = i + 2;
                let mut seq_end = seq_start;
                while seq_end < bytes.len() && !(0x40..=0x7E).contains(&bytes[seq_end]) {
                    seq_end += 1;
                }

                if seq_end < bytes.len() {
                    let cmd = bytes[seq_end];
                    let params_str = std::str::from_utf8(&bytes[seq_start..seq_end]).unwrap_or("");
                    if cmd == b'm' {
                        current_style = update_sgr_style(current_style, params_str);
                    }
                    i = seq_end + 1;
                    text_start = i;
                } else {
                    // Malformed escape sequence, skip ESC [
                    i += 2;
                    text_start = i;
                }
            } else {
                i += 1;
            }
        }

        // Flush remainder of the line
        if text_start < bytes.len() {
            let text = std::str::from_utf8(&bytes[text_start..]).unwrap_or("");
            if !text.is_empty() {
                spans.push(Span::styled(text.to_string(), current_style));
            }
        }

        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

fn update_sgr_style(mut style: Style, params: &str) -> Style {
    if params.is_empty() || params == "0" {
        return Style::default();
    }

    let codes: Vec<u32> = params
        .split(';')
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();

    let mut idx = 0;
    while idx < codes.len() {
        match codes[idx] {
            0 => {
                style = Style::default();
                idx += 1;
            }
            1 => {
                style = style.add_modifier(Modifier::BOLD);
                idx += 1;
            }
            22 => {
                style = style.remove_modifier(Modifier::BOLD);
                idx += 1;
            }
            38 => {
                // 38;2;R;G;B (24-bit direct color) or 38;5;N (256 colors)
                if idx + 4 < codes.len() && codes[idx + 1] == 2 {
                    let r = codes[idx + 2].min(255) as u8;
                    let g = codes[idx + 3].min(255) as u8;
                    let b = codes[idx + 4].min(255) as u8;
                    style = style.fg(Color::Rgb(r, g, b));
                    idx += 5;
                } else if idx + 2 < codes.len() && codes[idx + 1] == 5 {
                    let n = codes[idx + 2].min(255) as u8;
                    style = style.fg(Color::Indexed(n));
                    idx += 3;
                } else {
                    idx += 1;
                }
            }
            48 => {
                // 48;2;R;G;B or 48;5;N
                if idx + 4 < codes.len() && codes[idx + 1] == 2 {
                    let r = codes[idx + 2].min(255) as u8;
                    let g = codes[idx + 3].min(255) as u8;
                    let b = codes[idx + 4].min(255) as u8;
                    style = style.bg(Color::Rgb(r, g, b));
                    idx += 5;
                } else if idx + 2 < codes.len() && codes[idx + 1] == 5 {
                    let n = codes[idx + 2].min(255) as u8;
                    style = style.bg(Color::Indexed(n));
                    idx += 3;
                } else {
                    idx += 1;
                }
            }
            39 => {
                style = style.fg(Color::Reset);
                idx += 1;
            }
            49 => {
                style = style.bg(Color::Reset);
                idx += 1;
            }
            c @ 30..=37 => {
                let color = match c {
                    30 => Color::Black,
                    31 => Color::Red,
                    32 => Color::Green,
                    33 => Color::Yellow,
                    34 => Color::Blue,
                    35 => Color::Magenta,
                    36 => Color::Cyan,
                    _ => Color::Gray,
                };
                style = style.fg(color);
                idx += 1;
            }
            c @ 40..=47 => {
                let color = match c {
                    40 => Color::Black,
                    41 => Color::Red,
                    42 => Color::Green,
                    43 => Color::Yellow,
                    44 => Color::Blue,
                    45 => Color::Magenta,
                    46 => Color::Cyan,
                    _ => Color::Gray,
                };
                style = style.bg(color);
                idx += 1;
            }
            c @ 90..=97 => {
                let color = match c {
                    90 => Color::DarkGray,
                    91 => Color::LightRed,
                    92 => Color::LightGreen,
                    93 => Color::LightYellow,
                    94 => Color::LightBlue,
                    95 => Color::LightMagenta,
                    96 => Color::LightCyan,
                    _ => Color::White,
                };
                style = style.fg(color);
                idx += 1;
            }
            _ => {
                idx += 1;
            }
        }
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ansi_rgb_color() {
        let ansi = "\x1b[38;2;135;90;236m█\x1b[0m";
        let text = parse_ansi_to_text(ansi);
        assert_eq!(text.lines.len(), 1);
        let line = &text.lines[0];
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "█");
        assert_eq!(
            line.spans[0].style,
            Style::default().fg(Color::Rgb(135, 90, 236))
        );
    }

    #[test]
    fn test_parse_crudo_blocks_ans_file() {
        let content = std::fs::read_to_string("assets/crudo_blocks.ans")
            .unwrap_or_else(|_| include_str!("../../assets/crudo_blocks.ans").to_string());
        let text = parse_ansi_to_text(&content);
        assert!(!text.lines.is_empty());
        assert_eq!(text.lines.len(), 12);
    }
}
