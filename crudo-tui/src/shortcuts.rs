use crate::app::{App, Focus};
use crate::command::AppCommand;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyContext {
    pub focus: Focus,
    pub is_agent_active: bool,
    pub is_command_palette_open: bool,
    pub is_permission_prompt_open: bool,
}

impl KeyContext {
    #[allow(dead_code)]
    pub fn new(focus: Focus, is_agent_active: bool) -> Self {
        Self {
            focus,
            is_agent_active,
            is_command_palette_open: false,
            is_permission_prompt_open: false,
        }
    }

    #[allow(dead_code)]
    pub fn with_palette(
        focus: Focus,
        is_agent_active: bool,
        is_command_palette_open: bool,
    ) -> Self {
        Self {
            focus,
            is_agent_active,
            is_command_palette_open,
            is_permission_prompt_open: false,
        }
    }

    #[allow(dead_code)]
    pub fn with_permission(
        focus: Focus,
        is_agent_active: bool,
        is_permission_prompt_open: bool,
    ) -> Self {
        Self {
            focus,
            is_agent_active,
            is_command_palette_open: false,
            is_permission_prompt_open,
        }
    }

    pub fn from_app(app: &App) -> Self {
        Self {
            focus: app.focus,
            is_agent_active: app.is_agent_active(),
            is_command_palette_open: app.palette_state.is_open,
            is_permission_prompt_open: app.pending_permission.is_some(),
        }
    }
}

pub fn map_key_to_command(key: KeyEvent, ctx: &KeyContext) -> Option<AppCommand> {
    let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // 1. Control modifier bindings and control character codes
    if (is_ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')))
        || key.code == KeyCode::Char('\x03')
    {
        return Some(AppCommand::Quit);
    }

    if (is_ctrl && matches!(key.code, KeyCode::Char('u') | KeyCode::Char('U')))
        || key.code == KeyCode::Char('\x15')
    {
        return Some(AppCommand::ClearInput);
    }

    if (is_ctrl && matches!(key.code, KeyCode::Char('k') | KeyCode::Char('K')))
        || key.code == KeyCode::Char('\x0b')
    {
        if ctx.is_command_palette_open {
            return Some(AppCommand::CloseCommandPalette);
        } else {
            return Some(AppCommand::OpenCommandPalette);
        }
    }

    // 2. When Permission Prompt is open, it captures modal keyboard interaction
    if ctx.is_permission_prompt_open {
        return match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                Some(AppCommand::PermissionToggleChoice)
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('a') | KeyCode::Char('A') => {
                Some(AppCommand::PermissionSelectAllow)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('d') | KeyCode::Char('D') => {
                Some(AppCommand::PermissionSelectDeny)
            }
            KeyCode::Enter => Some(AppCommand::PermissionConfirm),
            KeyCode::Esc => Some(AppCommand::PermissionSelectDeny),
            _ => None,
        };
    }

    // 3. When Command Palette is open, it captures keyboard interaction
    if ctx.is_command_palette_open {
        return match key.code {
            KeyCode::Esc => Some(AppCommand::CloseCommandPalette),
            KeyCode::Up => Some(AppCommand::PalettePrevious),
            KeyCode::Down => Some(AppCommand::PaletteNext),
            KeyCode::Enter => Some(AppCommand::PaletteSelect),
            KeyCode::Backspace => Some(AppCommand::PaletteDeleteBackward),
            KeyCode::Char(c) => Some(AppCommand::PaletteInsertChar(c)),
            _ => None,
        };
    }

    // 2. Global navigation and modal toggles
    if key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT) {
        return Some(AppCommand::FocusPrevious);
    }

    match key.code {
        KeyCode::F(2) => return Some(AppCommand::ToggleActivity),
        KeyCode::Tab => return Some(AppCommand::FocusNext),
        KeyCode::BackTab => return Some(AppCommand::FocusPrevious),
        KeyCode::PageUp => return Some(AppCommand::PageUp),
        KeyCode::PageDown => return Some(AppCommand::PageDown),
        KeyCode::Home => return Some(AppCommand::ScrollToTop),
        KeyCode::End => return Some(AppCommand::ScrollToBottom),
        KeyCode::Esc => {
            if ctx.is_agent_active {
                return Some(AppCommand::CancelAgent);
            } else {
                return Some(AppCommand::Escape);
            }
        }
        _ => {}
    }

    // 3. Context-sensitive key bindings based on current focus
    match ctx.focus {
        Focus::Chat => match key.code {
            KeyCode::Up => Some(AppCommand::ScrollUp),
            KeyCode::Down => Some(AppCommand::ScrollDown),
            _ => None,
        },
        Focus::Activity => match key.code {
            KeyCode::Up => Some(AppCommand::ScrollUp),
            KeyCode::Down => Some(AppCommand::ScrollDown),
            _ => None,
        },
        Focus::Input => {
            // Block text entry, editing, and history navigation while agent is actively processing
            if ctx.is_agent_active {
                return None;
            }

            // Up/Down in Input navigates input history
            if key.code == KeyCode::Up {
                return Some(AppCommand::InputHistoryPrevious);
            }
            if key.code == KeyCode::Down {
                return Some(AppCommand::InputHistoryNext);
            }

            match key.code {
                KeyCode::Enter | KeyCode::Char('\n') | KeyCode::Char('\r') => {
                    Some(AppCommand::Submit)
                }
                KeyCode::Char(c) => Some(AppCommand::InsertChar(c)),
                KeyCode::Left => Some(AppCommand::CursorLeft),
                KeyCode::Right => Some(AppCommand::CursorRight),
                KeyCode::Backspace => Some(AppCommand::DeleteBackward),
                KeyCode::Delete => Some(AppCommand::DeleteForward),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[test]
    fn test_enter_submits_when_input_focused() {
        let ctx = KeyContext::new(Focus::Input, false);
        let key = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(map_key_to_command(key, &ctx), Some(AppCommand::Submit));

        let key_lf = make_key_event(KeyCode::Char('\n'), KeyModifiers::NONE);
        assert_eq!(map_key_to_command(key_lf, &ctx), Some(AppCommand::Submit));

        let key_cr = make_key_event(KeyCode::Char('\r'), KeyModifiers::NONE);
        assert_eq!(map_key_to_command(key_cr, &ctx), Some(AppCommand::Submit));
    }

    #[test]
    fn test_enter_blocked_when_agent_active() {
        let ctx = KeyContext::new(Focus::Input, true);
        let key = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(map_key_to_command(key, &ctx), None);
    }

    #[test]
    fn test_f2_toggles_activity() {
        let contexts = [
            KeyContext::new(Focus::Input, false),
            KeyContext::new(Focus::Input, true),
            KeyContext::new(Focus::Chat, false),
            KeyContext::new(Focus::Activity, false),
        ];

        let key = make_key_event(KeyCode::F(2), KeyModifiers::NONE);
        for ctx in &contexts {
            assert_eq!(
                map_key_to_command(key, ctx),
                Some(AppCommand::ToggleActivity)
            );
        }
    }

    #[test]
    fn test_ctrl_c_quits() {
        let ctx_idle = KeyContext::new(Focus::Input, false);
        let ctx_running = KeyContext::new(Focus::Input, true);
        let key = make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);

        assert_eq!(map_key_to_command(key, &ctx_idle), Some(AppCommand::Quit));
        assert_eq!(
            map_key_to_command(key, &ctx_running),
            Some(AppCommand::Quit)
        );
    }

    #[test]
    fn test_ctrl_u_clears_input() {
        let ctx = KeyContext::new(Focus::Input, false);
        let key = make_key_event(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(map_key_to_command(key, &ctx), Some(AppCommand::ClearInput));
    }

    #[test]
    fn test_page_up_and_page_down() {
        let contexts = [
            KeyContext::new(Focus::Input, false),
            KeyContext::new(Focus::Chat, false),
            KeyContext::new(Focus::Activity, false),
        ];

        let pgup = make_key_event(KeyCode::PageUp, KeyModifiers::NONE);
        let pgdn = make_key_event(KeyCode::PageDown, KeyModifiers::NONE);

        for ctx in &contexts {
            let cmd_up = map_key_to_command(pgup, ctx);
            let cmd_dn = map_key_to_command(pgdn, ctx);

            assert_eq!(cmd_up, Some(AppCommand::PageUp));
            assert_ne!(cmd_up, Some(AppCommand::ScrollToTop));

            assert_eq!(cmd_dn, Some(AppCommand::PageDown));
            assert_ne!(cmd_dn, Some(AppCommand::ScrollToBottom));
        }
    }

    #[test]
    fn test_home_and_end_in_chat() {
        let ctx = KeyContext::new(Focus::Chat, false);
        let home = make_key_event(KeyCode::Home, KeyModifiers::NONE);
        let end = make_key_event(KeyCode::End, KeyModifiers::NONE);

        assert_eq!(
            map_key_to_command(home, &ctx),
            Some(AppCommand::ScrollToTop)
        );
        assert_eq!(
            map_key_to_command(end, &ctx),
            Some(AppCommand::ScrollToBottom)
        );
    }

    #[test]
    fn test_home_and_end_in_activity() {
        let ctx = KeyContext::new(Focus::Activity, false);
        let home = make_key_event(KeyCode::Home, KeyModifiers::NONE);
        let end = make_key_event(KeyCode::End, KeyModifiers::NONE);

        assert_eq!(
            map_key_to_command(home, &ctx),
            Some(AppCommand::ScrollToTop)
        );
        assert_eq!(
            map_key_to_command(end, &ctx),
            Some(AppCommand::ScrollToBottom)
        );
    }

    #[test]
    fn test_home_and_end_in_input() {
        let ctx = KeyContext::new(Focus::Input, false);
        let home = make_key_event(KeyCode::Home, KeyModifiers::NONE);
        let end = make_key_event(KeyCode::End, KeyModifiers::NONE);

        assert_eq!(
            map_key_to_command(home, &ctx),
            Some(AppCommand::ScrollToTop)
        );
        assert_eq!(
            map_key_to_command(end, &ctx),
            Some(AppCommand::ScrollToBottom)
        );
    }

    #[test]
    fn test_esc_escapes() {
        let ctx = KeyContext::new(Focus::Input, false);
        let key = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(map_key_to_command(key, &ctx), Some(AppCommand::Escape));
    }

    #[test]
    fn test_esc_cancels_when_agent_active() {
        let ctx_active = KeyContext::new(Focus::Input, true);
        let key = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(key, &ctx_active),
            Some(AppCommand::CancelAgent)
        );

        // When palette is open and agent is active, Esc closes palette, does NOT cancel agent directly
        let ctx_palette = KeyContext::with_palette(Focus::Input, true, true);
        assert_eq!(
            map_key_to_command(key, &ctx_palette),
            Some(AppCommand::CloseCommandPalette)
        );
    }

    #[test]
    fn test_ordinary_character_in_input() {
        let ctx = KeyContext::new(Focus::Input, false);
        let key_a = make_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        let key_h = make_key_event(KeyCode::Char('H'), KeyModifiers::SHIFT);

        assert_eq!(
            map_key_to_command(key_a, &ctx),
            Some(AppCommand::InsertChar('a'))
        );
        assert_eq!(
            map_key_to_command(key_h, &ctx),
            Some(AppCommand::InsertChar('H'))
        );

        // When agent is active, characters are ignored
        let ctx_active = KeyContext::new(Focus::Input, true);
        assert_eq!(map_key_to_command(key_a, &ctx_active), None);
    }

    #[test]
    fn test_backspace_and_delete_in_input() {
        let ctx = KeyContext::new(Focus::Input, false);
        let bs = make_key_event(KeyCode::Backspace, KeyModifiers::NONE);
        let del = make_key_event(KeyCode::Delete, KeyModifiers::NONE);

        assert_eq!(
            map_key_to_command(bs, &ctx),
            Some(AppCommand::DeleteBackward)
        );
        assert_eq!(
            map_key_to_command(del, &ctx),
            Some(AppCommand::DeleteForward)
        );
    }

    #[test]
    fn test_arrow_keys_in_input() {
        let ctx = KeyContext::new(Focus::Input, false);
        let left = make_key_event(KeyCode::Left, KeyModifiers::NONE);
        let right = make_key_event(KeyCode::Right, KeyModifiers::NONE);
        let up = make_key_event(KeyCode::Up, KeyModifiers::NONE);
        let down = make_key_event(KeyCode::Down, KeyModifiers::NONE);

        assert_eq!(map_key_to_command(left, &ctx), Some(AppCommand::CursorLeft));
        assert_eq!(
            map_key_to_command(right, &ctx),
            Some(AppCommand::CursorRight)
        );
        assert_eq!(
            map_key_to_command(up, &ctx),
            Some(AppCommand::InputHistoryPrevious)
        );
        assert_eq!(
            map_key_to_command(down, &ctx),
            Some(AppCommand::InputHistoryNext)
        );

        // When agent is active, Up/Down in Input are blocked
        let ctx_active = KeyContext::new(Focus::Input, true);
        assert_eq!(map_key_to_command(up, &ctx_active), None);
        assert_eq!(map_key_to_command(down, &ctx_active), None);
    }

    #[test]
    fn test_arrow_keys_in_chat_and_activity() {
        let ctx_chat = KeyContext::new(Focus::Chat, false);
        let ctx_act = KeyContext::new(Focus::Activity, false);
        let up = make_key_event(KeyCode::Up, KeyModifiers::NONE);
        let down = make_key_event(KeyCode::Down, KeyModifiers::NONE);

        assert_eq!(
            map_key_to_command(up, &ctx_chat),
            Some(AppCommand::ScrollUp)
        );
        assert_eq!(
            map_key_to_command(down, &ctx_chat),
            Some(AppCommand::ScrollDown)
        );

        assert_eq!(map_key_to_command(up, &ctx_act), Some(AppCommand::ScrollUp));
        assert_eq!(
            map_key_to_command(down, &ctx_act),
            Some(AppCommand::ScrollDown)
        );
    }

    #[test]
    fn test_tab_and_backtab_focus_switching() {
        let ctx = KeyContext::new(Focus::Input, false);
        let tab = make_key_event(KeyCode::Tab, KeyModifiers::NONE);
        let backtab = make_key_event(KeyCode::BackTab, KeyModifiers::NONE);
        let shift_tab = make_key_event(KeyCode::Tab, KeyModifiers::SHIFT);

        assert_eq!(map_key_to_command(tab, &ctx), Some(AppCommand::FocusNext));
        assert_eq!(
            map_key_to_command(backtab, &ctx),
            Some(AppCommand::FocusPrevious)
        );
        assert_eq!(
            map_key_to_command(shift_tab, &ctx),
            Some(AppCommand::FocusPrevious)
        );
    }

    #[test]
    fn test_single_command_produced_per_event() {
        let ctx = KeyContext::new(Focus::Input, false);
        let key = make_key_event(KeyCode::Char('x'), KeyModifiers::NONE);
        let cmd = map_key_to_command(key, &ctx);
        assert_eq!(cmd, Some(AppCommand::InsertChar('x')));
    }

    #[test]
    fn test_no_duplicate_characters_or_submits() {
        let ctx = KeyContext::new(Focus::Input, false);

        // Character key does not produce Submit or any other command
        let char_key = make_key_event(KeyCode::Char('z'), KeyModifiers::NONE);
        let cmd = map_key_to_command(char_key, &ctx).unwrap();
        assert_eq!(cmd, AppCommand::InsertChar('z'));
        assert_ne!(cmd, AppCommand::Submit);

        // Enter key does not produce InsertChar
        let enter_key = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        let cmd_enter = map_key_to_command(enter_key, &ctx).unwrap();
        assert_eq!(cmd_enter, AppCommand::Submit);
        assert_ne!(cmd_enter, AppCommand::InsertChar('\n'));
    }

    #[test]
    fn test_ctrl_k_toggles_palette() {
        let input_ctx = KeyContext::new(Focus::Input, false);
        let chat_ctx = KeyContext::new(Focus::Chat, false);
        let activity_ctx = KeyContext::new(Focus::Activity, false);

        let ctrl_k = make_key_event(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key_to_command(ctrl_k, &input_ctx),
            Some(AppCommand::OpenCommandPalette)
        );
        assert_eq!(
            map_key_to_command(ctrl_k, &chat_ctx),
            Some(AppCommand::OpenCommandPalette)
        );
        assert_eq!(
            map_key_to_command(ctrl_k, &activity_ctx),
            Some(AppCommand::OpenCommandPalette)
        );

        // When palette is already open, Ctrl+K closes it
        let open_ctx = KeyContext::with_palette(Focus::Input, false, true);
        assert_eq!(
            map_key_to_command(ctrl_k, &open_ctx),
            Some(AppCommand::CloseCommandPalette)
        );

        // Raw control code \x0b (ASCII 11 / ^K) behaves identically
        let raw_ctrl_k = make_key_event(KeyCode::Char('\x0b'), KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(raw_ctrl_k, &input_ctx),
            Some(AppCommand::OpenCommandPalette)
        );
        assert_eq!(
            map_key_to_command(raw_ctrl_k, &open_ctx),
            Some(AppCommand::CloseCommandPalette)
        );
    }

    #[test]
    fn test_ctrl_k_is_standalone_shortcut_not_chord() {
        let ctx = KeyContext::new(Focus::Input, false);
        let ctrl_k = make_key_event(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key_to_command(ctrl_k, &ctx),
            Some(AppCommand::OpenCommandPalette)
        );
    }

    #[test]
    fn test_palette_open_captures_keys() {
        let ctx = KeyContext::with_palette(Focus::Input, false, true);

        // Esc closes palette
        let esc = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(esc, &ctx),
            Some(AppCommand::CloseCommandPalette)
        );

        // Navigation
        let up = make_key_event(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(up, &ctx),
            Some(AppCommand::PalettePrevious)
        );

        let down = make_key_event(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(down, &ctx),
            Some(AppCommand::PaletteNext)
        );

        // Execution
        let enter = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(enter, &ctx),
            Some(AppCommand::PaletteSelect)
        );

        // Typing query
        let char_a = make_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(char_a, &ctx),
            Some(AppCommand::PaletteInsertChar('a'))
        );

        let backspace = make_key_event(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(backspace, &ctx),
            Some(AppCommand::PaletteDeleteBackward)
        );

        // Normal shortcuts are NOT leaked while palette is open
        let tab = make_key_event(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(map_key_to_command(tab, &ctx), None);

        let f2 = make_key_event(KeyCode::F(2), KeyModifiers::NONE);
        assert_eq!(map_key_to_command(f2, &ctx), None);

        let page_up = make_key_event(KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(map_key_to_command(page_up, &ctx), None);
    }

    #[test]
    fn test_permission_prompt_captures_keys() {
        let ctx = KeyContext::with_permission(Focus::Input, true, true);

        // Arrow keys and tab toggle choices
        let left = make_key_event(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(left, &ctx),
            Some(AppCommand::PermissionToggleChoice)
        );
        let right = make_key_event(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(right, &ctx),
            Some(AppCommand::PermissionToggleChoice)
        );
        let tab = make_key_event(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(tab, &ctx),
            Some(AppCommand::PermissionToggleChoice)
        );

        // Confirmation with Enter
        let enter = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(enter, &ctx),
            Some(AppCommand::PermissionConfirm)
        );

        // Direct Allow with 'y' or 'a'
        let y = make_key_event(KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(y, &ctx),
            Some(AppCommand::PermissionSelectAllow)
        );
        let a = make_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(a, &ctx),
            Some(AppCommand::PermissionSelectAllow)
        );

        // Direct Deny with 'n', 'd', or Esc
        let n = make_key_event(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(n, &ctx),
            Some(AppCommand::PermissionSelectDeny)
        );
        let d = make_key_event(KeyCode::Char('d'), KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(d, &ctx),
            Some(AppCommand::PermissionSelectDeny)
        );
        let esc = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(
            map_key_to_command(esc, &ctx),
            Some(AppCommand::PermissionSelectDeny)
        );

        // Quit with Ctrl+C still takes precedence
        let ctrl_c = make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key_to_command(ctrl_c, &ctx), Some(AppCommand::Quit));
    }
}
