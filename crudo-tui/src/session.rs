use crate::message::{Message, Role};
use anyhow::{Context, Result};
use runtime::session_control::{SessionHandle, SessionStore};
use runtime::{ContentBlock, MessageRole, Session};
use std::path::Path;

/// Converts persisted [`Session`] conversation messages into TUI [`Message`] representation.
///
/// Filters for user and assistant text content that can be safely rendered in the
/// conversation view, ignoring internal runtime blocks (thinking, tool use, tool results,
/// system directives). Preserves chronological ordering.
pub fn session_to_tui_messages(session: &Session) -> Vec<Message> {
    let mut messages = Vec::new();
    let mut next_id = 1;

    for msg in &session.messages {
        let role = match msg.role {
            MessageRole::User => Role::User,
            MessageRole::Assistant => Role::Assistant,
            _ => continue,
        };

        let mut texts = Vec::new();
        for block in &msg.blocks {
            if let ContentBlock::Text { text } = block {
                if !text.trim().is_empty() {
                    texts.push(text.as_str());
                }
            }
        }

        if texts.is_empty() {
            continue;
        }

        let content = if texts.len() == 1 {
            texts[0].to_string()
        } else {
            texts.join("\n\n")
        };

        messages.push(Message::completed(next_id, role, content));
        next_id += 1;
    }

    messages
}

/// Coordinates session lifecycle and persistence for the CRUDO TUI.
///
/// Bridges the TUI application state with the runtime's underlying
/// [`SessionStore`] and canonical [`Session`] representation without
/// depending on Ratatui rendering primitives.
#[allow(dead_code)]
#[derive(Debug)]
pub struct SessionCoordinator {
    store: SessionStore,
    active_session: Session,
    active_handle: Option<SessionHandle>,
}

#[allow(dead_code)]
impl SessionCoordinator {
    /// Creates a new `SessionCoordinator` anchored at the process's current working directory.
    pub fn new() -> Result<Self> {
        let cwd = std::env::current_dir().context("Failed to get current working directory")?;
        Self::from_cwd(cwd)
    }

    /// Creates a new `SessionCoordinator` anchored at the provided working directory.
    ///
    /// Initializes a workspace-fingerprinted [`SessionStore`] and prepares an initial
    /// fresh [`Session`] configured with a persistence path and workspace root.
    pub fn from_cwd(cwd: impl AsRef<Path>) -> Result<Self> {
        let store = SessionStore::from_cwd(cwd.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to initialize SessionStore: {e}"))?;
        let (session, handle) = Self::build_fresh_session(&store);
        Ok(Self {
            store,
            active_session: session,
            active_handle: Some(handle),
        })
    }

    /// Creates and initializes a `SessionCoordinator` for application startup
    /// anchored at the process's current working directory.
    ///
    /// - If a persisted session exists in the workspace, loads the latest persisted session.
    /// - If no persisted session exists, creates a fresh persistent session.
    pub fn startup() -> Result<Self> {
        let cwd = std::env::current_dir().context("Failed to get current working directory")?;
        Self::startup_from_cwd(cwd)
    }

    /// Creates and initializes a `SessionCoordinator` for application startup
    /// anchored at the specified working directory.
    pub fn startup_from_cwd(cwd: impl AsRef<Path>) -> Result<Self> {
        let mut coordinator = Self::from_cwd(cwd)?;
        coordinator.init_startup_session()?;
        Ok(coordinator)
    }

    /// Determines whether persisted sessions exist and initializes the active session:
    /// - If persisted sessions exist, loads the latest persisted session.
    /// - If no persisted session exists, initializes a fresh persistent session.
    pub fn init_startup_session(&mut self) -> Result<&Session> {
        let sessions = self
            .store
            .list_sessions()
            .map_err(|e| anyhow::anyhow!("Failed to query sessions from SessionStore: {e}"))?;

        if let Some(candidate) = sessions
            .iter()
            .find(|s| s.message_count > 0)
            .or_else(|| sessions.first())
        {
            self.load_session(&candidate.id)?;
        } else {
            self.create_fresh_session();
        }

        Ok(&self.active_session)
    }

    /// Helper to construct a fresh `Session` properly bound to the store's workspace
    /// root and persistence path.
    fn build_fresh_session(store: &SessionStore) -> (Session, SessionHandle) {
        let session = Session::new().with_workspace_root(store.workspace_root());
        let handle = store.create_handle(&session.session_id);
        let session = session.with_persistence_path(handle.path.clone());
        (session, handle)
    }

    /// Creates and activates a new fresh session with unique ID, persistence path,
    /// and the workspace root. Returns a reference to the newly created session.
    pub fn create_fresh_session(&mut self) -> &Session {
        let (session, handle) = Self::build_fresh_session(&self.store);
        self.active_session = session;
        self.active_handle = Some(handle);
        &self.active_session
    }

    /// Loads a specific persisted session by reference (session ID, alias, or file path).
    ///
    /// The loaded session replaces the currently active session.
    pub fn load_session(&mut self, reference: &str) -> Result<&Session> {
        let loaded = self
            .store
            .load_session(reference)
            .map_err(|e| anyhow::anyhow!("Failed to load session '{reference}': {e}"))?;
        let mut session = loaded.session;
        if session.persistence_path().is_none() {
            session = session.with_persistence_path(loaded.handle.path.clone());
        }
        self.active_handle = Some(loaded.handle);
        self.active_session = session;
        Ok(&self.active_session)
    }

    /// Returns a reference to the active [`Session`].
    pub fn active_session(&self) -> &Session {
        &self.active_session
    }

    /// Returns a mutable reference to the active [`Session`].
    pub fn active_session_mut(&mut self) -> &mut Session {
        &mut self.active_session
    }

    /// Replaces the active session with the provided [`Session`].
    ///
    /// Automatically ensures persistence path and workspace root are bound if missing.
    pub fn set_active_session(&mut self, mut session: Session) {
        let handle = self.store.create_handle(&session.session_id);
        if session.persistence_path().is_none() {
            session = session.with_persistence_path(handle.path.clone());
        }
        if session.workspace_root().is_none() {
            session = session.with_workspace_root(self.store.workspace_root());
        }
        self.active_handle = Some(handle);
        self.active_session = session;
    }

    /// Returns a reference to the active session's [`SessionHandle`], if set.
    pub fn active_handle(&self) -> Option<&SessionHandle> {
        self.active_handle.as_ref()
    }

    /// Returns the session ID of the currently active session.
    pub fn active_session_id(&self) -> &str {
        &self.active_session.session_id
    }

    /// Returns the active session persistence path, if configured.
    pub fn persistence_path(&self) -> Option<&Path> {
        self.active_session.persistence_path()
    }

    /// Returns a reference to the underlying [`SessionStore`].
    pub fn store(&self) -> &SessionStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("{}_{}", prefix, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test directory");
        dir
    }

    #[test]
    fn test_create_fresh_coordinator() {
        let temp_dir = create_test_dir("crudo_coord_fresh");
        let coordinator = SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");

        assert!(!coordinator.active_session_id().is_empty());
        assert!(coordinator.active_handle().is_some());
        assert_eq!(
            coordinator.active_handle().unwrap().id,
            coordinator.active_session_id()
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_fresh_session_has_persistence_path() {
        let temp_dir = create_test_dir("crudo_coord_persist_path");
        let coordinator = SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");

        let persistence_path = coordinator.persistence_path();
        assert!(
            persistence_path.is_some(),
            "Fresh session must have persistence path"
        );

        let path = persistence_path.unwrap();
        let path_str = path.to_string_lossy();
        assert!(
            path_str.ends_with(".jsonl"),
            "Persistence path must use jsonl extension"
        );
        assert!(
            path_str.contains(coordinator.active_session_id()),
            "Persistence path must contain the active session id"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_fresh_session_has_correct_workspace_root() {
        let temp_dir = create_test_dir("crudo_coord_ws_root");
        let coordinator = SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");

        let ws_root = coordinator.active_session().workspace_root();
        assert!(
            ws_root.is_some(),
            "Fresh session must have workspace root bound"
        );
        assert_eq!(
            ws_root.unwrap(),
            coordinator.store().workspace_root(),
            "Workspace root must match SessionStore workspace root"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_replacement_works() {
        let temp_dir = create_test_dir("crudo_coord_replace");
        let mut coordinator = SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");

        let original_id = coordinator.active_session_id().to_string();

        let replacement = Session::new();
        let replacement_id = replacement.session_id.clone();
        assert_ne!(
            original_id, replacement_id,
            "Replacement session should have distinct id"
        );

        coordinator.set_active_session(replacement);

        assert_eq!(coordinator.active_session_id(), replacement_id);
        assert!(
            coordinator.persistence_path().is_some(),
            "Replaced session must have persistence path configured"
        );
        assert!(
            coordinator
                .persistence_path()
                .unwrap()
                .to_string_lossy()
                .contains(&replacement_id),
            "Persistence path must match replacement session id"
        );
        assert_eq!(
            coordinator.active_session().workspace_root(),
            Some(coordinator.store().workspace_root())
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_create_fresh_session_generates_distinct_session() {
        let temp_dir = create_test_dir("crudo_coord_new_session");
        let mut coordinator = SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");

        let first_id = coordinator.active_session_id().to_string();
        coordinator.create_fresh_session();
        let second_id = coordinator.active_session_id().to_string();

        assert_ne!(
            first_id, second_id,
            "create_fresh_session must generate a new unique session id"
        );
        assert!(coordinator.persistence_path().is_some());
        assert!(coordinator
            .persistence_path()
            .unwrap()
            .to_string_lossy()
            .contains(&second_id));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_specific_persisted_session() {
        let temp_dir = create_test_dir("crudo_coord_load");
        let mut coordinator = SessionCoordinator::from_cwd(&temp_dir).expect("create coordinator");

        let first_id = coordinator.active_session_id().to_string();
        let first_path = coordinator
            .persistence_path()
            .expect("first path")
            .to_path_buf();

        if let Some(parent) = first_path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        coordinator
            .active_session()
            .save_to_path(&first_path)
            .expect("save initial session");

        coordinator.create_fresh_session();
        assert_ne!(coordinator.active_session_id(), first_id);

        coordinator
            .load_session(&first_id)
            .expect("load saved session");
        assert_eq!(coordinator.active_session_id(), first_id);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_with_no_existing_session_creates_persistent_session() {
        let temp_dir = create_test_dir("crudo_startup_no_session");
        let coordinator =
            SessionCoordinator::startup_from_cwd(&temp_dir).expect("startup coordinator");

        assert!(
            !coordinator.active_session_id().is_empty(),
            "Fresh session must have valid session id"
        );
        assert!(
            coordinator.persistence_path().is_some(),
            "Fresh session must have persistence path"
        );
        assert_eq!(
            coordinator.active_session().workspace_root(),
            Some(coordinator.store().workspace_root()),
            "Fresh session must bind workspace root"
        );
        assert!(
            coordinator.active_session().messages.is_empty(),
            "Fresh session starts with no messages"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_with_existing_session_loads_latest_session() {
        let temp_dir = create_test_dir("crudo_startup_existing");

        // 1. Create and persist an initial session with message
        let expected_id;
        let expected_path;
        {
            let mut coord = SessionCoordinator::from_cwd(&temp_dir).expect("initial coord");
            coord
                .active_session_mut()
                .push_user_text("Existing query from previous run")
                .expect("push message");
            expected_id = coord.active_session_id().to_string();
            expected_path = coord.persistence_path().unwrap().to_path_buf();
            // Ensure disk write
            if let Some(parent) = expected_path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            coord
                .active_session()
                .save_to_path(&expected_path)
                .expect("save session");
        }

        // 2. Run startup on the same directory
        let startup_coord =
            SessionCoordinator::startup_from_cwd(&temp_dir).expect("startup coordinator");

        assert_eq!(
            startup_coord.active_session_id(),
            expected_id,
            "Startup must load the existing persisted session ID"
        );
        assert_eq!(
            startup_coord.persistence_path(),
            Some(expected_path.as_path()),
            "Startup must preserve the persistence path"
        );
        assert_eq!(
            startup_coord.active_session().workspace_root(),
            Some(startup_coord.store().workspace_root()),
            "Startup must preserve the workspace root"
        );
        assert_eq!(
            startup_coord.active_session().messages.len(),
            1,
            "Existing conversation messages must be preserved"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_session_installed_into_real_agent_adapter() {
        let temp_dir = create_test_dir("crudo_startup_adapter");

        // 1. Create persisted session with history
        let expected_id;
        {
            let mut coord = SessionCoordinator::from_cwd(&temp_dir).expect("initial coord");
            coord
                .active_session_mut()
                .push_user_text("Query for adapter test")
                .expect("push message");
            expected_id = coord.active_session_id().to_string();
            let path = coord.persistence_path().unwrap().to_path_buf();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            coord.active_session().save_to_path(&path).expect("save");
        }

        // 2. Run startup coordinator and install into RealAgentAdapter
        let coord = SessionCoordinator::startup_from_cwd(&temp_dir).expect("startup");
        let adapter = crate::agent::real::RealAgentAdapter::new()
            .with_session(coord.active_session().clone());

        let adapter_session = adapter.get_session();
        assert_eq!(adapter_session.session_id, expected_id);
        assert_eq!(adapter_session.persistence_path(), coord.persistence_path());
        assert_eq!(
            adapter_session.workspace_root(),
            coord.active_session().workspace_root()
        );
        assert_eq!(adapter_session.messages, coord.active_session().messages);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_fails_conservatively_on_corrupted_session() {
        let temp_dir = create_test_dir("crudo_startup_corrupt");

        // Create corrupt session file in session namespace
        let coord = SessionCoordinator::from_cwd(&temp_dir).expect("coord");
        let path = coord.persistence_path().unwrap().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, "not a valid json or jsonl session").expect("write corrupt file");

        // Startup should fail conservatively and report error rather than silently ignoring
        let res = SessionCoordinator::startup_from_cwd(&temp_dir);
        assert!(
            res.is_err(),
            "Startup should return an error when persisted session is corrupted"
        );
        let err_msg = res.err().unwrap().to_string();
        assert!(
            err_msg.contains("Failed to load session"),
            "Error message should mention failed session loading, got: {err_msg}"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_hydrates_into_app_messages() {
        let temp_dir = create_test_dir("crudo_startup_hydrate");

        // 1. Create a session on disk with a complete exchange
        let expected_id;
        {
            let mut coord = SessionCoordinator::from_cwd(&temp_dir).expect("coord");
            coord
                .active_session_mut()
                .push_user_text("Saved user question")
                .expect("push user");
            coord
                .active_session_mut()
                .push_message(runtime::ConversationMessage::assistant(vec![
                    runtime::ContentBlock::Text {
                        text: "Saved assistant answer".to_string(),
                    },
                ]))
                .expect("push assistant");
            expected_id = coord.active_session_id().to_string();
            let path = coord.persistence_path().unwrap().to_path_buf();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            coord.active_session().save_to_path(&path).expect("save");
        }

        // 2. Perform startup coordinator loading and hydrate App
        let startup_coord = SessionCoordinator::startup_from_cwd(&temp_dir).expect("startup");
        assert_eq!(startup_coord.active_session_id(), expected_id);

        let mut app = crate::app::App::new();
        app.hydrate_from_session(startup_coord.active_session());

        assert_eq!(app.messages.len(), 2);
        assert_eq!(app.messages[0].role, crate::message::Role::User);
        assert_eq!(app.messages[0].content, "Saved user question");
        assert_eq!(app.messages[1].role, crate::message::Role::Assistant);
        assert_eq!(app.messages[1].content, "Saved assistant answer");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
