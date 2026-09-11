mod activity;
mod agent;
mod app;
mod command;
mod events;
mod history;
mod input;
mod markdown;
mod message;
mod palette;
mod scroll;
mod session;
mod shortcuts;
mod spinner;
mod terminal;
mod theme;
mod ui;

use agent::controller::AgentController;
use anyhow::Result;
use app::{App, UserEvent};
use command::AppCommand;
use events::{Event, EventHandler};
use shortcuts::{map_key_to_command, KeyContext};

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;

/// Resolves the destination path for runtime and agent logs using the provided environment lookup.
fn resolve_log_path_from(
    env_lookup: impl Fn(&str) -> Result<String, std::env::VarError>,
) -> PathBuf {
    if let Ok(path) = env_lookup("TERMINA_LOG_FILE").or_else(|_| env_lookup("CRUDO_LOG_FILE")) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Ok(home) = env_lookup("USERPROFILE").or_else(|_| env_lookup("HOME")) {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(".crudo").join("termina.log");
        }
    }

    std::env::temp_dir().join("termina.log")
}

/// Resolves the destination path for runtime and agent logs.
fn resolve_log_path() -> PathBuf {
    resolve_log_path_from(|k| std::env::var(k))
}

/// Initializes tracing to write to a log file instead of stdout/stderr,
/// preventing terminal corruption in full-screen Ratatui mode.
fn init_logging() -> Option<PathBuf> {
    let log_path = resolve_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        if tracing_subscriber::fmt()
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .try_init()
            .is_ok()
        {
            return Some(log_path);
        }
    }

    // If opening the file failed, route all logs to a sink so stdout/stderr
    // are never polluted while Ratatui owns the terminal.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::sink)
        .with_ansi(false)
        .try_init();

    None
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_file = init_logging();

    let mut session_coordinator = session::SessionCoordinator::startup()?;

    let mut term = terminal::init_terminal()?;
    let mut app = App::new();
    app.hydrate_from_session(session_coordinator.active_session());
    let mut events = EventHandler::new(250);
    let use_mock = std::env::var("TERMINA_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut agent_controller = if use_mock {
        AgentController::new()
    } else {
        let adapter = crate::agent::real::RealAgentAdapter::new()
            .with_session(session_coordinator.active_session().clone());
        AgentController::with_agent(adapter)
    };

    let res = run_app(
        &mut term,
        &mut app,
        &mut events,
        &mut agent_controller,
        &mut session_coordinator,
    )
    .await;

    terminal::restore_terminal()?;

    if let Err(e) = res {
        eprintln!("Error: {:?}", e);
    }

    Ok(())
}

async fn run_app(
    term: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    events: &mut EventHandler,
    agent_controller: &mut AgentController,
    session_coordinator: &mut session::SessionCoordinator,
) -> Result<()> {
    let (agent_tx, mut agent_rx) =
        tokio::sync::mpsc::channel::<(usize, agent::events::AgentEvent)>(32);

    loop {
        term.draw(|f| ui::draw(f, app))?;

        tokio::select! {
            Some(event) = events.next() => {
                match event {
                    Event::Key(key) => {
                        let ctx = KeyContext::from_app(app);
                        if let Some(cmd) = map_key_to_command(key, &ctx) {
                            app.handle_command(cmd);
                        }
                    }
                    Event::Paste(text) => {
                        app.handle_command(AppCommand::Paste(text));
                    }
                    Event::Resize(_, _) => {
                        // Clear all terminal cells so ratatui's diff-based renderer
                        // does not leave stale characters from the previous frame
                        // visible after the terminal is resized.
                        term.clear()?;
                    }
                    Event::Tick => {
                        app.tick();
                    }
                }
            }
            Some((run_id, agent_event)) = agent_rx.recv() => {
                // Discard stale events from previously cancelled or completed runs
                if agent_controller.active_run_id() == Some(run_id) {
                    if matches!(
                        agent_event,
                        agent::events::AgentEvent::Completed
                            | agent::events::AgentEvent::Error(_)
                            | agent::events::AgentEvent::Cancelled
                    ) {
                        agent_controller.mark_completed();
                    }
                    app.handle_run_event(run_id, agent_event);
                } else if agent_controller.cancelled_run_id() == Some(run_id)
                    && matches!(agent_event, agent::events::AgentEvent::Cancelled)
                {
                    agent_controller.consume_cancelled();
                    app.handle_run_event(run_id, agent_event);
                }
            }
        }

        let pending_events: Vec<_> = app.events.drain(..).collect();
        for event in pending_events {
            match event {
                UserEvent::SendMessage(text) => {
                    app.add_message(message::Role::User, text.clone());
                    let run_id = agent_controller.submit_prompt(text, agent_tx.clone());
                    app.set_active_run_id(Some(run_id));
                }
                UserEvent::ExecuteSlash(cmd) => {
                    let mut ctx = command::CommandContext {
                        app,
                        session_coordinator,
                        agent_controller,
                    };
                    command::execute_slash_command(&cmd, &mut ctx);
                }
                UserEvent::CancelAgent => {
                    agent_controller.cancel();
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_log_path_default() {
        let path = resolve_log_path_from(|var| match var {
            "USERPROFILE" => Ok("C:\\Users\\test".to_string()),
            _ => Err(std::env::VarError::NotPresent),
        });
        assert_eq!(path, PathBuf::from("C:\\Users\\test\\.crudo\\termina.log"));
    }

    #[test]
    fn test_resolve_log_path_custom_env() {
        let path = resolve_log_path_from(|var| match var {
            "TERMINA_LOG_FILE" => Ok("D:\\custom\\my.log".to_string()),
            _ => Err(std::env::VarError::NotPresent),
        });
        assert_eq!(path, PathBuf::from("D:\\custom\\my.log"));
    }

    #[test]
    fn test_resolve_log_path_crudo_env() {
        let path = resolve_log_path_from(|var| match var {
            "CRUDO_LOG_FILE" => Ok("D:\\crudo\\agent.log".to_string()),
            _ => Err(std::env::VarError::NotPresent),
        });
        assert_eq!(path, PathBuf::from("D:\\crudo\\agent.log"));
    }

    #[test]
    fn test_resolve_log_path_fallback_temp() {
        let path = resolve_log_path_from(|_| Err(std::env::VarError::NotPresent));
        assert_eq!(path, std::env::temp_dir().join("termina.log"));
    }

    #[test]
    fn test_init_logging_creates_file_if_missing() {
        let test_log = std::env::temp_dir().join("test_termina_create.log");
        let _ = std::fs::remove_file(&test_log);
        if let Some(parent) = test_log.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&test_log)
            .expect("create file");
        drop(file);
        assert!(test_log.exists());
        let _ = std::fs::remove_file(&test_log);
    }

    #[test]
    fn test_tracing_event_writes_to_file() {
        let temp_log = std::env::temp_dir().join("termina_test_trace.log");
        let _ = std::fs::remove_file(&temp_log);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&temp_log)
            .expect("create file");

        let subscriber = tracing_subscriber::fmt()
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("aspect_std::logging: [ENTRY] execute_tool_with_enforcer");
            tracing::info!("aspect_std::logging: [EXIT] execute_tool_with_enforcer");
        });

        let content = std::fs::read_to_string(&temp_log).expect("read log");
        assert!(content.contains("aspect_std::logging: [ENTRY] execute_tool_with_enforcer"));
        assert!(content.contains("aspect_std::logging: [EXIT] execute_tool_with_enforcer"));
        let _ = std::fs::remove_file(&temp_log);
    }
}
