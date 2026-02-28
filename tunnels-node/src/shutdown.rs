//! Graceful shutdown coordination.

use tokio::sync::broadcast;

/// Shutdown signal sender.
pub type ShutdownTx = broadcast::Sender<()>;

/// Shutdown signal receiver.
pub type ShutdownRx = broadcast::Receiver<()>;

/// Create a shutdown channel.
pub fn shutdown_channel() -> (ShutdownTx, ShutdownRx) {
    broadcast::channel(1)
}

/// Wait for a shutdown signal (SIGINT or SIGTERM).
pub async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT, initiating shutdown...");
            }
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, initiating shutdown...");
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C, initiating shutdown...");
    }
}

/// A guard that holds a shutdown receiver and can check for shutdown.
#[allow(dead_code)]
pub struct ShutdownGuard {
    rx: ShutdownRx,
}

impl ShutdownGuard {
    /// Create a new shutdown guard from a sender.
    #[allow(dead_code)]
    pub fn new(tx: &ShutdownTx) -> Self {
        Self {
            rx: tx.subscribe(),
        }
    }

    /// Wait for the shutdown signal.
    #[allow(dead_code)]
    pub async fn wait(&mut self) {
        let _ = self.rx.recv().await;
    }

    /// Check if shutdown has been signaled (non-blocking).
    #[allow(dead_code)]
    pub fn is_shutdown(&mut self) -> bool {
        matches!(self.rx.try_recv(), Ok(_) | Err(broadcast::error::TryRecvError::Closed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_channel() {
        let (tx, _rx) = shutdown_channel();
        let mut guard = ShutdownGuard::new(&tx);

        assert!(!guard.is_shutdown());

        tx.send(()).unwrap();

        // After sending, the guard should detect shutdown
        assert!(guard.is_shutdown());
    }
}
