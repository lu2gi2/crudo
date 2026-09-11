use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Help,
    Clear,
    Quit,
    Attach(PathBuf),
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDefinition {
    pub name: &'static str,
    pub syntax: &'static str,
    pub description: &'static str,
}

pub static AVAILABLE_COMMANDS: &[CommandDefinition] = &[
    CommandDefinition {
        name: "help",
        syntax: "/help",
        description: "Show available commands and keyboard shortcuts",
    },
    CommandDefinition {
        name: "attach",
        syntax: "/attach <path>",
        description: "Attach a local file (PDF, image, drawing, document) for processing",
    },
    CommandDefinition {
        name: "clear",
        syntax: "/clear",
        description: "Clear the current conversation history",
    },
    CommandDefinition {
        name: "status",
        syntax: "/status",
        description: "Display local system, backend, model, and subsystem status",
    },
    CommandDefinition {
        name: "quit",
        syntax: "/quit",
        description: "Exit the CRUDO interface",
    },
];

impl SlashCommand {
    /// Attempts to parse a slash command from the user's raw input.
    /// Returns:
    /// - `None` if the input is not a slash command (i.e. regular prompt)
    /// - `Some(Ok(cmd))` if successfully parsed
    /// - `Some(Err(msg))` if input starts with `/` but is unknown or invalid syntax
    pub fn parse(input: &str) -> Option<Result<SlashCommand, String>> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let mut parts = trimmed.split_whitespace();
        let cmd = parts.next().unwrap_or("/");

        match cmd {
            "/help" => Some(Ok(SlashCommand::Help)),
            "/clear" => Some(Ok(SlashCommand::Clear)),
            "/quit" | "/exit" => Some(Ok(SlashCommand::Quit)),
            "/status" => Some(Ok(SlashCommand::Status)),
            "/attach" => {
                let rest: Vec<&str> = parts.collect();
                if rest.is_empty() {
                    Some(Err("Usage: /attach <path-to-file>".to_string()))
                } else {
                    let path_str = rest.join(" ");
                    // Strip surrounding quotes if provided
                    let cleaned = path_str.trim_matches('"').trim_matches('\'');
                    Some(Ok(SlashCommand::Attach(PathBuf::from(cleaned))))
                }
            }
            _ => Some(Err(format!(
                "Unknown command: '{cmd}'. Type /help for available commands."
            ))),
        }
    }

    /// Generates the standard formatted help text for CRUDO.
    pub fn help_text() -> String {
        let mut text = String::new();
        text.push_str("/help       — Show available commands\n");
        text.push_str("/clear      — Clear the conversation\n");
        text.push_str("/attach     — Attach a document or image\n");
        text.push_str("/status     — Display system and subsystem status\n");
        text.push_str("/quit       — Exit CRUDO\n\n");
        text.push_str("SHORTCUTS\n\n");
        text.push_str("↑ / ↓       — Scroll chat\n");
        text.push_str("PgUp/PgDn   — Scroll by viewport\n");
        text.push_str("Home        — Go to beginning\n");
        text.push_str("End         — Go to latest message\n");
        text.push_str("Enter       — Send message\n");
        text.push_str("Esc         — Cancel current action\n");
        text.push_str("Ctrl+F      — Attach a file\n");
        text.push_str("Ctrl+C      — Exit CRUDO\n");

        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_command_parsing() {
        assert_eq!(SlashCommand::parse("hello world"), None);
        assert_eq!(SlashCommand::parse("/help"), Some(Ok(SlashCommand::Help)));
        assert_eq!(SlashCommand::parse("/clear"), Some(Ok(SlashCommand::Clear)));
        assert_eq!(SlashCommand::parse("/quit"), Some(Ok(SlashCommand::Quit)));
        assert_eq!(SlashCommand::parse("/exit"), Some(Ok(SlashCommand::Quit)));
        assert_eq!(
            SlashCommand::parse("/status"),
            Some(Ok(SlashCommand::Status))
        );
        assert_eq!(
            SlashCommand::parse("/attach report.pdf"),
            Some(Ok(SlashCommand::Attach(PathBuf::from("report.pdf"))))
        );
        assert_eq!(
            SlashCommand::parse("/attach \"my report.pdf\""),
            Some(Ok(SlashCommand::Attach(PathBuf::from("my report.pdf"))))
        );
    }

    #[test]
    fn test_unknown_slash_command() {
        let res = SlashCommand::parse("/unknown");
        assert!(res.is_some());
        assert!(res.unwrap().is_err());
    }

    #[test]
    fn test_attach_missing_arg() {
        let res = SlashCommand::parse("/attach");
        assert!(res.is_some());
        assert!(res.unwrap().is_err());
    }

    #[test]
    fn test_help_text_content() {
        let help = SlashCommand::help_text();
        println!("\n{help}");
        assert!(help.contains("/help"));
        assert!(help.contains("/clear"));
        assert!(help.contains("/quit"));
        assert!(help.contains("/attach"));
        assert!(help.contains("/status"));
        assert!(help.contains("SHORTCUTS"));
        assert!(help.contains("—"));
    }
}
