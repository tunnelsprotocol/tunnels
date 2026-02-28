//! Outbound connection management.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::config::P2pConfig;
use crate::error::P2pError;
use crate::peer::{spawn_peer_connection, ConnectionDirection, PeerEvent, PeerId};

/// Result of an outbound connection attempt.
pub enum ConnectResult {
    /// Connection successful, includes the peer task handle.
    Success(PeerId, JoinHandle<()>),
    /// Connection failed.
    Failed(SocketAddr, P2pError),
}

/// Attempt to connect to a peer.
pub async fn connect_to_peer(
    peer_id: PeerId,
    addr: SocketAddr,
    config: Arc<P2pConfig>,
    event_tx: mpsc::Sender<PeerEvent>,
    genesis_hash: [u8; 32],
    best_height: u64,
    best_hash: [u8; 32],
) -> ConnectResult {
    tracing::debug!(addr = %addr, "Connecting to peer");

    // Attempt TCP connection with timeout
    let stream = match timeout(config.connect_timeout, TcpStream::connect(addr)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            return ConnectResult::Failed(addr, P2pError::Io(e));
        }
        Err(_) => {
            return ConnectResult::Failed(
                addr,
                P2pError::ConnectionTimeout { addr },
            );
        }
    };

    // Configure the stream
    if let Err(e) = stream.set_nodelay(true) {
        tracing::warn!(addr = %addr, error = %e, "Failed to set TCP_NODELAY");
    }

    tracing::debug!(addr = %addr, "TCP connection established, starting handshake");

    // Spawn the peer connection handler
    let (_command_tx, handle) = spawn_peer_connection(
        peer_id,
        addr,
        ConnectionDirection::Outbound,
        stream,
        event_tx,
        config,
        genesis_hash,
        best_height,
        best_hash,
    );

    ConnectResult::Success(peer_id, handle)
}

/// Manager for outbound connections.
pub struct OutboundManager {
    /// P2P configuration.
    config: Arc<P2pConfig>,
    /// Target number of outbound connections.
    target_outbound: usize,
    /// Next peer ID to assign.
    next_peer_id: u64,
}

impl OutboundManager {
    /// Create a new outbound manager.
    pub fn new(config: Arc<P2pConfig>) -> Self {
        Self {
            target_outbound: config.target_outbound,
            config,
            next_peer_id: 1,
        }
    }

    /// Get the target number of outbound connections.
    pub fn target(&self) -> usize {
        self.target_outbound
    }

    /// Allocate a new peer ID.
    pub fn next_peer_id(&mut self) -> PeerId {
        let id = self.next_peer_id;
        self.next_peer_id += 1;
        PeerId::new(id)
    }

    /// Get the connection timeout.
    pub fn connect_timeout(&self) -> Duration {
        self.config.connect_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbound_manager() {
        let config = Arc::new(P2pConfig::default());
        let mut manager = OutboundManager::new(config);

        assert_eq!(manager.target(), 8); // Default target

        let id1 = manager.next_peer_id();
        let id2 = manager.next_peer_id();

        assert_eq!(id1, PeerId::new(1));
        assert_eq!(id2, PeerId::new(2));
    }
}
