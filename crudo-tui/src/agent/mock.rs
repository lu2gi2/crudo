use super::events::AgentEvent;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

pub struct MockAgent;

impl MockAgent {
    async fn sleep_or_cancel(duration: Duration, cancel_rx: &mut watch::Receiver<bool>) -> bool {
        if *cancel_rx.borrow() {
            return true;
        }
        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                *cancel_rx.borrow()
            }
            _ = cancel_rx.changed() => {
                *cancel_rx.borrow()
            }
        }
    }

    pub async fn run(
        tx: mpsc::Sender<(usize, AgentEvent)>,
        mut cancel_rx: watch::Receiver<bool>,
        run_id: usize,
    ) {
        if *cancel_rx.borrow() {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }

        let _ = tx.send((run_id, AgentEvent::Started)).await;

        if Self::sleep_or_cancel(Duration::from_millis(500), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }
        let _ = tx.send((run_id, AgentEvent::Thinking)).await;

        if Self::sleep_or_cancel(Duration::from_millis(1000), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }
        let _ = tx
            .send((
                run_id,
                AgentEvent::ToolStarted {
                    tool: "Read".to_string(),
                    summary: "src/main.rs".to_string(),
                },
            ))
            .await;

        if Self::sleep_or_cancel(Duration::from_millis(1500), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }
        let _ = tx
            .send((
                run_id,
                AgentEvent::ToolOutput {
                    id: 1, // Will be mapped by app.rs
                    output: "Reading src/main.rs...".to_string(),
                },
            ))
            .await;
        let _ = tx
            .send((
                run_id,
                AgentEvent::ToolFinished {
                    id: 1,
                    duration_ms: 1500,
                },
            ))
            .await;

        if Self::sleep_or_cancel(Duration::from_millis(500), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }
        let _ = tx
            .send((
                run_id,
                AgentEvent::ToolStarted {
                    tool: "Read".to_string(),
                    summary: "src/app.rs".to_string(),
                },
            ))
            .await;

        if Self::sleep_or_cancel(Duration::from_millis(1000), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }
        let _ = tx
            .send((
                run_id,
                AgentEvent::ToolOutput {
                    id: 2,
                    output: "Reading src/app.rs...".to_string(),
                },
            ))
            .await;
        let _ = tx
            .send((
                run_id,
                AgentEvent::ToolFinished {
                    id: 2,
                    duration_ms: 1000,
                },
            ))
            .await;

        if Self::sleep_or_cancel(Duration::from_millis(500), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }

        let chunks = [
            "Analyzing the project...\n",
            "I found several Rust modules.\n",
            "The TUI is structured correctly.\n",
            "Next, I would inspect the agent integration.",
        ];

        for chunk in chunks {
            let _ = tx
                .send((run_id, AgentEvent::TextChunk(chunk.to_string())))
                .await;
            if Self::sleep_or_cancel(Duration::from_millis(200), &mut cancel_rx).await {
                let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
                return;
            }
        }

        if Self::sleep_or_cancel(Duration::from_millis(500), &mut cancel_rx).await {
            let _ = tx.send((run_id, AgentEvent::Cancelled)).await;
            return;
        }
        let _ = tx.send((run_id, AgentEvent::Completed)).await;
    }
}

impl super::Agent for MockAgent {
    fn name(&self) -> &'static str {
        "MockAgent"
    }

    fn execute(
        &self,
        _prompt: String,
        tx: mpsc::Sender<(usize, AgentEvent)>,
        cancel_rx: watch::Receiver<bool>,
        run_id: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            MockAgent::run(tx, cancel_rx, run_id).await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Agent;
    use super::*;

    #[test]
    fn test_mock_agent_trait_compatibility() {
        let agent = MockAgent;
        assert_eq!(agent.name(), "MockAgent");
    }

    #[tokio::test]
    async fn test_mock_agent_sequence() {
        let (tx, mut rx) = mpsc::channel(32);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        tokio::spawn(async move {
            MockAgent::run(tx, cancel_rx, 1).await;
        });

        // 1. Started
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::Started));

        // 2. Thinking
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::Thinking));

        // 3. ToolStarted 1
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::ToolStarted { .. }));

        // 4. ToolOutput 1
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::ToolOutput { .. }));

        // 5. ToolFinished 1
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::ToolFinished { .. }));

        // 6. ToolStarted 2
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::ToolStarted { .. }));

        // 7. ToolOutput 2
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::ToolOutput { .. }));

        // 8. ToolFinished 2
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::ToolFinished { .. }));

        // 9. TextChunk 1
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::TextChunk(_)));

        // 10. TextChunk 2
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::TextChunk(_)));

        // 11. TextChunk 3
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::TextChunk(_)));

        // 12. TextChunk 4
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::TextChunk(_)));

        // 13. Completed
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 1);
        assert!(matches!(ev, AgentEvent::Completed));
    }

    #[tokio::test]
    async fn test_mock_agent_cancellation() {
        let (tx, mut rx) = mpsc::channel(32);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        tokio::spawn(async move {
            MockAgent::run(tx, cancel_rx, 42).await;
        });

        // 1. Started
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 42);
        assert!(matches!(ev, AgentEvent::Started));

        // Signal cancellation immediately
        let _ = cancel_tx.send(true);

        // Next event must be Cancelled
        let (run_id, ev) = rx.recv().await.unwrap();
        assert_eq!(run_id, 42);
        assert!(matches!(ev, AgentEvent::Cancelled));

        // Channel should close with no further events
        assert!(rx.recv().await.is_none());
    }
}
