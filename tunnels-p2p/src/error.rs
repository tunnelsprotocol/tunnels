//! P2P error types.

use std::io;
use std::net::SocketAddr;
use thiserror::Error;

/// P2P-specific errors.
#[derive(Debug, Error)]
pub enum P2pError {
    /// I/O error during network operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Failed to serialize or deserialize a message.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Message exceeds maximum allowed size.
    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    /// Invalid network magic bytes.
    #[error("Invalid network magic: expected {expected:?}, got {actual:?}")]
    InvalidMagic { expected: [u8; 4], actual: [u8; 4] },

    /// Handshake failed.
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    /// Handshake timed out.
    #[error("Handshake timeout")]
    HandshakeTimeout,

    /// Connection timed out.
    #[error("Connection timeout to {addr}")]
    ConnectionTimeout { addr: SocketAddr },

    /// Peer disconnected unexpectedly.
    #[error("Peer disconnected: {reason}")]
    PeerDisconnected { reason: String },

    /// Genesis hash mismatch during handshake.
    #[error("Genesis mismatch: expected {expected}, got {actual}")]
    GenesisMismatch { expected: String, actual: String },

    /// Protocol version incompatible.
    #[error("Incompatible protocol version: {peer_version} (our version: {our_version})")]
    IncompatibleVersion { peer_version: u32, our_version: u32 },

    /// Peer sent an unexpected message.
    #[error("Unexpected message: expected {expected}, got {actual}")]
    UnexpectedMessage { expected: String, actual: String },

    /// Peer already connected.
    #[error("Already connected to peer: {addr}")]
    AlreadyConnected { addr: SocketAddr },

    /// Maximum connections reached.
    #[error("Maximum connections reached: {max}")]
    MaxConnectionsReached { max: usize },

    /// Invalid peer address.
    #[error("Invalid peer address: {0}")]
    InvalidAddress(String),

    /// DNS resolution failed.
    #[error("DNS resolution failed for {host}: {error}")]
    DnsResolutionFailed { host: String, error: String },

    /// Block validation failed.
    #[error("Block validation failed: {0}")]
    BlockValidationFailed(String),

    /// Transaction validation failed.
    #[error("Transaction validation failed: {0}")]
    TransactionValidationFailed(String),

    /// Sync error.
    #[error("Sync error: {0}")]
    SyncError(String),

    /// Channel send error.
    #[error("Channel send error: {0}")]
    ChannelSend(String),

    /// Channel receive error.
    #[error("Channel receive error: {0}")]
    ChannelRecv(String),

    /// Peer not found.
    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    /// Invalid block locator.
    #[error("Invalid block locator: {0}")]
    InvalidLocator(String),

    /// Ping timeout.
    #[error("Ping timeout for peer")]
    PingTimeout,

    /// Node is shutting down.
    #[error("Node shutting down")]
    Shutdown,
}

impl From<tunnels_core::SerializationError> for P2pError {
    fn from(err: tunnels_core::SerializationError) -> Self {
        P2pError::Serialization(err.to_string())
    }
}

impl From<tunnels_chain::ChainError> for P2pError {
    fn from(err: tunnels_chain::ChainError) -> Self {
        P2pError::BlockValidationFailed(err.to_string())
    }
}

/// Result type for P2P operations.
pub type P2pResult<T> = Result<T, P2pError>;
