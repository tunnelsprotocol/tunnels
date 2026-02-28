//! Version handshake logic.
//!
//! The handshake protocol:
//! 1. Both sides send Version message
//! 2. Upon receiving Version, validate and send VersionAck
//! 3. Connection is established when both sides have sent and received VersionAck

use std::net::SocketAddr;
use std::time::SystemTime;

use crate::config::PROTOCOL_VERSION;
use crate::error::{P2pError, P2pResult};
use crate::protocol::{Message, VersionMessage};

/// Create a version message for handshake.
pub fn create_version_message(
    genesis_hash: [u8; 32],
    best_height: u64,
    best_hash: [u8; 32],
    user_agent: &str,
    listen_addr: Option<SocketAddr>,
) -> VersionMessage {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    VersionMessage {
        protocol_version: PROTOCOL_VERSION,
        genesis_hash,
        best_height,
        best_hash,
        user_agent: user_agent.to_string(),
        timestamp,
        listen_addr,
    }
}

/// Validate a received version message.
///
/// Checks:
/// - Protocol version is compatible
/// - Genesis hash matches
pub fn validate_version(
    received: &VersionMessage,
    our_genesis_hash: [u8; 32],
) -> P2pResult<()> {
    // Check protocol version compatibility
    // For now, require exact match; later could allow compatible ranges
    if received.protocol_version != PROTOCOL_VERSION {
        return Err(P2pError::IncompatibleVersion {
            peer_version: received.protocol_version,
            our_version: PROTOCOL_VERSION,
        });
    }

    // Check genesis hash matches
    if received.genesis_hash != our_genesis_hash {
        return Err(P2pError::GenesisMismatch {
            expected: hex_encode(&our_genesis_hash),
            actual: hex_encode(&received.genesis_hash),
        });
    }

    Ok(())
}

/// Result of handshake validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResult {
    /// Protocol version agreed upon.
    pub protocol_version: u32,
    /// Peer's best block height.
    pub best_height: u64,
    /// Peer's best block hash.
    pub best_hash: [u8; 32],
    /// Peer's user agent.
    pub user_agent: String,
    /// Peer's listening address (if provided).
    pub listen_addr: Option<SocketAddr>,
}

impl From<VersionMessage> for HandshakeResult {
    fn from(msg: VersionMessage) -> Self {
        Self {
            protocol_version: msg.protocol_version,
            best_height: msg.best_height,
            best_hash: msg.best_hash,
            user_agent: msg.user_agent,
            listen_addr: msg.listen_addr,
        }
    }
}

/// Helper to encode bytes as hex for error messages.
fn hex_encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Handshake state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeState {
    /// Initial state, no messages sent.
    Initial,
    /// Version sent, waiting for peer's version.
    VersionSent,
    /// Version received, waiting for our version ack to be sent.
    VersionReceived(VersionMessage),
    /// Both versions exchanged, waiting for version ack.
    AwaitingAck(VersionMessage),
    /// Handshake complete.
    Complete(HandshakeResult),
    /// Handshake failed.
    Failed(String),
}

impl HandshakeState {
    /// Create initial handshake state.
    pub fn new() -> Self {
        Self::Initial
    }

    /// Process sending our version message.
    pub fn sent_version(&mut self) -> P2pResult<()> {
        match self {
            Self::Initial => {
                *self = Self::VersionSent;
                Ok(())
            }
            Self::VersionReceived(msg) => {
                let msg = msg.clone();
                *self = Self::AwaitingAck(msg);
                Ok(())
            }
            _ => Err(P2pError::HandshakeFailed(
                "Invalid state: cannot send version".to_string(),
            )),
        }
    }

    /// Process receiving a version message.
    pub fn received_version(
        &mut self,
        msg: VersionMessage,
        our_genesis_hash: [u8; 32],
    ) -> P2pResult<()> {
        // Validate the version
        validate_version(&msg, our_genesis_hash)?;

        match self {
            Self::Initial => {
                *self = Self::VersionReceived(msg);
                Ok(())
            }
            Self::VersionSent => {
                *self = Self::AwaitingAck(msg);
                Ok(())
            }
            _ => Err(P2pError::HandshakeFailed(
                "Invalid state: unexpected version".to_string(),
            )),
        }
    }

    /// Process sending version ack.
    pub fn sent_version_ack(&mut self) -> P2pResult<()> {
        // This doesn't change state, just validates we're in a valid state
        match self {
            Self::VersionReceived(_) | Self::AwaitingAck(_) => Ok(()),
            _ => Err(P2pError::HandshakeFailed(
                "Invalid state: cannot send verack".to_string(),
            )),
        }
    }

    /// Process receiving version ack.
    pub fn received_version_ack(&mut self) -> P2pResult<()> {
        match self {
            Self::AwaitingAck(msg) => {
                let result = HandshakeResult::from(msg.clone());
                *self = Self::Complete(result);
                Ok(())
            }
            _ => Err(P2pError::HandshakeFailed(
                "Invalid state: unexpected verack".to_string(),
            )),
        }
    }

    /// Check if handshake is complete.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete(_))
    }

    /// Get the handshake result if complete.
    pub fn result(&self) -> Option<&HandshakeResult> {
        match self {
            Self::Complete(r) => Some(r),
            _ => None,
        }
    }

    /// Get the next message to send, if any.
    pub fn next_message(
        &self,
        genesis_hash: [u8; 32],
        best_height: u64,
        best_hash: [u8; 32],
        user_agent: &str,
        listen_addr: Option<SocketAddr>,
    ) -> Option<Message> {
        match self {
            Self::Initial => Some(Message::Version(create_version_message(
                genesis_hash,
                best_height,
                best_hash,
                user_agent,
                listen_addr,
            ))),
            Self::VersionReceived(_) => Some(Message::Version(create_version_message(
                genesis_hash,
                best_height,
                best_hash,
                user_agent,
                listen_addr,
            ))),
            Self::VersionSent => None, // Wait for peer's version
            Self::AwaitingAck(_) => Some(Message::VersionAck),
            Self::Complete(_) => None,
            Self::Failed(_) => None,
        }
    }
}

impl Default for HandshakeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_genesis_hash() -> [u8; 32] {
        [0u8; 32]
    }

    fn test_version() -> VersionMessage {
        create_version_message(
            test_genesis_hash(),
            100,
            [1u8; 32],
            "test/1.0",
            None,
        )
    }

    #[test]
    fn test_validate_version_success() {
        let msg = test_version();
        assert!(validate_version(&msg, test_genesis_hash()).is_ok());
    }

    #[test]
    fn test_validate_version_genesis_mismatch() {
        let msg = test_version();
        let different_genesis = [1u8; 32];
        let result = validate_version(&msg, different_genesis);
        assert!(matches!(result, Err(P2pError::GenesisMismatch { .. })));
    }

    #[test]
    fn test_handshake_outbound() {
        // Outbound connection: we send version first
        let mut state = HandshakeState::new();

        // Send our version
        state.sent_version().unwrap();
        assert!(matches!(state, HandshakeState::VersionSent));

        // Receive peer's version
        state.received_version(test_version(), test_genesis_hash()).unwrap();
        assert!(matches!(state, HandshakeState::AwaitingAck(_)));

        // Send verack
        state.sent_version_ack().unwrap();

        // Receive verack
        state.received_version_ack().unwrap();
        assert!(state.is_complete());
    }

    #[test]
    fn test_handshake_inbound() {
        // Inbound connection: peer sends version first
        let mut state = HandshakeState::new();

        // Receive peer's version first
        state.received_version(test_version(), test_genesis_hash()).unwrap();
        assert!(matches!(state, HandshakeState::VersionReceived(_)));

        // Send our version
        state.sent_version().unwrap();
        assert!(matches!(state, HandshakeState::AwaitingAck(_)));

        // Send verack
        state.sent_version_ack().unwrap();

        // Receive verack
        state.received_version_ack().unwrap();
        assert!(state.is_complete());
    }
}
