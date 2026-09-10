use super::events::AgentEvent;
use super::mock::MockAgent;
use super::Agent;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

pub struct AgentController {
    agent: Arc<dyn Agent>,
    is_running: bool,
    cancel_tx: Option<watch::Sender<bool>>,
    current_run_id: usize,
    active_run_id: Option<usize>,
    cancelled_run_id: Option<usize>,
}

impl AgentController {
    pub fn new() -> Self {
        Self {
            agent: Arc::new(MockAgent),
            is_running: false,
            cancel_tx: None,
            current_run_id: 0,
            active_run_id: None,
            cancelled_run_id: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_agent(agent: impl Agent + 'static) -> Self {
        Self {
            agent: Arc::new(agent),
            is_running: false,
            cancel_tx: None,
            current_run_id: 0,
            active_run_id: None,
            cancelled_run_id: None,
        }
    }

    #[allow(dead_code)]
    pub fn set_agent(&mut self, agent: impl Agent + 'static) {
        self.agent = Arc::new(agent);
    }

    #[allow(dead_code)]
    pub fn agent_name(&self) -> &'static str {
        self.agent.name()
    }

    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    #[allow(dead_code)]
    pub fn current_run_id(&self) -> usize {
        self.current_run_id
    }

    pub fn active_run_id(&self) -> Option<usize> {
        self.active_run_id
    }

    pub fn cancelled_run_id(&self) -> Option<usize> {
        self.cancelled_run_id
    }

    pub fn mark_completed(&mut self) {
        self.is_running = false;
        self.cancel_tx = None;
        self.active_run_id = None;
    }

    pub fn cancel(&mut self) {
        if let Some(run_id) = self.active_run_id.take() {
            self.cancelled_run_id = Some(run_id);
        } else if self.current_run_id > 0 && self.cancelled_run_id.is_none() {
            self.cancelled_run_id = Some(self.current_run_id);
        }
        if self.is_running {
            if let Some(tx) = &self.cancel_tx {
                let _ = tx.send(true);
            }
            self.is_running = false;
        }
    }

    pub fn consume_cancelled(&mut self) {
        self.cancelled_run_id = None;
    }

    pub fn submit_prompt(
        &mut self,
        prompt: String,
        tx: mpsc::Sender<(usize, AgentEvent)>,
    ) -> usize {
        if self.is_running {
            self.cancel();
        }
        self.is_running = true;
        self.current_run_id += 1;
        let run_id = self.current_run_id;
        self.active_run_id = Some(run_id);
        self.cancelled_run_id = None;

        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.cancel_tx = Some(cancel_tx);

        let agent = self.agent.clone();
        tokio::spawn(async move {
            agent.execute(prompt, tx, cancel_rx, run_id).await;
        });

        run_id
    }
}

impl Default for AgentController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::real::RealAgentAdapter;

    struct TestCustomAgent;

    impl Agent for TestCustomAgent {
        fn name(&self) -> &'static str {
            "TestCustomAgent"
        }

        fn execute(
            &self,
            _prompt: String,
            tx: mpsc::Sender<(usize, AgentEvent)>,
            _cancel_rx: watch::Receiver<bool>,
            run_id: usize,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
            Box::pin(async move {
                let _ = tx.send((run_id, AgentEvent::Started)).await;
                let _ = tx.send((run_id, AgentEvent::Completed)).await;
            })
        }
    }

    #[test]
    fn test_agent_controller_defaults_to_mock() {
        let controller = AgentController::new();
        assert_eq!(controller.agent_name(), "MockAgent");
        assert!(!controller.is_running());
        assert_eq!(controller.current_run_id(), 0);
    }

    #[test]
    fn test_agent_controller_custom_and_switch() {
        let mut controller = AgentController::with_agent(RealAgentAdapter::new());
        assert_eq!(controller.agent_name(), "RealAgentAdapter");

        controller.set_agent(TestCustomAgent);
        assert_eq!(controller.agent_name(), "TestCustomAgent");

        controller.set_agent(MockAgent);
        assert_eq!(controller.agent_name(), "MockAgent");
    }

    #[tokio::test]
    async fn test_agent_controller_routes_to_active_agent() {
        let mut controller = AgentController::with_agent(TestCustomAgent);
        let (tx, mut rx) = mpsc::channel(16);

        controller.submit_prompt("test prompt".to_string(), tx);
        assert!(controller.is_running());
        assert_eq!(controller.current_run_id(), 1);

        let ev1 = rx.recv().await.expect("ev1");
        assert_eq!(ev1.0, 1);
        assert!(matches!(ev1.1, AgentEvent::Started));

        let ev2 = rx.recv().await.expect("ev2");
        assert_eq!(ev2.0, 1);
        assert!(matches!(ev2.1, AgentEvent::Completed));

        controller.mark_completed();
        assert!(!controller.is_running());
    }

    #[tokio::test]
    async fn test_agent_controller_cancel_flow() {
        let mut controller = AgentController::with_agent(RealAgentAdapter::new());
        let (tx, _rx) = mpsc::channel(16);

        controller.submit_prompt("hello".to_string(), tx);
        assert!(controller.is_running());

        controller.cancel();
        assert!(!controller.is_running());
    }
}
