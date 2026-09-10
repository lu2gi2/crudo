use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;

pub enum Event {
    Tick,
    Key(KeyEvent),
    Paste(String),
    Resize(u16, u16),
}

pub struct EventHandler {
    receiver: mpsc::Receiver<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        let (sender, receiver) = mpsc::channel(32);

        let sender_clone = sender.clone();
        tokio::task::spawn_blocking(move || loop {
            if event::poll(Duration::from_millis(250)).unwrap_or(false) {
                if let Ok(event) = event::read() {
                    match event {
                        CrosstermEvent::Key(key) => {
                            if key.kind == crossterm::event::KeyEventKind::Press
                                || key.kind == crossterm::event::KeyEventKind::Repeat
                            {
                                if sender_clone.blocking_send(Event::Key(key)).is_err() {
                                    break;
                                }
                            }
                        }
                        CrosstermEvent::Paste(text) => {
                            if sender_clone.blocking_send(Event::Paste(text)).is_err() {
                                break;
                            }
                        }
                        CrosstermEvent::Resize(w, h) => {
                            if sender_clone.blocking_send(Event::Resize(w, h)).is_err() {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        let sender_clone2 = sender.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(tick_rate));
            loop {
                interval.tick().await;
                if sender_clone2.send(Event::Tick).await.is_err() {
                    break;
                }
            }
        });

        Self { receiver }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }
}
