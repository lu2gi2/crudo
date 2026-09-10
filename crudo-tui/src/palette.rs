use crate::command::AppCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteCommand {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub action: AppCommand,
    pub shortcut: Option<&'static str>,
}

pub fn default_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            id: "toggle_activity",
            name: "Toggle Activity Panel",
            description: "Show or hide the activity and tool log panel",
            action: AppCommand::ToggleActivity,
            shortcut: Some("F2"),
        },
        PaletteCommand {
            id: "scroll_up",
            name: "Scroll Up",
            description: "Scroll active view up by 1 line",
            action: AppCommand::ScrollUp,
            shortcut: Some("Up"),
        },
        PaletteCommand {
            id: "scroll_down",
            name: "Scroll Down",
            description: "Scroll active view down by 1 line",
            action: AppCommand::ScrollDown,
            shortcut: Some("Down"),
        },
        PaletteCommand {
            id: "page_up",
            name: "Page Up",
            description: "Scroll chat history up by one page",
            action: AppCommand::PageUp,
            shortcut: Some("PageUp"),
        },
        PaletteCommand {
            id: "page_down",
            name: "Page Down",
            description: "Scroll chat history down by one page",
            action: AppCommand::PageDown,
            shortcut: Some("PageDown"),
        },
        PaletteCommand {
            id: "scroll_to_top",
            name: "Scroll to Top",
            description: "Jump to the very top of chat history",
            action: AppCommand::ScrollToTop,
            shortcut: Some("Home"),
        },
        PaletteCommand {
            id: "scroll_to_bottom",
            name: "Scroll to Bottom",
            description: "Jump to the very bottom of chat history",
            action: AppCommand::ScrollToBottom,
            shortcut: Some("End"),
        },
        PaletteCommand {
            id: "focus_next",
            name: "Next Panel",
            description: "Switch focus to the next panel",
            action: AppCommand::FocusNext,
            shortcut: Some("Tab"),
        },
        PaletteCommand {
            id: "focus_previous",
            name: "Previous Panel",
            description: "Switch focus to the previous panel",
            action: AppCommand::FocusPrevious,
            shortcut: Some("Shift+Tab"),
        },
        PaletteCommand {
            id: "clear_input",
            name: "Clear Input",
            description: "Clear current prompt in the input box",
            action: AppCommand::ClearInput,
            shortcut: Some("Ctrl+U"),
        },
        PaletteCommand {
            id: "cancel_agent",
            name: "Cancel Agent",
            description: "Stop the currently running agent task",
            action: AppCommand::CancelAgent,
            shortcut: Some("Esc"),
        },
        PaletteCommand {
            id: "quit",
            name: "Quit",
            description: "Exit CRUDO",
            action: AppCommand::Quit,
            shortcut: Some("Ctrl+C"),
        },
    ]
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
        self.selected_index = 0;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
        self.selected_index = 0;
    }

    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.selected_index = 0;
    }

    pub fn delete_backward(&mut self) {
        self.query.pop();
        self.selected_index = 0;
    }

    pub fn visual_cursor(&self) -> u16 {
        use unicode_width::UnicodeWidthStr;
        self.query.as_str().width() as u16
    }

    pub fn filtered_commands<'a>(&self, commands: &'a [PaletteCommand]) -> Vec<&'a PaletteCommand> {
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            commands.iter().collect()
        } else {
            commands
                .iter()
                .filter(|cmd| {
                    cmd.name.to_lowercase().contains(&q)
                        || cmd.description.to_lowercase().contains(&q)
                        || cmd.id.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn select_previous(&mut self, match_count: usize) {
        if match_count == 0 {
            self.selected_index = 0;
        } else if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn select_next(&mut self, match_count: usize) {
        if match_count == 0 {
            self.selected_index = 0;
        } else if self.selected_index + 1 < match_count {
            self.selected_index += 1;
        }
    }

    pub fn selected_command<'a>(
        &self,
        commands: &'a [PaletteCommand],
    ) -> Option<&'a PaletteCommand> {
        let matches = self.filtered_commands(commands);
        if matches.is_empty() {
            None
        } else {
            let idx = self.selected_index.min(matches.len() - 1);
            Some(matches[idx])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_starts_closed() {
        let state = CommandPaletteState::new();
        assert!(!state.is_open);
        assert_eq!(state.query, "");
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_open_close_toggle() {
        let mut state = CommandPaletteState::new();
        state.open();
        assert!(state.is_open);

        state.close();
        assert!(!state.is_open);

        state.toggle();
        assert!(state.is_open);

        state.toggle();
        assert!(!state.is_open);
    }

    #[test]
    fn test_case_insensitive_filtering() {
        let commands = default_commands();
        let mut state = CommandPaletteState::new();

        state.query = "SCROLL".to_string();
        let matches = state.filtered_commands(&commands);
        assert_eq!(matches.len(), 6); // 4 scroll commands + 2 page scroll commands

        for m in matches {
            let matches_term = m.name.to_lowercase().contains("scroll")
                || m.description.to_lowercase().contains("scroll")
                || m.id.to_lowercase().contains("scroll");
            assert!(matches_term);
        }

        state.query = "pAgE".to_string();
        let page_matches = state.filtered_commands(&commands);
        assert_eq!(page_matches.len(), 2);
        for m in page_matches {
            assert!(m.name.to_lowercase().contains("page"));
        }
    }

    #[test]
    fn test_empty_search_results() {
        let commands = default_commands();
        let mut state = CommandPaletteState::new();

        state.query = "nonexistent_command_xyz".to_string();
        let matches = state.filtered_commands(&commands);
        assert!(matches.is_empty());
        assert_eq!(state.selected_command(&commands), None);
    }

    #[test]
    fn test_up_down_navigation_and_clamping() {
        let commands = default_commands();
        let mut state = CommandPaletteState::new();
        let count = commands.len();

        assert_eq!(state.selected_index, 0);

        // Up at boundary 0 stays at 0 (no wrap)
        state.select_previous(count);
        assert_eq!(state.selected_index, 0);

        // Down advances
        state.select_next(count);
        assert_eq!(state.selected_index, 1);

        state.select_next(count);
        assert_eq!(state.selected_index, 2);

        // Up moves back
        state.select_previous(count);
        assert_eq!(state.selected_index, 1);

        // Move to last
        for _ in 0..count + 5 {
            state.select_next(count);
        }
        assert_eq!(state.selected_index, count - 1);

        // Down at last boundary stays at last (no wrap)
        state.select_next(count);
        assert_eq!(state.selected_index, count - 1);
    }

    #[test]
    fn test_selected_command_selection() {
        let commands = default_commands();
        let mut state = CommandPaletteState::new();

        state.query = "activity".to_string();
        let matches = state.filtered_commands(&commands);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Toggle Activity Panel");

        let selected = state.selected_command(&commands).unwrap();
        assert_eq!(selected.action, AppCommand::ToggleActivity);
    }

    #[test]
    fn test_insert_and_delete_query() {
        let mut state = CommandPaletteState::new();
        state.insert_char('p');
        state.insert_char('a');
        state.insert_char('g');
        state.insert_char('e');
        assert_eq!(state.query, "page");

        state.delete_backward();
        assert_eq!(state.query, "pag");
    }
}
