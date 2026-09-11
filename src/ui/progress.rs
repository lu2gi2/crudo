use crate::ui::theme::{GLYPH_PROGRESS_EMPTY, GLYPH_PROGRESS_FULL};

/// Renders a terminal-native progress bar string:
/// Example: `████████████████████░░░░░░░░  72%`
pub fn render_progress_bar(percentage: u8, bar_width: usize) -> String {
    let pct = percentage.min(100);
    let filled_count = (pct as usize * bar_width) / 100;
    let empty_count = bar_width.saturating_sub(filled_count);

    let mut out = String::with_capacity(bar_width * 4 + 8);
    for _ in 0..filled_count {
        out.push_str(GLYPH_PROGRESS_FULL);
    }
    for _ in 0..empty_count {
        out.push_str(GLYPH_PROGRESS_EMPTY);
    }
    out.push_str(&format!("  {:>3}%", pct));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_percentage_rendering() {
        let bar = render_progress_bar(72, 28);
        assert!(bar.contains("72%"));
        assert!(bar.contains(GLYPH_PROGRESS_FULL));
        assert!(bar.contains(GLYPH_PROGRESS_EMPTY));

        let full = render_progress_bar(100, 20);
        assert!(full.contains("100%"));
        assert!(!full.contains(GLYPH_PROGRESS_EMPTY));

        let zero = render_progress_bar(0, 20);
        assert!(zero.contains("  0%"));
        assert!(!zero.contains(GLYPH_PROGRESS_FULL));
    }
}
