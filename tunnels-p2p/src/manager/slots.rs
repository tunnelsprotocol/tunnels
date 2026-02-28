//! Connection slot tracking.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use crate::peer::{ConnectionDirection, PeerId, PeerInfo};

/// Tracks connection slots and limits.
#[derive(Debug)]
pub struct ConnectionSlots {
    /// Maximum outbound connections.
    max_outbound: usize,
    /// Maximum inbound connections.
    max_inbound: usize,
    /// Current outbound connection count.
    outbound_count: usize,
    /// Current inbound connection count.
    inbound_count: usize,
    /// Connected peer IDs by address.
    by_address: HashMap<SocketAddr, PeerId>,
    /// Connected peer info by ID.
    peers: HashMap<PeerId, PeerInfo>,
    /// Addresses we're currently connecting to.
    connecting: HashSet<SocketAddr>,
}

impl ConnectionSlots {
    /// Create a new slot tracker with the given limits.
    pub fn new(max_outbound: usize, max_inbound: usize) -> Self {
        Self {
            max_outbound,
            max_inbound,
            outbound_count: 0,
            inbound_count: 0,
            by_address: HashMap::new(),
            peers: HashMap::new(),
            connecting: HashSet::new(),
        }
    }

    /// Check if we can accept a new outbound connection.
    pub fn can_connect_outbound(&self) -> bool {
        self.outbound_count < self.max_outbound
    }

    /// Check if we can accept a new inbound connection.
    pub fn can_accept_inbound(&self) -> bool {
        self.inbound_count < self.max_inbound
    }

    /// Get available outbound slots.
    pub fn available_outbound(&self) -> usize {
        self.max_outbound.saturating_sub(self.outbound_count)
    }

    /// Get available inbound slots.
    pub fn available_inbound(&self) -> usize {
        self.max_inbound.saturating_sub(self.inbound_count)
    }

    /// Check if we're already connected to an address.
    pub fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.by_address.contains_key(addr)
    }

    /// Check if we're currently connecting to an address.
    pub fn is_connecting(&self, addr: &SocketAddr) -> bool {
        self.connecting.contains(addr)
    }

    /// Check if we should connect to an address.
    pub fn should_connect(&self, addr: &SocketAddr) -> bool {
        !self.is_connected(addr) && !self.is_connecting(addr) && self.can_connect_outbound()
    }

    /// Mark an address as connecting.
    pub fn start_connecting(&mut self, addr: SocketAddr) {
        self.connecting.insert(addr);
    }

    /// Remove an address from the connecting set.
    pub fn stop_connecting(&mut self, addr: &SocketAddr) {
        self.connecting.remove(addr);
    }

    /// Add a connected peer.
    pub fn add_peer(&mut self, info: PeerInfo) {
        let peer_id = info.id;
        let addr = info.addr;
        let direction = info.direction;

        self.connecting.remove(&addr);

        match direction {
            ConnectionDirection::Outbound => {
                self.outbound_count += 1;
            }
            ConnectionDirection::Inbound => {
                self.inbound_count += 1;
            }
        }

        self.by_address.insert(addr, peer_id);
        self.peers.insert(peer_id, info);
    }

    /// Remove a peer.
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Option<PeerInfo> {
        if let Some(info) = self.peers.remove(peer_id) {
            self.by_address.remove(&info.addr);
            self.connecting.remove(&info.addr);

            match info.direction {
                ConnectionDirection::Outbound => {
                    self.outbound_count = self.outbound_count.saturating_sub(1);
                }
                ConnectionDirection::Inbound => {
                    self.inbound_count = self.inbound_count.saturating_sub(1);
                }
            }

            Some(info)
        } else {
            None
        }
    }

    /// Get a peer by ID.
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    /// Get a mutable reference to a peer by ID.
    pub fn get_peer_mut(&mut self, peer_id: &PeerId) -> Option<&mut PeerInfo> {
        self.peers.get_mut(peer_id)
    }

    /// Get peer by address.
    pub fn get_peer_by_addr(&self, addr: &SocketAddr) -> Option<&PeerInfo> {
        self.by_address.get(addr).and_then(|id| self.peers.get(id))
    }

    /// Get all connected peer IDs.
    pub fn peer_ids(&self) -> Vec<PeerId> {
        self.peers.keys().copied().collect()
    }

    /// Get all connected peer addresses.
    pub fn peer_addrs(&self) -> Vec<SocketAddr> {
        self.by_address.keys().copied().collect()
    }

    /// Get total connection count.
    pub fn total_count(&self) -> usize {
        self.outbound_count + self.inbound_count
    }

    /// Get outbound connection count.
    pub fn outbound_count(&self) -> usize {
        self.outbound_count
    }

    /// Get inbound connection count.
    pub fn inbound_count(&self) -> usize {
        self.inbound_count
    }

    /// Get connecting count.
    pub fn connecting_count(&self) -> usize {
        self.connecting.len()
    }

    /// Iterate over all peers.
    pub fn iter(&self) -> impl Iterator<Item = &PeerInfo> {
        self.peers.values()
    }

    /// Get peers with the highest best_height.
    pub fn peers_at_height(&self, height: u64) -> Vec<PeerId> {
        self.peers
            .iter()
            .filter(|(_, info)| info.best_height >= height)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get the peer with the best height.
    pub fn best_peer(&self) -> Option<PeerId> {
        self.peers
            .iter()
            .max_by_key(|(_, info)| info.best_height)
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer(id: u64, addr: &str, direction: ConnectionDirection) -> PeerInfo {
        PeerInfo::new(PeerId::new(id), addr.parse().unwrap(), direction)
    }

    #[test]
    fn test_slot_tracking() {
        let mut slots = ConnectionSlots::new(2, 2);

        assert!(slots.can_connect_outbound());
        assert!(slots.can_accept_inbound());
        assert_eq!(slots.available_outbound(), 2);

        // Add outbound peer
        let peer1 = make_peer(1, "127.0.0.1:8333", ConnectionDirection::Outbound);
        slots.add_peer(peer1);

        assert!(slots.can_connect_outbound());
        assert_eq!(slots.outbound_count(), 1);
        assert_eq!(slots.available_outbound(), 1);

        // Add another outbound
        let peer2 = make_peer(2, "127.0.0.2:8333", ConnectionDirection::Outbound);
        slots.add_peer(peer2);

        assert!(!slots.can_connect_outbound());
        assert_eq!(slots.outbound_count(), 2);
    }

    #[test]
    fn test_connecting_state() {
        let mut slots = ConnectionSlots::new(2, 2);
        let addr: SocketAddr = "127.0.0.1:8333".parse().unwrap();

        assert!(slots.should_connect(&addr));

        slots.start_connecting(addr);
        assert!(!slots.should_connect(&addr));
        assert!(slots.is_connecting(&addr));

        slots.stop_connecting(&addr);
        assert!(slots.should_connect(&addr));
    }

    #[test]
    fn test_remove_peer() {
        let mut slots = ConnectionSlots::new(2, 2);

        let peer = make_peer(1, "127.0.0.1:8333", ConnectionDirection::Outbound);
        let addr = peer.addr;
        let peer_id = peer.id;

        slots.add_peer(peer);
        assert!(slots.is_connected(&addr));
        assert_eq!(slots.outbound_count(), 1);

        let removed = slots.remove_peer(&peer_id);
        assert!(removed.is_some());
        assert!(!slots.is_connected(&addr));
        assert_eq!(slots.outbound_count(), 0);
    }
}
