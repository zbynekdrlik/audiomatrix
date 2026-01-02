//! Service shutdown signal.

use tokio::sync::broadcast;

/// Service shutdown signal.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    sender: broadcast::Sender<()>,
}

impl ShutdownSignal {
    /// Creates a new shutdown signal.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1);
        Self { sender }
    }

    /// Triggers a shutdown.
    pub fn shutdown(&self) {
        let _ = self.sender.send(());
    }

    /// Subscribes to the shutdown signal.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}
