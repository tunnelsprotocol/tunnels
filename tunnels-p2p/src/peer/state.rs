//! Peer state machine.

use std::fmt;

/// State of a peer connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeerState {
    /// TCP connection established, awaiting handshake.
    #[default]
    Connecting,
    /// We sent our Version message, awaiting response.
    VersionSent,
    /// Handshake complete, peer is connected.
    Connected,
    /// Currently syncing headers from this peer.
    SyncingHeaders,
    /// Currently syncing blocks from this peer.
    SyncingBlocks,
    /// Fully synced, participating in gossip.
    Live,
    /// Gracefully disconnecting.
    Disconnecting,
}

impl PeerState {
    /// Check if the peer has completed handshake.
    pub fn is_connected(&self) -> bool {
        !matches!(self, PeerState::Connecting | PeerState::VersionSent)
    }

    /// Check if the peer is syncing.
    pub fn is_syncing(&self) -> bool {
        matches!(self, PeerState::SyncingHeaders | PeerState::SyncingBlocks)
    }

    /// Check if the peer is live (ready for gossip).
    pub fn is_live(&self) -> bool {
        matches!(self, PeerState::Live)
    }

    /// Check if the peer is disconnecting or already disconnected.
    pub fn is_disconnecting(&self) -> bool {
        matches!(self, PeerState::Disconnecting)
    }

    /// Check if the peer can receive gossip messages.
    pub fn can_receive_gossip(&self) -> bool {
        matches!(
            self,
            PeerState::Connected | PeerState::SyncingHeaders | PeerState::SyncingBlocks | PeerState::Live
        )
    }
}

impl fmt::Display for PeerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerState::Connecting => write!(f, "connecting"),
            PeerState::VersionSent => write!(f, "version_sent"),
            PeerState::Connected => write!(f, "connected"),
            PeerState::SyncingHeaders => write!(f, "syncing_headers"),
            PeerState::SyncingBlocks => write!(f, "syncing_blocks"),
            PeerState::Live => write!(f, "live"),
            PeerState::Disconnecting => write!(f, "disconnecting"),
        }
    }
}

/// Context for managing a peer's interaction state.
#[derive(Debug)]
pub struct PeerContext {
    /// Current state of the peer.
    pub state: PeerState,
    /// Number of headers requested from this peer.
    pub headers_requested: usize,
    /// Number of blocks requested from this peer.
    pub blocks_requested: usize,
    /// Hashes of blocks we've requested from this peer.
    pub pending_blocks: Vec<[u8; 32]>,
    /// Whether we've requested peers from this connection.
    pub peers_requested: bool,
    /// Number of invalid messages received (for banning).
    pub invalid_messages: usize,
    /// Score for peer quality (higher is better).
    pub score: i32,
}

impl PeerContext {
    /// Create a new peer context.
    pub fn new() -> Self {
        Self {
            state: PeerState::Connecting,
            headers_requested: 0,
            blocks_requested: 0,
            pending_blocks: Vec::new(),
            peers_requested: false,
            invalid_messages: 0,
            score: 0,
        }
    }

    /// Transition to a new state.
    pub fn transition_to(&mut self, new_state: PeerState) {
        tracing::debug!(
            from = %self.state,
            to = %new_state,
            "Peer state transition"
        );
        self.state = new_state;
    }

    /// Record a successful interaction (improves score).
    pub fn record_success(&mut self) {
        self.score = self.score.saturating_add(1);
    }

    /// Record a failed interaction (decreases score).
    pub fn record_failure(&mut self) {
        self.score = self.score.saturating_sub(10);
    }

    /// Record an invalid message.
    pub fn record_invalid(&mut self) {
        self.invalid_messages += 1;
        self.record_failure();
    }

    /// Check if the peer should be banned.
    pub fn should_ban(&self) -> bool {
        self.invalid_messages > 3 || self.score < -100
    }

    /// Add a block hash to pending requests.
    pub fn add_pending_block(&mut self, hash: [u8; 32]) {
        self.pending_blocks.push(hash);
        self.blocks_requested += 1;
    }

    /// Remove a block hash from pending requests.
    pub fn remove_pending_block(&mut self, hash: &[u8; 32]) -> bool {
        if let Some(pos) = self.pending_blocks.iter().position(|h| h == hash) {
            self.pending_blocks.remove(pos);
            true
        } else {
            false
        }
    }

    /// Check if we're waiting for a specific block.
    pub fn is_block_pending(&self, hash: &[u8; 32]) -> bool {
        self.pending_blocks.contains(hash)
    }
}

impl Default for PeerContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_state_checks() {
        assert!(!PeerState::Connecting.is_connected());
        assert!(!PeerState::VersionSent.is_connected());
        assert!(PeerState::Connected.is_connected());
        assert!(PeerState::Live.is_connected());

        assert!(!PeerState::Connected.is_syncing());
        assert!(PeerState::SyncingHeaders.is_syncing());
        assert!(PeerState::SyncingBlocks.is_syncing());

        assert!(PeerState::Live.is_live());
        assert!(!PeerState::Connected.is_live());
    }

    #[test]
    fn test_peer_context() {
        let mut ctx = PeerContext::new();
        assert_eq!(ctx.state, PeerState::Connecting);
        assert_eq!(ctx.score, 0);

        ctx.record_success();
        assert_eq!(ctx.score, 1);

        ctx.record_failure();
        assert_eq!(ctx.score, -9);
    }

    #[test]
    fn test_pending_blocks() {
        let mut ctx = PeerContext::new();
        let hash = [1u8; 32];

        assert!(!ctx.is_block_pending(&hash));

        ctx.add_pending_block(hash);
        assert!(ctx.is_block_pending(&hash));
        assert_eq!(ctx.blocks_requested, 1);

        assert!(ctx.remove_pending_block(&hash));
        assert!(!ctx.is_block_pending(&hash));
    }
}
