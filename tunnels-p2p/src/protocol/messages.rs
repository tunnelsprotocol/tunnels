//! P2P protocol messages.
//!
//! This module defines all 12 message types used in the Tunnels P2P protocol.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tunnels_core::block::{Block, BlockHeader};
use tunnels_core::transaction::SignedTransaction;

/// Version information exchanged during handshake.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionMessage {
    /// Protocol version number.
    pub protocol_version: u32,
    /// Hash of the genesis block.
    pub genesis_hash: [u8; 32],
    /// Height of the best known block.
    pub best_height: u64,
    /// Hash of the best known block.
    pub best_hash: [u8; 32],
    /// User agent string.
    pub user_agent: String,
    /// Timestamp when the message was created.
    pub timestamp: u64,
    /// The sender's listening address (if any).
    pub listen_addr: Option<SocketAddr>,
}

/// Request for block headers starting from known hashes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetHeadersMessage {
    /// Block locator hashes (at exponentially increasing heights).
    pub locator_hashes: Vec<[u8; 32]>,
    /// Stop hash (return headers up to this hash, or all if zero).
    pub stop_hash: [u8; 32],
}

/// Response containing block headers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeadersMessage {
    /// The block headers.
    pub headers: Vec<BlockHeader>,
}

/// Request for a specific block by hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetBlockMessage {
    /// Hash of the requested block.
    pub hash: [u8; 32],
}

/// Response containing a full block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockDataMessage {
    /// The full block.
    pub block: Block,
}

/// Response indicating requested data was not found.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotFoundMessage {
    /// Type of item that wasn't found ("block", "transaction").
    pub item_type: String,
    /// Hash of the item that wasn't found.
    pub hash: [u8; 32],
}

/// Announce a new transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewTransactionMessage {
    /// The signed transaction.
    pub transaction: SignedTransaction,
}

/// Announce a new block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewBlockMessage {
    /// The full block.
    pub block: Block,
}

/// Response containing peer addresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeersMessage {
    /// Known peer addresses.
    pub peers: Vec<PeerAddress>,
}

/// A peer address with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerAddress {
    /// Socket address of the peer.
    pub addr: SocketAddr,
    /// Last time this peer was seen active (Unix timestamp).
    pub last_seen: u64,
}

/// All P2P protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum Message {
    // Handshake messages
    /// Version information sent at start of connection.
    Version(VersionMessage),
    /// Acknowledgment of version message.
    VersionAck,

    // Block sync messages
    /// Request for block headers.
    GetHeaders(GetHeadersMessage),
    /// Response with block headers.
    Headers(HeadersMessage),
    /// Request for a specific block.
    GetBlock(GetBlockMessage),
    /// Response with a full block.
    BlockData(BlockDataMessage),
    /// Response indicating data was not found.
    NotFound(NotFoundMessage),

    // Gossip messages
    /// Announce a new transaction.
    NewTransaction(NewTransactionMessage),
    /// Announce a new block.
    NewBlock(NewBlockMessage),

    // Peer discovery messages
    /// Request for peer addresses.
    GetPeers,
    /// Response with peer addresses.
    Peers(PeersMessage),

    // Keepalive messages
    /// Ping with a nonce.
    Ping(u64),
    /// Pong echoing the nonce.
    Pong(u64),
}

impl Message {
    /// Get a human-readable name for the message type.
    pub fn name(&self) -> &'static str {
        match self {
            Message::Version(_) => "version",
            Message::VersionAck => "verack",
            Message::GetHeaders(_) => "getheaders",
            Message::Headers(_) => "headers",
            Message::GetBlock(_) => "getblock",
            Message::BlockData(_) => "blockdata",
            Message::NotFound(_) => "notfound",
            Message::NewTransaction(_) => "newtx",
            Message::NewBlock(_) => "newblock",
            Message::GetPeers => "getpeers",
            Message::Peers(_) => "peers",
            Message::Ping(_) => "ping",
            Message::Pong(_) => "pong",
        }
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Version(v) => write!(
                f,
                "Version(version={}, height={}, agent={})",
                v.protocol_version, v.best_height, v.user_agent
            ),
            Message::VersionAck => write!(f, "VersionAck"),
            Message::GetHeaders(g) => write!(
                f,
                "GetHeaders(locators={}, stop={:?})",
                g.locator_hashes.len(),
                &g.stop_hash[..8]
            ),
            Message::Headers(h) => write!(f, "Headers(count={})", h.headers.len()),
            Message::GetBlock(g) => write!(f, "GetBlock(hash={:?})", &g.hash[..8]),
            Message::BlockData(b) => {
                write!(f, "BlockData(height={})", b.block.header.height)
            }
            Message::NotFound(n) => {
                write!(f, "NotFound(type={}, hash={:?})", n.item_type, &n.hash[..8])
            }
            Message::NewTransaction(t) => {
                write!(f, "NewTransaction(id={:?})", &t.transaction.id()[..8])
            }
            Message::NewBlock(b) => write!(f, "NewBlock(height={})", b.block.header.height),
            Message::GetPeers => write!(f, "GetPeers"),
            Message::Peers(p) => write!(f, "Peers(count={})", p.peers.len()),
            Message::Ping(n) => write!(f, "Ping({})", n),
            Message::Pong(n) => write!(f, "Pong({})", n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_names() {
        assert_eq!(Message::VersionAck.name(), "verack");
        assert_eq!(Message::GetPeers.name(), "getpeers");
        assert_eq!(Message::Ping(42).name(), "ping");
    }

    #[test]
    fn test_message_display() {
        let msg = Message::Ping(12345);
        assert_eq!(format!("{}", msg), "Ping(12345)");

        let msg = Message::VersionAck;
        assert_eq!(format!("{}", msg), "VersionAck");
    }
}
