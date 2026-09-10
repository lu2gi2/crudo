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
}
