use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::events::CrudoEvent;
use crate::system::{CpuMonitor, GpuMonitor, SystemMetrics};

#[derive(Debug, Clone)]
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
    SystemUpdate(SystemMetrics),
    Backend(CrudoEvent),
}

pub struct EventHandler {
    receiver: mpsc::UnboundedReceiver<AppEvent>,
    _sender: mpsc::UnboundedSender<AppEvent>,
}

impl EventHandler {
    pub fn new(
        tick_rate: Duration,
        metrics_rate: Duration,
        mut backend_rx: Option<tokio::sync::broadcast::Receiver<CrudoEvent>>,
    ) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let event_sender = sender.clone();

        // 1. Crossterm input reader task (dedicated blocking thread, zero polling latency)
        let input_sender = sender.clone();
        std::thread::spawn(move || {
            while let Ok(ev) = event::read() {
                let app_event = match ev {
                    CrosstermEvent::Key(key) => AppEvent::Key(key),
                    CrosstermEvent::Mouse(mouse) => match mouse.kind {
                        crossterm::event::MouseEventKind::ScrollUp
                        | crossterm::event::MouseEventKind::ScrollDown => AppEvent::Mouse(mouse),
                        _ => continue,
                    },
                    CrosstermEvent::Resize(w, h) => AppEvent::Resize(w, h),
                    _ => continue,
                };
                if input_sender.send(app_event).is_err() {
                    break;
                }
            }
        });

        // 2. Tick task for UI animations, clock updates
        let tick_sender = sender.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                interval.tick().await;
                if tick_sender.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        // 3. System Metrics background worker on dedicated thread (prevents nvidia-smi from blocking Tokio runtime)
        let metrics_sender = sender.clone();
        std::thread::spawn(move || {
            let mut cpu_monitor = CpuMonitor::new();
            let mut gpu_monitor = GpuMonitor::new();
            loop {
                std::thread::sleep(metrics_rate);
                let (cpu_usage, cpu_cores) = cpu_monitor.refresh();
                let gpu_usage = gpu_monitor.query_utilization();

                let metrics = SystemMetrics {
                    cpu_usage: Some(cpu_usage),
                    cpu_cores,
                    gpu_usage,
                };

                if metrics_sender
                    .send(AppEvent::SystemUpdate(metrics))
                    .is_err()
                {
                    break;
                }
            }
        });

        // 4. Backend event subscription bridge
        if let Some(mut rx) = backend_rx.take() {
            let backend_sender = sender.clone();
            tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    if backend_sender.send(AppEvent::Backend(event)).is_err() {
                        break;
                    }
                }
            });
        }

        Self {
            receiver,
            _sender: event_sender,
        }
    }

    /// Receives the next incoming application event asynchronously.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.receiver.recv().await
    }

    /// Non-blocking check for any pending application event in the queue.
    pub fn try_recv(&mut self) -> Option<AppEvent> {
        self.receiver.try_recv().ok()
    }
}
