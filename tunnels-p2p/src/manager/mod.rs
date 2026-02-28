//! Peer manager.
//!
//! Coordinates inbound and outbound connections, tracks connection slots,
//! and routes messages to/from peers.

pub mod inbound;
pub mod outbound;
pub mod slots;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::config::P2pConfig;
use crate::error::{P2pError, P2pResult};
use crate::peer::{PeerCommand, PeerId, PeerInfo};
use crate::protocol::Message;

pub use inbound::InboundListener;
pub use outbound::{connect_to_peer, ConnectResult, OutboundManager};
pub use slots::ConnectionSlots;

/// Manages all peer connections.
pub struct PeerManager {
    /// Connection slot tracking.
    slots: ConnectionSlots,
    /// Command channels to each peer (unbounded to avoid deadlock).
    peer_commands: HashMap<PeerId, mpsc::UnboundedSender<PeerCommand>>,
    /// P2P configuration.
    config: Arc<P2pConfig>,
    /// Outbound connection manager.
    outbound: OutboundManager,
    /// Next peer ID counter.
    next_peer_id: u64,
}

impl PeerManager {
    /// Create a new peer manager.
    pub fn new(config: Arc<P2pConfig>) -> Self {
        let slots = ConnectionSlots::new(config.max_outbound, config.max_inbound);
        let outbound = OutboundManager::new(config.clone());

        Self {
            slots,
            peer_commands: HashMap::new(),
            config,
            outbound,
            next_peer_id: 1,
        }
    }

    /// Allocate a new peer ID.
    pub fn next_peer_id(&mut self) -> PeerId {
        let id = self.next_peer_id;
        self.next_peer_id += 1;
        PeerId::new(id)
    }

    /// Check if we should connect to more peers.
    pub fn needs_outbound(&self) -> bool {
        self.slots.outbound_count() + self.slots.connecting_count()
            < self.outbound.target()
    }

    /// Check if we can accept an inbound connection.
    pub fn can_accept_inbound(&self) -> bool {
        self.slots.can_accept_inbound()
    }

    /// Check if we're already connected to an address.
    pub fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.slots.is_connected(addr)
    }

    /// Check if we should connect to an address.
    pub fn should_connect(&self, addr: &SocketAddr) -> bool {
        self.slots.should_connect(addr)
    }

    /// Mark an address as connecting.
    pub fn start_connecting(&mut self, addr: SocketAddr) {
        self.slots.start_connecting(addr);
    }

    /// Remove an address from connecting state.
    pub fn stop_connecting(&mut self, addr: &SocketAddr) {
        self.slots.stop_connecting(addr);
    }

    /// Register a connected peer.
    pub fn add_peer(&mut self, info: PeerInfo, command_tx: mpsc::UnboundedSender<PeerCommand>) {
        let peer_id = info.id;
        self.slots.add_peer(info);
        self.peer_commands.insert(peer_id, command_tx);
    }

    /// Remove a peer.
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.peer_commands.remove(peer_id);
        self.slots.remove_peer(peer_id)
    }

    /// Get a peer by ID.
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.slots.get_peer(peer_id)
    }

    /// Get a mutable reference to a peer.
    pub fn get_peer_mut(&mut self, peer_id: &PeerId) -> Option<&mut PeerInfo> {
        self.slots.get_peer_mut(peer_id)
    }

    /// Update a peer's chain tip info.
    pub fn update_peer_tip(&mut self, peer_id: &PeerId, height: u64, hash: [u8; 32]) {
        if let Some(info) = self.slots.get_peer_mut(peer_id) {
            info.best_height = height;
            info.best_hash = hash;
        }
    }

    /// Send a message to a specific peer.
    /// Uses unbounded channel so this never blocks.
    pub async fn send_to_peer(&self, peer_id: &PeerId, message: Message) -> P2pResult<()> {
        let tx = self
            .peer_commands
            .get(peer_id)
            .ok_or_else(|| P2pError::PeerNotFound(peer_id.to_string()))?;

        tx.send(PeerCommand::Send(message))
            .map_err(|_| P2pError::ChannelSend("Peer command channel closed".to_string()))
    }

    /// Send a message to all connected peers.
    /// Uses unbounded channels so this never blocks.
    pub async fn broadcast(&self, message: Message) {
        for (peer_id, tx) in &self.peer_commands {
            if let Err(e) = tx.send(PeerCommand::Send(message.clone())) {
                tracing::debug!(peer = %peer_id, error = %e, "Failed to broadcast to peer");
            }
        }
    }

    /// Send a message to all peers except one.
    /// Uses unbounded channels so this never blocks.
    pub async fn broadcast_except(&self, message: Message, exclude: &PeerId) {
        for (peer_id, tx) in &self.peer_commands {
            if peer_id != exclude {
                if let Err(e) = tx.send(PeerCommand::Send(message.clone())) {
                    tracing::debug!(peer = %peer_id, error = %e, "Failed to broadcast to peer");
                }
            }
        }
    }

    /// Disconnect a peer.
    /// Uses unbounded channel so this never blocks.
    pub async fn disconnect_peer(&self, peer_id: &PeerId) -> P2pResult<()> {
        let tx = self
            .peer_commands
            .get(peer_id)
            .ok_or_else(|| P2pError::PeerNotFound(peer_id.to_string()))?;

        tx.send(PeerCommand::Disconnect)
            .map_err(|_| P2pError::ChannelSend("Peer command channel closed".to_string()))
    }

    /// Get all connected peer IDs.
    pub fn peer_ids(&self) -> Vec<PeerId> {
        self.slots.peer_ids()
    }

    /// Get connection counts.
    pub fn connection_counts(&self) -> (usize, usize) {
        (self.slots.outbound_count(), self.slots.inbound_count())
    }

    /// Get the peer with the best height.
    pub fn best_peer(&self) -> Option<PeerId> {
        self.slots.best_peer()
    }

    /// Get peers at or above a given height.
    pub fn peers_at_height(&self, height: u64) -> Vec<PeerId> {
        self.slots.peers_at_height(height)
    }

    /// Iterate over all peers.
    pub fn peers(&self) -> impl Iterator<Item = &PeerInfo> {
        self.slots.iter()
    }

    /// Get the configuration.
    pub fn config(&self) -> &P2pConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::ConnectionDirection;

    fn make_peer(id: u64, addr: &str, direction: ConnectionDirection) -> PeerInfo {
        PeerInfo::new(PeerId::new(id), addr.parse().unwrap(), direction)
    }

    #[test]
    fn test_peer_manager() {
        let config = Arc::new(P2pConfig::default());
        let mut manager = PeerManager::new(config);

        assert!(manager.needs_outbound());

        // Add a peer
        let (tx, _rx) = mpsc::unbounded_channel();
        let peer = make_peer(1, "127.0.0.1:8333", ConnectionDirection::Outbound);
        let peer_id = peer.id;

        manager.add_peer(peer, tx);

        assert!(manager.is_connected(&"127.0.0.1:8333".parse().unwrap()));
        assert!(manager.get_peer(&peer_id).is_some());

        // Remove peer
        let removed = manager.remove_peer(&peer_id);
        assert!(removed.is_some());
        assert!(!manager.is_connected(&"127.0.0.1:8333".parse().unwrap()));
    }
}
