//! Peer information and identification.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::SocketAddr;
use std::time::Instant;

/// Unique identifier for a peer connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub u64);

impl PeerId {
    /// Create a new peer ID from a counter value.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "peer-{}", self.0)
    }
}

/// Direction of the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionDirection {
    /// We initiated the connection.
    Outbound,
    /// Peer connected to us.
    Inbound,
}

impl fmt::Display for ConnectionDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionDirection::Outbound => write!(f, "outbound"),
            ConnectionDirection::Inbound => write!(f, "inbound"),
        }
    }
}

/// Information about a connected peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Unique peer identifier for this session.
    pub id: PeerId,
    /// Socket address of the peer.
    pub addr: SocketAddr,
    /// Direction of the connection.
    pub direction: ConnectionDirection,
    /// Protocol version (set after handshake).
    pub protocol_version: Option<u32>,
    /// Peer's user agent (set after handshake).
    pub user_agent: Option<String>,
    /// Peer's best block height (updated during sync).
    pub best_height: u64,
    /// Peer's best block hash (updated during sync).
    pub best_hash: [u8; 32],
    /// When the connection was established.
    pub connected_at: Instant,
    /// Last time we received a message from this peer.
    pub last_recv: Instant,
    /// Last time we sent a message to this peer.
    pub last_send: Instant,
    /// Number of bytes received from this peer.
    pub bytes_recv: u64,
    /// Number of bytes sent to this peer.
    pub bytes_sent: u64,
    /// Number of messages received from this peer.
    pub messages_recv: u64,
    /// Number of messages sent to this peer.
    pub messages_sent: u64,
    /// Last ping nonce sent (for matching pong).
    pub last_ping_nonce: Option<u64>,
    /// Round-trip time from last ping/pong.
    pub ping_rtt: Option<std::time::Duration>,
}

impl PeerInfo {
    /// Create info for a new peer connection.
    pub fn new(id: PeerId, addr: SocketAddr, direction: ConnectionDirection) -> Self {
        let now = Instant::now();
        Self {
            id,
            addr,
            direction,
            protocol_version: None,
            user_agent: None,
            best_height: 0,
            best_hash: [0u8; 32],
            connected_at: now,
            last_recv: now,
            last_send: now,
            bytes_recv: 0,
            bytes_sent: 0,
            messages_recv: 0,
            messages_sent: 0,
            last_ping_nonce: None,
            ping_rtt: None,
        }
    }

    /// Update info after successful handshake.
    pub fn complete_handshake(
        &mut self,
        protocol_version: u32,
        user_agent: String,
        best_height: u64,
        best_hash: [u8; 32],
    ) {
        self.protocol_version = Some(protocol_version);
        self.user_agent = Some(user_agent);
        self.best_height = best_height;
        self.best_hash = best_hash;
    }

    /// Record that we received a message.
    pub fn record_recv(&mut self, bytes: u64) {
        self.last_recv = Instant::now();
        self.bytes_recv += bytes;
        self.messages_recv += 1;
    }

    /// Record that we sent a message.
    pub fn record_send(&mut self, bytes: u64) {
        self.last_send = Instant::now();
        self.bytes_sent += bytes;
        self.messages_sent += 1;
    }

    /// Check if this is an outbound connection.
    pub fn is_outbound(&self) -> bool {
        self.direction == ConnectionDirection::Outbound
    }

    /// Check if this is an inbound connection.
    pub fn is_inbound(&self) -> bool {
        self.direction == ConnectionDirection::Inbound
    }

    /// Get the connection duration.
    pub fn connection_duration(&self) -> std::time::Duration {
        self.connected_at.elapsed()
    }
}

impl fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}, {}, height={})",
            self.id,
            self.addr,
            self.direction,
            self.best_height
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_display() {
        let id = PeerId::new(42);
        assert_eq!(format!("{}", id), "peer-42");
    }

    #[test]
    fn test_peer_info_new() {
        let info = PeerInfo::new(
            PeerId::new(1),
            "127.0.0.1:8333".parse().unwrap(),
            ConnectionDirection::Outbound,
        );

        assert_eq!(info.id, PeerId::new(1));
        assert!(info.is_outbound());
        assert!(!info.is_inbound());
        assert!(info.protocol_version.is_none());
    }

    #[test]
    fn test_complete_handshake() {
        let mut info = PeerInfo::new(
            PeerId::new(1),
            "127.0.0.1:8333".parse().unwrap(),
            ConnectionDirection::Inbound,
        );

        info.complete_handshake(1, "test/1.0".to_string(), 100, [1u8; 32]);

        assert_eq!(info.protocol_version, Some(1));
        assert_eq!(info.user_agent, Some("test/1.0".to_string()));
        assert_eq!(info.best_height, 100);
    }
}
