pub struct InputState {
    value: String,
    char_index: usize,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            char_index: 0,
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn cursor(&self) -> usize {
        self.char_index
    }

    fn byte_index(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    pub fn insert_char(&mut self, c: char) {
        let idx = self.byte_index();
        self.value.insert(idx, c);
        self.char_index += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
        let idx = self.byte_index();
        self.value.insert_str(idx, &normalized);
        self.char_index += normalized.chars().count();
    }

    pub fn move_cursor_left(&mut self) {
        if self.char_index > 0 {
            self.char_index -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.char_index < self.value.chars().count() {
            self.char_index += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.char_index = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.char_index = self.value.chars().count();
    }

    pub fn delete_backward(&mut self) {
        if self.char_index > 0 {
            self.char_index -= 1;
            let start = self.byte_index();
            let c_len = self.value[start..].chars().next().unwrap().len_utf8();
            self.value.drain(start..start + c_len);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.char_index < self.value.chars().count() {
            let start = self.byte_index();
            let c_len = self.value[start..].chars().next().unwrap().len_utf8();
            self.value.drain(start..start + c_len);
        }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.char_index = 0;
    }

    pub fn set_value(&mut self, val: String) {
        self.char_index = val.chars().count();
        self.value = val;
    }

    pub fn visual_cursor(&self) -> u16 {
        use unicode_width::UnicodeWidthStr;
        let start = self.byte_index();
        let slice = &self.value[..start];
        slice.width() as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_cursor() {
        let mut input = InputState::new();
        input.insert_char('H');
        input.insert_char('e');
        assert_eq!(input.value(), "He");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');

        input.move_cursor_left();
        assert_eq!(input.cursor(), 2);

        input.move_cursor_home();
        assert_eq!(input.cursor(), 0);

        input.move_cursor_right();
        assert_eq!(input.cursor(), 1);

        input.move_cursor_end();
        assert_eq!(input.cursor(), 3);
    }

    #[test]
    fn test_deletion() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');

        // a b c_
        input.delete_backward();
        assert_eq!(input.value(), "ab");
        assert_eq!(input.cursor(), 2);

        input.move_cursor_left(); // a b_
        input.delete_forward(); // should delete nothing since at end, but wait cursor was 2, now 1. 'a _ b' Wait, value is "ab", char_index is 1. delete_forward deletes 'b'
        assert_eq!(input.value(), "a");
        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn test_unicode() {
        let mut input = InputState::new();
        input.insert_char('😀');
        input.insert_char('é');
        assert_eq!(input.value(), "😀é");

        input.move_cursor_left();
        input.delete_backward();
        assert_eq!(input.value(), "é");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn test_clear() {
        let mut input = InputState::new();
        input.insert_char('a');
        input.clear();
        assert_eq!(input.value(), "");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn test_set_value() {
        let mut input = InputState::new();
        input.set_value("recalled text".to_string());
        assert_eq!(input.value(), "recalled text");
        assert_eq!(input.cursor(), 13);
    }

    #[test]
    fn test_insert_str_empty_input() {
        let mut input = InputState::new();
        input.insert_str("hello world");
        assert_eq!(input.value(), "hello world");
        assert_eq!(input.cursor(), 11);
    }

    #[test]
    fn test_insert_str_in_middle() {
        let mut input = InputState::new();
        input.insert_str("helloworld");
        // Move cursor 5 steps left to place between "hello" and "world"
        for _ in 0..5 {
            input.move_cursor_left();
        }
        assert_eq!(input.cursor(), 5);

        input.insert_str(" beautiful ");
        assert_eq!(input.value(), "hello beautiful world");
        assert_eq!(input.cursor(), 16);
    }

    #[test]
    fn test_insert_str_at_end() {
        let mut input = InputState::new();
        input.insert_str("foo");
        assert_eq!(input.cursor(), 3);
        input.insert_str(" bar");
        assert_eq!(input.value(), "foo bar");
        assert_eq!(input.cursor(), 7);
    }

    #[test]
    fn test_insert_str_multiline() {
        let mut input = InputState::new();
        input.insert_str("line 1\nline 2\r\nline 3");
        assert_eq!(input.value(), "line 1\nline 2\nline 3");
        assert_eq!(input.cursor(), "line 1\nline 2\nline 3".chars().count());
    }

    #[test]
    fn test_insert_str_followed_by_normal_typing() {
        let mut input = InputState::new();
        input.insert_str("pasted");
        input.insert_char(' ');
        input.insert_char('t');
        input.insert_char('e');
        input.insert_char('x');
        input.insert_char('t');
        assert_eq!(input.value(), "pasted text");
        assert_eq!(input.cursor(), 11);
    }

    #[test]
    fn test_insert_str_unicode() {
        let mut input = InputState::new();
        input.insert_str("🦀");
        assert_eq!(input.value(), "🦀");
        assert_eq!(input.cursor(), 1);

        input.insert_str(" Rust 🚀");
        assert_eq!(input.value(), "🦀 Rust 🚀");
        assert_eq!(input.cursor(), 8);
    }
}
