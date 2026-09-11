use crate::message::Role;
use crate::palette::PaletteCommand;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    Quit,
    Escape,
    ToggleActivity,
    FocusNext,
    FocusPrevious,
    PageUp,
    PageDown,
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    Submit,
    InsertChar(char),
    Paste(String),
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    DeleteBackward,
    DeleteForward,
    ClearInput,
    InputHistoryPrevious,
    InputHistoryNext,
    OpenCommandPalette,
    ToggleCommandPalette,
    CloseCommandPalette,
    CancelAgent,
    PalettePrevious,
    PaletteNext,
    PaletteInsertChar(char),
    PaletteDeleteBackward,
    PaletteSelect,
    PermissionToggleChoice,
    PermissionSelectAllow,
    PermissionSelectDeny,
    PermissionConfirm,
    SetInput(String),
    #[allow(dead_code)]
    ExecuteSlash(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Session,
    Model,
    Agent,
    Help,
    Exit,
}

#[derive(Debug, Clone)]
pub struct SlashCommandDef {
    pub id: &'static str,
    pub name: &'static str,
    pub syntax: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    #[allow(dead_code)]
    pub category: CommandCategory,
}

impl SlashCommandDef {
    pub fn to_palette_command(&self) -> PaletteCommand {
        PaletteCommand {
            id: self.id,
            name: self.syntax,
            description: self.description,
            action: AppCommand::SetInput(if self.syntax.contains('<') {
                format!("{} ", self.name)
            } else {
                self.name.to_string()
            }),
            shortcut: None,
        }
    }
}

pub struct CommandRegistry {
    commands: Vec<SlashCommandDef>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: vec![
                SlashCommandDef {
                    id: "set",
                    name: "/set",
                    syntax: "/set <key> <value>",
                    description: "Set session/configuration variables",
                    aliases: &[],
                    category: CommandCategory::Model,
                },
                SlashCommandDef {
                    id: "show",
                    name: "/show",
                    syntax: "/show",
                    description: "Show current session/model information",
                    aliases: &[],
                    category: CommandCategory::Model,
                },
                SlashCommandDef {
                    id: "model",
                    name: "/model",
                    syntax: "/model <name>",
                    description: "Select/load a model",
                    aliases: &[],
                    category: CommandCategory::Model,
                },
                SlashCommandDef {
                    id: "models",
                    name: "/models",
                    syntax: "/models",
                    description: "List available models",
                    aliases: &[],
                    category: CommandCategory::Model,
                },
                SlashCommandDef {
                    id: "new",
                    name: "/new",
                    syntax: "/new",
                    description: "Start a new session",
                    aliases: &[],
                    category: CommandCategory::Session,
                },
                SlashCommandDef {
                    id: "resume",
                    name: "/resume",
                    syntax: "/resume <id>",
                    description: "Resume a session",
                    aliases: &[],
                    category: CommandCategory::Session,
                },
                SlashCommandDef {
                    id: "sessions",
                    name: "/sessions",
                    syntax: "/sessions",
                    description: "List saved sessions",
                    aliases: &[],
                    category: CommandCategory::Session,
                },
                SlashCommandDef {
                    id: "session",
                    name: "/session",
                    syntax: "/session <id>",
                    description: "Switch to a session",
                    aliases: &[],
                    category: CommandCategory::Session,
                },
                SlashCommandDef {
                    id: "save",
                    name: "/save",
                    syntax: "/save",
                    description: "Save current session",
                    aliases: &[],
                    category: CommandCategory::Session,
                },
                SlashCommandDef {
                    id: "clear",
                    name: "/clear",
                    syntax: "/clear",
                    description: "Clear current conversation",
                    aliases: &[],
                    category: CommandCategory::Session,
                },
                SlashCommandDef {
                    id: "cancel",
                    name: "/cancel",
                    syntax: "/cancel",
                    description: "Cancel the running task",
                    aliases: &[],
                    category: CommandCategory::Agent,
                },
                SlashCommandDef {
                    id: "tools",
                    name: "/tools",
                    syntax: "/tools",
                    description: "Show available tools",
                    aliases: &[],
                    category: CommandCategory::Agent,
                },
                SlashCommandDef {
                    id: "permissions",
                    name: "/permissions",
                    syntax: "/permissions",
                    description: "Show permission status",
                    aliases: &[],
                    category: CommandCategory::Agent,
                },
                SlashCommandDef {
                    id: "help",
                    name: "/help",
                    syntax: "/help",
                    description: "Show available commands",
                    aliases: &["/?"],
                    category: CommandCategory::Help,
                },
                SlashCommandDef {
                    id: "help_cmd",
                    name: "/help",
                    syntax: "/help <command>",
                    description: "Show command-specific help",
                    aliases: &[],
                    category: CommandCategory::Help,
                },
                SlashCommandDef {
                    id: "shortcuts",
                    name: "/shortcuts",
                    syntax: "/shortcuts",
                    description: "Show keyboard shortcuts",
                    aliases: &[],
                    category: CommandCategory::Help,
                },
                SlashCommandDef {
                    id: "bye",
                    name: "/bye",
                    syntax: "/bye",
                    description: "Exit CRUDO",
                    aliases: &[],
                    category: CommandCategory::Exit,
                },
                SlashCommandDef {
                    id: "quit",
                    name: "/quit",
                    syntax: "/quit",
                    description: "Exit CRUDO",
                    aliases: &[],
                    category: CommandCategory::Exit,
                },
            ],
        }
    }

    pub fn global() -> &'static CommandRegistry {
        static INSTANCE: OnceLock<CommandRegistry> = OnceLock::new();
        INSTANCE.get_or_init(CommandRegistry::new)
    }

    #[allow(dead_code)]
    pub fn commands(&self) -> &[SlashCommandDef] {
        &self.commands
    }

    pub fn find(&self, token: &str) -> Option<&SlashCommandDef> {
        let normalized = if token.starts_with('/') {
            token.to_lowercase()
        } else {
            format!("/{}", token.to_lowercase())
        };

        self.commands.iter().find(|cmd| {
            cmd.name.eq_ignore_ascii_case(&normalized)
                || cmd
                    .aliases
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(&normalized))
        })
    }

    pub fn format_help(&self) -> String {
        let mut out = String::from("Available Commands:\n\n");
        let max_syntax = self
            .commands
            .iter()
            .map(|c| c.syntax.len())
            .max()
            .unwrap_or(20);

        for cmd in &self.commands {
            let padding = " ".repeat(max_syntax.saturating_sub(cmd.syntax.len()) + 3);
            out.push_str(&format!("  {}{}{}\n", cmd.syntax, padding, cmd.description));
        }

        out.push_str("\nUse \"\"\" to begin a multi-line message.");
        out
    }

    pub fn format_command_help(&self, name_or_alias: &str) -> Result<String, String> {
        let clean = name_or_alias.trim();
        if clean.eq_ignore_ascii_case("shortcuts") {
            return Ok(self.format_shortcuts());
        }

        if let Some(cmd) = self.find(clean) {
            let mut res = format!("Help for {}:\n\n", cmd.name);
            res.push_str(&format!("  Syntax:      {}\n", cmd.syntax));
            res.push_str(&format!("  Description: {}\n", cmd.description));
            if !cmd.aliases.is_empty() {
                res.push_str(&format!("  Aliases:     {}\n", cmd.aliases.join(", ")));
            }
            Ok(res)
        } else {
            let display_name = if clean.starts_with('/') {
                clean.to_string()
            } else {
                format!("/{}", clean)
            };
            Err(format!(
                "Unknown command: {}\nType /help to see available commands.",
                display_name
            ))
        }
    }

    pub fn format_shortcuts(&self) -> String {
        let mut out = String::from("Keyboard Shortcuts:\n\n");
        let shortcuts = [
            ("F2", "Toggle activity and tool log panel"),
            ("Ctrl+K", "Open command palette"),
            ("Tab / Shift+Tab", "Focus next / previous panel"),
            ("PageUp / PageDn", "Scroll chat history by page"),
            ("Home / End", "Scroll chat history to top / bottom"),
            ("Up / Down", "Navigate input history (or scroll chat)"),
            ("Ctrl+U", "Clear prompt input"),
            ("Esc", "Cancel active running agent task"),
            ("Ctrl+C", "Quit CRUDO"),
        ];

        for (key, desc) in shortcuts {
            let padding = " ".repeat(18usize.saturating_sub(key.len()) + 2);
            out.push_str(&format!("  {}{}{}\n", key, padding, desc));
        }
        out
    }

    pub fn to_palette_commands(&self) -> Vec<PaletteCommand> {
        let mut list = Vec::new();
        for cmd in &self.commands {
            // Avoid duplicate entries for help command variant in palette
            if cmd.id == "help_cmd" {
                continue;
            }
            list.push(cmd.to_palette_command());
        }
        list
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns true if the input text should be interpreted as a slash command.
///
/// Multiline inputs beginning with `"""` are preserved as normal messages.
pub fn is_slash_command(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.starts_with("\"\"\"") {
        return false;
    }
    trimmed.starts_with('/')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSlashCommand<'a> {
    pub raw: &'a str,
    pub command: &'a str,
    pub args: Vec<&'a str>,
}

pub fn parse_slash_command(input: &str) -> Option<ParsedSlashCommand<'_>> {
    let trimmed = input.trim();
    if !is_slash_command(trimmed) {
        return None;
    }

    let mut tokens = trimmed.split_whitespace();
    let command = tokens.next()?;
    let args: Vec<&str> = tokens.collect();

    Some(ParsedSlashCommand {
        raw: trimmed,
        command,
        args,
    })
}

pub struct CommandContext<'a> {
    pub app: &'a mut crate::app::App,
    pub session_coordinator: &'a mut crate::session::SessionCoordinator,
    pub agent_controller: &'a mut crate::agent::controller::AgentController,
}

pub fn execute_slash_command(input: &str, ctx: &mut CommandContext) {
    let parsed = match parse_slash_command(input) {
        Some(p) => p,
        None => return,
    };

    let cmd_name = parsed.command.to_lowercase();
    let registry = CommandRegistry::global();

    // Handle /? alias and /? shortcuts
    if cmd_name == "/?" {
        if parsed.args.first().copied() == Some("shortcuts") {
            ctx.app
                .add_message(Role::System, registry.format_shortcuts());
        } else if let Some(arg) = parsed.args.first() {
            match registry.format_command_help(arg) {
                Ok(help_text) => ctx.app.add_message(Role::System, help_text),
                Err(err_text) => ctx.app.add_message(Role::System, err_text),
            }
        } else {
            ctx.app.add_message(Role::System, registry.format_help());
        }
        return;
    }

    let cmd_def = match registry.find(&cmd_name) {
        Some(def) => def,
        None => {
            ctx.app.add_message(
                Role::System,
                format!(
                    "Unknown command: {}\nType /help to see available commands.",
                    parsed.command
                ),
            );
            return;
        }
    };

    match cmd_def.id {
        "new" => {
            let new_session = ctx.session_coordinator.create_fresh_session();
            let session_id = new_session.session_id.clone();
            ctx.agent_controller.set_session(new_session.clone());
            ctx.app.hydrate_from_session(new_session);
            ctx.app
                .add_message(Role::System, format!("Started new session: {}", session_id));
        }
        "resume" | "session" => {
            if parsed.args.is_empty() {
                ctx.app
                    .add_message(Role::System, format!("Usage: {} <id>", cmd_def.name));
                return;
            }
            let target_id = parsed.args[0];
            match ctx.session_coordinator.load_session(target_id) {
                Ok(loaded) => {
                    let active_id = loaded.session_id.clone();
                    ctx.agent_controller.set_session(loaded.clone());
                    ctx.app.hydrate_from_session(loaded);
                    ctx.app
                        .add_message(Role::System, format!("Switched to session: {}", active_id));
                }
                Err(e) => {
                    ctx.app.add_message(
                        Role::System,
                        format!("Failed to load session '{}': {}", target_id, e),
                    );
                }
            }
        }
        "sessions" => match ctx.session_coordinator.store().list_sessions() {
            Ok(sessions) => {
                if sessions.is_empty() {
                    ctx.app
                        .add_message(Role::System, "No saved sessions found.".to_string());
                } else {
                    let active_id = ctx.session_coordinator.active_session_id();
                    let mut out = String::from("Saved Sessions:\n\n");
                    for s in &sessions {
                        let marker = if s.id == active_id { "* " } else { "  " };
                        let active_tag = if s.id == active_id { " (active)" } else { "" };
                        out.push_str(&format!(
                            "{}{} - {} messages{}\n",
                            marker, s.id, s.message_count, active_tag
                        ));
                    }
                    out.push_str("\nUse /resume <id> or /session <id> to switch sessions.");
                    ctx.app.add_message(Role::System, out);
                }
            }
            Err(e) => {
                ctx.app
                    .add_message(Role::System, format!("Failed to list sessions: {}", e));
            }
        },
        "save" => {
            if let Some(path) = ctx.session_coordinator.persistence_path() {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match ctx.session_coordinator.active_session().save_to_path(path) {
                    Ok(_) => {
                        ctx.app.add_message(
                            Role::System,
                            format!("Session saved to: {}", path.display()),
                        );
                    }
                    Err(e) => {
                        ctx.app
                            .add_message(Role::System, format!("Failed to save session: {}", e));
                    }
                }
            } else {
                ctx.app.add_message(
                    Role::System,
                    "No persistence path configured for active session.".to_string(),
                );
            }
        }
        "clear" => {
            ctx.app.messages.clear();
            ctx.app.chat_scroll.set_max_offset(0);
            ctx.app.chat_scroll.auto_scroll = true;
            ctx.app.chat_scroll.unseen_items = 0;
        }
        "cancel" => {
            ctx.app.cancel_active_run();
            ctx.agent_controller.cancel();
            ctx.app
                .add_message(Role::System, "Agent execution cancelled.".to_string());
        }
        "set" => {
            if parsed.args.len() < 2 {
                ctx.app
                    .add_message(Role::System, "Usage: /set <key> <value>".to_string());
                return;
            }
            let key = parsed.args[0].to_lowercase();
            let value = parsed.args[1..].join(" ");
            match key.as_str() {
                "model" => {
                    let resolved = api::resolve_model_alias(&value);
                    ctx.agent_controller.set_model(resolved.clone());
                    ctx.app
                        .add_message(Role::System, format!("Model set to: {}", resolved));
                }
                "permission" | "permissions" => {
                    let mode = match value.to_lowercase().as_str() {
                        "ask" | "prompt" => runtime::PermissionMode::Prompt,
                        "dontask" | "allow" | "full" => runtime::PermissionMode::Allow,
                        "readonly" | "read-only" => runtime::PermissionMode::ReadOnly,
                        "write" | "workspacewrite" => runtime::PermissionMode::WorkspaceWrite,
                        "danger" | "dangerfullaccess" => runtime::PermissionMode::DangerFullAccess,
                        _ => {
                            ctx.app.add_message(
                                Role::System,
                                format!(
                                    "Unknown permission mode '{}'. Valid: prompt, allow, readonly, write, danger",
                                    value
                                ),
                            );
                            return;
                        }
                    };
                    ctx.agent_controller.set_permission_mode(mode);

                    ctx.app
                        .add_message(Role::System, format!("Permission mode set to: {:?}", mode));
                }
                _ => {
                    ctx.app.add_message(
                        Role::System,
                        format!("Configuration set: {} = {}", key, value),
                    );
                }
            }
        }
        "show" => {
            let session = ctx.session_coordinator.active_session();
            let model = ctx
                .agent_controller
                .model()
                .unwrap_or_else(crate::agent::real::resolve_configured_model);
            let perm = ctx
                .agent_controller
                .permission_mode()
                .map(|m| format!("{:?}", m))
                .unwrap_or_else(|| "Ask".to_string());
            let path_str = ctx
                .session_coordinator
                .persistence_path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "None".to_string());
            let ws_str = session
                .workspace_root()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "None".to_string());

            let info = format!(
                "Session & Model Information:\n\n  Session ID:       {}\n  Persistence Path: {}\n  Workspace Root:   {}\n  Messages:         {}\n  Model:            {}\n  Permission Mode:  {}",
                session.session_id, path_str, ws_str, session.messages.len(), model, perm
            );
            ctx.app.add_message(Role::System, info);
        }
        "model" => {
            if parsed.args.is_empty() {
                ctx.app
                    .add_message(Role::System, "Usage: /model <name>".to_string());
                return;
            }
            let resolved = api::resolve_model_alias(parsed.args[0]);
            ctx.agent_controller.set_model(resolved.clone());
            ctx.app
                .add_message(Role::System, format!("Model set to: {}", resolved));
        }
        "models" => {
            let out = "Available Models:\n\n  claude-sonnet-4-6   (aliases: sonnet)\n  claude-opus-4-7     (aliases: opus)\n  claude-haiku-4-5    (aliases: haiku)\n  grok-3              (aliases: grok)\n  grok-3-mini         (aliases: grok-mini)\n  grok-2\n  kimi-k2.5           (aliases: kimi)";
            ctx.app.add_message(Role::System, out.to_string());
        }
        "tools" => {
            let out = "Available Tools:\n\n  read_file    Read contents of a file in the workspace\n  write_file   Write contents to a file in the workspace\n  edit_file    Apply targeted text replacements to a file\n  bash         Execute shell commands in the workspace";
            ctx.app.add_message(Role::System, out.to_string());
        }
        "permissions" => {
            let perm = ctx
                .agent_controller
                .permission_mode()
                .map(|m| format!("{:?}", m))
                .unwrap_or_else(|| "Ask".to_string());
            ctx.app
                .add_message(Role::System, format!("Current Permission Mode: {}", perm));
        }
        "help" | "help_cmd" => {
            if parsed.args.is_empty() {
                ctx.app.add_message(Role::System, registry.format_help());
            } else if parsed.args[0].eq_ignore_ascii_case("shortcuts") {
                ctx.app
                    .add_message(Role::System, registry.format_shortcuts());
            } else {
                match registry.format_command_help(parsed.args[0]) {
                    Ok(help_text) => ctx.app.add_message(Role::System, help_text),
                    Err(err_text) => ctx.app.add_message(Role::System, err_text),
                }
            }
        }
        "shortcuts" => {
            ctx.app
                .add_message(Role::System, registry.format_shortcuts());
        }
        "bye" | "quit" => {
            ctx.app.quit();
            ctx.app
                .add_message(Role::System, "Exiting CRUDO...".to_string());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_slash_command_basic() {
        assert!(is_slash_command("/help"));
        assert!(is_slash_command("  /sessions "));
        assert!(is_slash_command("/set key value"));
        assert!(!is_slash_command("hello world"));
        assert!(!is_slash_command(""));
    }

    #[test]
    fn test_is_slash_command_multiline_ignored() {
        assert!(!is_slash_command("\"\"\"\n/help\n\"\"\""));
        assert!(!is_slash_command("\"\"\"/something\"\"\""));
    }

    #[test]
    fn test_parse_slash_command() {
        let p = parse_slash_command("/set model claude-sonnet-4-6").unwrap();
        assert_eq!(p.command, "/set");
        assert_eq!(p.args, vec!["model", "claude-sonnet-4-6"]);

        let p2 = parse_slash_command("/help").unwrap();
        assert_eq!(p2.command, "/help");
        assert!(p2.args.is_empty());
    }

    #[test]
    fn test_help_generated_from_registry() {
        let registry = CommandRegistry::global();
        let help = registry.format_help();
        assert!(help.contains("/set <key> <value>"));
        assert!(help.contains("/show"));
        assert!(help.contains("/model <name>"));
        assert!(help.contains("/models"));
        assert!(help.contains("/new"));
        assert!(help.contains("/resume <id>"));
        assert!(help.contains("/sessions"));
        assert!(help.contains("/session <id>"));
        assert!(help.contains("/save"));
        assert!(help.contains("/clear"));
        assert!(help.contains("/cancel"));
        assert!(help.contains("/tools"));
        assert!(help.contains("/permissions"));
        assert!(help.contains("/help"));
        assert!(help.contains("/shortcuts"));
        assert!(help.contains("/bye"));
        assert!(help.contains("/quit"));
        assert!(help.contains("Use \"\"\" to begin a multi-line message."));
    }

    #[test]
    fn test_format_command_help() {
        let registry = CommandRegistry::global();
        let resume_help = registry.format_command_help("resume").unwrap();
        assert!(resume_help.contains("/resume <id>"));
        assert!(resume_help.contains("Resume a session"));

        let unknown = registry.format_command_help("nonexistent");
        assert!(unknown.is_err());
        assert!(unknown
            .unwrap_err()
            .contains("Unknown command: /nonexistent"));
    }

    fn create_test_context(
        prefix: &str,
    ) -> (
        crate::app::App,
        crate::session::SessionCoordinator,
        crate::agent::controller::AgentController,
        std::path::PathBuf,
    ) {
        let temp_dir = std::env::temp_dir().join(format!("{}_{}", prefix, std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let coordinator = crate::session::SessionCoordinator::from_cwd(&temp_dir).unwrap();
        let adapter = crate::agent::real::RealAgentAdapter::new()
            .with_session(coordinator.active_session().clone());
        let controller = crate::agent::controller::AgentController::with_agent(adapter);
        let app = crate::app::App::new();

        (app, coordinator, controller, temp_dir)
    }

    #[test]
    fn test_execute_slash_help() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_help");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/help", &mut ctx);
        assert_eq!(ctx.app.messages.len(), 1);
        assert_eq!(ctx.app.messages[0].role, Role::System);
        assert!(ctx.app.messages[0].content.contains("Available Commands:"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_question_mark_alias() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_qm");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/?", &mut ctx);
        assert_eq!(ctx.app.messages.len(), 1);
        assert_eq!(ctx.app.messages[0].role, Role::System);
        assert!(ctx.app.messages[0].content.contains("Available Commands:"));

        execute_slash_command("/? shortcuts", &mut ctx);
        assert_eq!(ctx.app.messages.len(), 2);
        assert!(ctx.app.messages[1].content.contains("Keyboard Shortcuts:"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_unknown_command() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_unknown");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/something", &mut ctx);
        assert_eq!(ctx.app.messages.len(), 1);
        assert_eq!(ctx.app.messages[0].role, Role::System);
        assert_eq!(
            ctx.app.messages[0].content,
            "Unknown command: /something\nType /help to see available commands."
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_missing_args() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_missing");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/resume", &mut ctx);
        assert_eq!(ctx.app.messages[0].content, "Usage: /resume <id>");

        execute_slash_command("/set", &mut ctx);
        assert_eq!(ctx.app.messages[1].content, "Usage: /set <key> <value>");

        execute_slash_command("/model", &mut ctx);
        assert_eq!(ctx.app.messages[2].content, "Usage: /model <name>");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_new_switches_session() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_new");
        let first_id = coord.active_session_id().to_string();

        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/new", &mut ctx);

        let second_id = ctx.session_coordinator.active_session_id().to_string();
        assert_ne!(first_id, second_id);
        assert_eq!(
            ctx.agent_controller.get_session().unwrap().session_id,
            second_id
        );
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("Started new session:"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_resume_and_session() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_resume");

        // 1. Create and save an initial session with conversation
        let initial_id;
        {
            coord
                .active_session_mut()
                .push_user_text("Saved query before resume")
                .unwrap();
            coord
                .active_session_mut()
                .push_message(runtime::ConversationMessage::assistant(vec![
                    runtime::ContentBlock::Text {
                        text: "Saved answer before resume".to_string(),
                    },
                ]))
                .unwrap();
            initial_id = coord.active_session_id().to_string();
            let path = coord.persistence_path().unwrap();
            coord.active_session().save_to_path(path).unwrap();
        }

        // 2. Start a fresh session
        coord.create_fresh_session();
        assert_ne!(coord.active_session_id(), initial_id);

        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        // 3. Resume the previous session
        execute_slash_command(&format!("/resume {}", initial_id), &mut ctx);

        assert_eq!(ctx.session_coordinator.active_session_id(), initial_id);
        assert_eq!(
            ctx.agent_controller.get_session().unwrap().session_id,
            initial_id
        );
        // Hydrated messages: 2 from session + 1 system confirmation
        assert_eq!(ctx.app.messages.len(), 3);
        assert_eq!(ctx.app.messages[0].content, "Saved query before resume");
        assert_eq!(ctx.app.messages[1].content, "Saved answer before resume");
        assert!(ctx.app.messages[2].content.contains("Switched to session:"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_sessions_list() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_sessions");
        let active_id = coord.active_session_id().to_string();
        let path = coord.persistence_path().unwrap();
        coord.active_session().save_to_path(path).unwrap();

        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/sessions", &mut ctx);
        let out = &ctx.app.messages.last().unwrap().content;
        assert!(out.contains("Saved Sessions:"));
        assert!(out.contains(&active_id));
        assert!(out.contains("(active)"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_save() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_save");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/save", &mut ctx);
        let last_msg = ctx.app.messages.last().unwrap();
        assert!(last_msg.content.contains("Session saved to:"));
        assert!(ctx.session_coordinator.persistence_path().unwrap().exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_clear() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_clear");
        app.add_message(Role::User, "Hello".to_string());
        app.add_message(Role::Assistant, "Hi".to_string());
        assert_eq!(app.messages.len(), 2);

        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/clear", &mut ctx);
        assert!(ctx.app.messages.is_empty());
        assert_eq!(ctx.app.chat_scroll.offset, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_model_and_set() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_model");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/model grok", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("Model set to: grok-3"));
        assert_eq!(ctx.agent_controller.model(), Some("grok-3".to_string()));

        execute_slash_command("/set model opus", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("Model set to: claude-opus-4-7"));
        assert_eq!(
            ctx.agent_controller.model(),
            Some("claude-opus-4-7".to_string())
        );

        execute_slash_command("/models", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("claude-sonnet-4-6"));
        assert!(ctx.app.messages.last().unwrap().content.contains("grok-3"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_show_and_tools_permissions() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_info");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/show", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("Session & Model Information:"));

        execute_slash_command("/tools", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("read_file"));

        execute_slash_command("/permissions", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("Current Permission Mode:"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_slash_cancel_and_exit() {
        let (mut app, mut coord, mut ctrl, temp_dir) = create_test_context("slash_lifecycle");
        let mut ctx = CommandContext {
            app: &mut app,
            session_coordinator: &mut coord,
            agent_controller: &mut ctrl,
        };

        execute_slash_command("/cancel", &mut ctx);
        assert!(ctx
            .app
            .messages
            .last()
            .unwrap()
            .content
            .contains("cancelled"));

        execute_slash_command("/quit", &mut ctx);
        assert!(ctx.app.should_quit);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_palette_exposes_slash_commands() {
        let commands = crate::palette::default_commands();
        let mut state = crate::palette::CommandPaletteState::new();
        state.query = "/".to_string();

        let matches = state.filtered_commands(&commands);
        assert!(!matches.is_empty());
        assert!(matches.iter().all(|c| c.name.starts_with('/')));
        assert!(matches.iter().any(|c| c.id == "new"));
        assert!(matches.iter().any(|c| c.id == "resume"));
        assert!(matches.iter().any(|c| c.id == "sessions"));
        assert!(matches.iter().any(|c| c.id == "help"));
    }
}
