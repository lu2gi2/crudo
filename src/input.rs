#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// The actual text buffer typed by the user.
    /// Under no circumstances is placeholder text placed here.
    buffer: String,
    /// Byte offset of the cursor within the buffer, guaranteed on a valid UTF-8 char boundary.
    cursor: usize,
    /// Prompt history.
    history: Vec<String>,
    /// Current pointer in history when navigating Up/Down.
    history_index: Option<usize>,
    /// Temporary storage for the user's unsubmitted draft while scrolling history.
    history_draft: String,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
        }
    }

    /// Access the raw buffer string.
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Returns true if the user has entered no text.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Returns the length in bytes of the actual buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the cursor position in UTF-8 characters (useful for visual column positioning).
    pub fn cursor_char_offset(&self) -> usize {
        self.buffer[..self.cursor].chars().count()
    }

    /// Returns the byte index of the cursor.
    pub fn cursor_byte_idx(&self) -> usize {
        self.cursor
    }

    /// Inserts a character at the current cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Inserts a string at the current cursor position.
    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            self.insert_char(c);
        }
    }

    /// Deletes the character immediately preceding the cursor (Backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        if let Some((prev_idx, _)) = self.buffer[..self.cursor].char_indices().last() {
            self.buffer.remove(prev_idx);
            self.cursor = prev_idx;
        }
    }

    /// Deletes the character at the current cursor position (Delete).
    pub fn delete(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }

        self.buffer.remove(self.cursor);
    }

    /// Moves the cursor one character to the left.
    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        if let Some((prev_idx, _)) = self.buffer[..self.cursor].char_indices().last() {
            self.cursor = prev_idx;
        }
    }

    /// Moves the cursor one character to the right.
    pub fn move_right(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }

        if let Some((next_idx, c)) = self.buffer[self.cursor..].char_indices().next() {
            self.cursor += next_idx + c.len_utf8();
        }
    }

    /// Moves cursor to the start of the buffer.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves cursor to the end of the buffer.
    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Deletes all characters from cursor to end of line (Ctrl+K).
    pub fn delete_to_end(&mut self) {
        self.buffer.truncate(self.cursor);
    }

    /// Deletes all characters before the cursor (Ctrl+U).
    pub fn delete_to_start(&mut self) {
        self.buffer = self.buffer[self.cursor..].to_string();
        self.cursor = 0;
    }

    /// Deletes the previous word (Ctrl+W).
    pub fn delete_prev_word(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let prefix = &self.buffer[..self.cursor];
        let trimmed = prefix.trim_end();
        let target_idx =
            match trimmed.rfind(|c: char| c.is_whitespace() || c == '/' || c == '-' || c == '_') {
                Some(idx) => idx + 1,
                None => 0,
            };

        let remainder = self.buffer[self.cursor..].to_string();
        self.buffer = format!("{}{}", &self.buffer[..target_idx], remainder);
        self.cursor = target_idx;
    }

    /// Submits the current input buffer, adds it to history, and resets input state.
    pub fn submit(&mut self) -> String {
        let submitted = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        self.history_index = None;
        self.history_draft.clear();

        let trimmed = submitted.trim().to_string();
        if !trimmed.is_empty() {
            // Avoid immediate duplicate in history
            if self.history.last().map(|s| s.as_str()) != Some(&trimmed) {
                self.history.push(trimmed);
            }
        }

        submitted
    }

    /// Clears the input buffer completely.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.history_index = None;
        self.history_draft.clear();
    }

    /// Sets the buffer to a specific string and positions cursor at the end.
    pub fn set_text(&mut self, text: &str) {
        self.buffer = text.to_string();
        self.cursor = self.buffer.len();
    }

    /// Navigates up in command/prompt history.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Save draft
                self.history_draft = self.buffer.clone();
                let last_idx = self.history.len() - 1;
                self.history_index = Some(last_idx);
                self.buffer = self.history[last_idx].clone();
                self.cursor = self.buffer.len();
            }
            Some(idx) if idx > 0 => {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.buffer = self.history[new_idx].clone();
                self.cursor = self.buffer.len();
            }
            Some(_) => {} // Already at oldest history
        }
    }

    /// Navigates down in command/prompt history.
    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                let new_idx = idx + 1;
                self.history_index = Some(new_idx);
                self.buffer = self.history[new_idx].clone();
                self.cursor = self.buffer.len();
            } else {
                // Return to user's saved draft
                self.history_index = None;
                self.buffer = std::mem::take(&mut self.history_draft);
                self.cursor = self.buffer.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input_and_placeholder_isolation() {
        let input = InputState::new();
        // Section 46 Requirement:
        // EMPTY INPUT: input buffer: "", placeholder: "Type a message... /help for commands"
        // Expected cursor position = beginning of actual input (0), NOT placeholder length.
        assert_eq!(input.buffer(), "");
        assert_eq!(input.cursor_char_offset(), 0);
        assert_eq!(input.cursor_byte_idx(), 0);
        assert!(input.is_empty());
    }

    #[test]
    fn test_typing_and_cursor() {
        let mut input = InputState::new();
        input.insert_str("how do machines work?");
        assert_eq!(input.buffer(), "how do machines work?");
        assert_eq!(input.cursor_char_offset(), 21);

        input.move_home();
        assert_eq!(input.cursor_char_offset(), 0);

        input.move_right();
        input.move_right();
        assert_eq!(input.cursor_char_offset(), 2);

        input.insert_char('x');
        assert_eq!(input.buffer(), "hoxw do machines work?");
        assert_eq!(input.cursor_char_offset(), 3);

        input.backspace();
        assert_eq!(input.buffer(), "how do machines work?");
        assert_eq!(input.cursor_char_offset(), 2);
    }

    #[test]
    fn test_submission_and_history() {
        let mut input = InputState::new();
        input.insert_str("first prompt");
        let submitted = input.submit();
        assert_eq!(submitted, "first prompt");
        assert_eq!(input.buffer(), "");
        assert_eq!(input.cursor_char_offset(), 0);

        // Test history navigation
        input.insert_str("draft in progress");
        input.history_prev();
        assert_eq!(input.buffer(), "first prompt");

        input.history_next();
        assert_eq!(input.buffer(), "draft in progress");
    }

    #[test]
    fn test_unicode_boundary_safety() {
        let mut input = InputState::new();
        input.insert_str("⚙ CRUDO ▶");
        assert_eq!(input.cursor_char_offset(), 9);
        input.backspace();
        assert_eq!(input.buffer(), "⚙ CRUDO ");
        input.delete_prev_word();
        assert_eq!(input.buffer(), "⚙ ");
    }
}
