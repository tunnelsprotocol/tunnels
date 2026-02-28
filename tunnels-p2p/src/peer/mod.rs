//! Peer connection management.
//!
//! This module provides:
//! - Peer identification and metadata
//! - Connection state machine
//! - Per-peer read/write loop

pub mod connection;
pub mod info;
pub mod state;

// Re-export main types
pub use connection::{spawn_peer_connection, PeerCommand, PeerConnection, PeerEvent};
pub use info::{ConnectionDirection, PeerId, PeerInfo};
pub use state::{PeerContext, PeerState};
