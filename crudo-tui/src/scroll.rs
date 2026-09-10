#[derive(Debug, Clone)]
pub struct ScrollState {
    pub offset: u16,
    pub auto_scroll: bool,
    pub unseen_items: usize,
    pub max_offset: u16,
    pub page_size: u16,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0,
            auto_scroll: true,
            unseen_items: 0,
            max_offset: 0,
            page_size: 10,
        }
    }
}

impl ScrollState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_max_offset(&mut self, max_offset: u16) {
        self.max_offset = max_offset;
        if self.auto_scroll {
            self.offset = self.max_offset;
            self.unseen_items = 0;
        } else {
            // Keep offset clamped to max_offset
            if self.offset > self.max_offset {
                self.offset = self.max_offset;
            }
        }
    }

    pub fn set_viewport(&mut self, max_offset: u16, page_size: u16) {
        self.page_size = page_size;
        self.set_max_offset(max_offset);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.auto_scroll = false;
        self.offset = self.offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.offset = self.offset.saturating_add(amount).min(self.max_offset);
        if self.offset == self.max_offset {
            self.auto_scroll = true;
            self.unseen_items = 0;
        }
    }

    pub fn scroll_by_page(&mut self, direction: i32) {
        let amount = if self.page_size > 0 {
            self.page_size
        } else {
            10
        };
        if direction < 0 {
            self.scroll_up(amount);
        } else if direction > 0 {
            self.scroll_down(amount);
        }
    }

    pub fn scroll_to_top(&mut self) {
        self.home();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.end();
    }

    pub fn home(&mut self) {
        self.auto_scroll = false;
        self.offset = 0;
    }

    pub fn end(&mut self) {
        self.auto_scroll = true;
        self.offset = self.max_offset;
        self.unseen_items = 0;
    }

    pub fn notify_new_item(&mut self) {
        if !self.auto_scroll {
            self.unseen_items += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = ScrollState::new();
        assert_eq!(state.offset, 0);
        assert!(state.auto_scroll);
        assert_eq!(state.unseen_items, 0);
    }

    #[test]
    fn test_scroll_up_disables_auto_scroll() {
        let mut state = ScrollState::new();
        state.set_max_offset(10);
        assert_eq!(state.offset, 10);

        state.scroll_up(2);
        assert_eq!(state.offset, 8);
        assert!(!state.auto_scroll);
    }

    #[test]
    fn test_scroll_down_to_bottom_enables_auto_scroll() {
        let mut state = ScrollState::new();
        state.set_max_offset(10);
        state.scroll_up(5);
        assert!(!state.auto_scroll);

        state.scroll_down(5);
        assert_eq!(state.offset, 10);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_scroll_position_cannot_become_negative() {
        let mut state = ScrollState::new();
        state.set_max_offset(10);
        state.scroll_up(100);
        assert_eq!(state.offset, 0);
        assert!(!state.auto_scroll);

        state.scroll_up(1);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn test_scroll_position_cannot_exceed_max_content_range() {
        let mut state = ScrollState::new();
        state.set_max_offset(10);
        state.scroll_down(50);
        assert_eq!(state.offset, 10);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_scroll_position_clamped_when_max_offset_decreases() {
        let mut state = ScrollState::new();
        state.set_max_offset(20);
        state.scroll_up(5); // offset = 15, auto_scroll = false
        assert_eq!(state.offset, 15);

        // Content shrunk (e.g. window resize or content change)
        state.set_max_offset(10);
        assert_eq!(state.offset, 10);
        assert!(!state.auto_scroll);
    }

    #[test]
    fn test_adding_message_does_not_reset_manually_selected_scroll_position() {
        let mut state = ScrollState::new();
        state.set_max_offset(15);
        assert_eq!(state.offset, 15);
        assert!(state.auto_scroll);

        // User manually scrolls up to inspect history
        state.scroll_up(10);
        assert_eq!(state.offset, 5);
        assert!(!state.auto_scroll);

        // A new message arrives
        state.notify_new_item();
        assert_eq!(state.unseen_items, 1);

        // Rerender computes higher max_offset
        state.set_max_offset(25);
        // Manual position must remain 5, not jump to bottom 25
        assert_eq!(state.offset, 5);
        assert!(!state.auto_scroll);
    }

    #[test]
    fn test_home_and_end() {
        let mut state = ScrollState::new();
        state.set_max_offset(20);
        state.home();
        assert_eq!(state.offset, 0);
        assert!(!state.auto_scroll);

        state.end();
        assert_eq!(state.offset, 20);
        assert!(state.auto_scroll);
    }

    #[test]
    fn test_scroll_by_page_and_top_bottom() {
        let mut state = ScrollState::new();
        state.set_viewport(100, 15);
        assert_eq!(state.offset, 100);
        assert!(state.auto_scroll);

        // Page up by 15 lines
        state.scroll_by_page(-1);
        assert_eq!(state.offset, 85);
        assert!(!state.auto_scroll);

        // Page up again
        state.scroll_by_page(-1);
        assert_eq!(state.offset, 70);

        // Page down by 15 lines
        state.scroll_by_page(1);
        assert_eq!(state.offset, 85);

        // Jump to top
        state.scroll_to_top();
        assert_eq!(state.offset, 0);
        assert!(!state.auto_scroll);

        // Page down from top
        state.scroll_by_page(1);
        assert_eq!(state.offset, 15);

        // Jump to bottom
        state.scroll_to_bottom();
        assert_eq!(state.offset, 100);
        assert!(state.auto_scroll);
    }
}
