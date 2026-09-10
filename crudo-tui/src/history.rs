#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputHistory {
    entries: Vec<String>,
    index: Option<usize>,
    draft: String,
}

impl InputHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: String) {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return;
        }

        // Prevent consecutive duplicates
        if self.entries.last().map(|s| s.as_str()) != Some(trimmed) {
            self.entries.push(trimmed.to_string());
        }

        self.reset_navigation();
    }

    pub fn reset_navigation(&mut self) {
        self.index = None;
        self.draft.clear();
    }

    pub fn previous(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.index {
            None => {
                // Beginning navigation: capture current input as draft and pick newest
                self.draft = current_input.to_string();
                let last_idx = self.entries.len() - 1;
                self.index = Some(last_idx);
                Some(&self.entries[last_idx])
            }
            Some(idx) => {
                if idx > 0 {
                    let prev_idx = idx - 1;
                    self.index = Some(prev_idx);
                    Some(&self.entries[prev_idx])
                } else {
                    // Stay at oldest boundary; do not wrap around
                    Some(&self.entries[0])
                }
            }
        }
    }

    pub fn next(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.index {
            None => {
                // Not in navigation mode; already at current input
                None
            }
            Some(idx) => {
                if idx + 1 < self.entries.len() {
                    let next_idx = idx + 1;
                    self.index = Some(next_idx);
                    Some(&self.entries[next_idx])
                } else {
                    // Reached past newest entry: restore draft and reset navigation
                    self.index = None;
                    Some(&self.draft)
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_history() {
        let mut history = InputHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        assert_eq!(history.previous("draft"), None);
        assert_eq!(history.next(), None);
    }

    #[test]
    fn test_single_entry() {
        let mut history = InputHistory::new();
        history.push("first".to_string());
        assert_eq!(history.len(), 1);

        assert_eq!(history.previous(""), Some("first"));
        // Oldest boundary
        assert_eq!(history.previous(""), Some("first"));

        // Move forward back to draft
        assert_eq!(history.next(), Some(""));
        // Past newest boundary
        assert_eq!(history.next(), None);
    }

    #[test]
    fn test_multiple_entries_navigation_and_boundaries() {
        let mut history = InputHistory::new();
        history.push("A".to_string());
        history.push("B".to_string());
        history.push("C".to_string());

        // Up -> C
        assert_eq!(history.previous("my draft"), Some("C"));
        // Up -> B
        assert_eq!(history.previous("C"), Some("B"));
        // Up -> A
        assert_eq!(history.previous("B"), Some("A"));
        // Up at boundary -> A (no wrap)
        assert_eq!(history.previous("A"), Some("A"));
        assert_eq!(history.previous("A"), Some("A"));

        // Down -> B
        assert_eq!(history.next(), Some("B"));
        // Down -> C
        assert_eq!(history.next(), Some("C"));
        // Down -> restored draft
        assert_eq!(history.next(), Some("my draft"));
        // Down beyond draft -> None (no wrap)
        assert_eq!(history.next(), None);
    }

    #[test]
    fn test_consecutive_duplicate_prevention() {
        let mut history = InputHistory::new();
        history.push("same".to_string());
        history.push("same".to_string());
        history.push("different".to_string());
        history.push("different".to_string());
        history.push("same".to_string());

        assert_eq!(history.entries(), &["same", "different", "same"]);
    }

    #[test]
    fn test_empty_and_whitespace_submission_ignored() {
        let mut history = InputHistory::new();
        history.push("".to_string());
        history.push("   ".to_string());
        assert!(history.is_empty());
    }

    #[test]
    fn test_new_submission_resets_navigation() {
        let mut history = InputHistory::new();
        history.push("first".to_string());
        history.push("second".to_string());

        // Navigate back to "first"
        history.previous("");
        history.previous("");

        // Submit new item
        history.push("third".to_string());

        // Next previous should start from newest ("third")
        assert_eq!(history.previous(""), Some("third"));
    }

    #[test]
    fn test_unicode_and_long_input() {
        let mut history = InputHistory::new();
        let long_text = "A".repeat(1000);
        let unicode_text = "こんにちは 🚀 CRUDO";

        history.push(long_text.clone());
        history.push(unicode_text.to_string());

        assert_eq!(history.previous(""), Some(unicode_text));
        assert_eq!(history.previous(""), Some(long_text.as_str()));
    }
}
