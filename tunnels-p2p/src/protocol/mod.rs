//! P2P protocol layer.
//!
//! This module contains:
//! - Message definitions for all 12 protocol message types
//! - Length-prefixed framing codec
//! - Version handshake logic

pub mod framing;
pub mod messages;
pub mod version;

// Re-export main types
pub use framing::MessageCodec;
pub use messages::{
    BlockDataMessage, GetBlockMessage, GetHeadersMessage, HeadersMessage, Message,
    NewBlockMessage, NewTransactionMessage, NotFoundMessage, PeerAddress, PeersMessage,
    VersionMessage,
};
pub use version::{create_version_message, validate_version, HandshakeResult, HandshakeState};
