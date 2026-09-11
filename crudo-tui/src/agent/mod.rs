pub mod controller;
pub mod events;
pub mod mock;
pub mod real;

use events::AgentEvent;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::{mpsc, watch};

/// Core abstraction trait for all agent implementations (MockAgent and RealAgentAdapter).
pub trait Agent: Send + Sync {
    /// Human-readable identifier for the agent
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    /// Runs a prompt execution and sends normalized events to `tx`.
    fn execute(
        &self,
        prompt: String,
        tx: mpsc::Sender<(usize, AgentEvent)>,
        cancel_rx: watch::Receiver<bool>,
        run_id: usize,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;

    /// Updates the active session used by the agent.
    #[allow(dead_code)]
    fn set_session(&self, _session: runtime::Session) {}

    /// Returns a clone of the agent's active session, if available.
    #[allow(dead_code)]
    fn get_session(&self) -> Option<runtime::Session> {
        None
    }

    /// Returns the currently configured model name, if set.
    #[allow(dead_code)]
    fn model(&self) -> Option<String> {
        None
    }

    /// Dynamically sets the model name.
    #[allow(dead_code)]
    fn set_model(&self, _model: String) {}

    /// Returns the active permission mode, if configured.
    #[allow(dead_code)]
    fn permission_mode(&self) -> Option<runtime::PermissionMode> {
        None
    }

    /// Dynamically sets the permission mode.
    #[allow(dead_code)]
    fn set_permission_mode(&self, _mode: runtime::PermissionMode) {}
}
