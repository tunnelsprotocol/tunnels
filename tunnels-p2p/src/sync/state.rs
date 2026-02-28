//! Sync state tracking.

use std::collections::HashSet;
use std::time::Instant;

use crate::peer::PeerId;

/// Current sync state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SyncState {
    /// Not syncing, waiting to determine if sync is needed.
    #[default]
    Idle,
    /// Syncing headers from peers.
    SyncingHeaders {
        /// Peer we're syncing from.
        peer_id: PeerId,
        /// Height we started at.
        start_height: u64,
        /// Target height (peer's best height).
        target_height: u64,
    },
    /// Syncing blocks after headers.
    SyncingBlocks {
        /// Peer we're syncing from.
        peer_id: PeerId,
        /// Height we started at.
        start_height: u64,
        /// Target height.
        target_height: u64,
        /// Current height being downloaded.
        current_height: u64,
    },
    /// Fully synced.
    Synced,
}

impl SyncState {
    /// Check if we're currently syncing.
    pub fn is_syncing(&self) -> bool {
        matches!(
            self,
            SyncState::SyncingHeaders { .. } | SyncState::SyncingBlocks { .. }
        )
    }

    /// Check if we're synced.
    pub fn is_synced(&self) -> bool {
        matches!(self, SyncState::Synced)
    }

    /// Get the sync peer if syncing.
    pub fn sync_peer(&self) -> Option<PeerId> {
        match self {
            SyncState::SyncingHeaders { peer_id, .. } => Some(*peer_id),
            SyncState::SyncingBlocks { peer_id, .. } => Some(*peer_id),
            _ => None,
        }
    }

    /// Get sync progress as a percentage.
    pub fn progress(&self) -> Option<f64> {
        match self {
            SyncState::SyncingBlocks {
                start_height,
                target_height,
                current_height,
                ..
            } => {
                if target_height > start_height {
                    let total = target_height - start_height;
                    let done = current_height.saturating_sub(*start_height);
                    Some((done as f64 / total as f64) * 100.0)
                } else {
                    Some(100.0)
                }
            }
            SyncState::Synced => Some(100.0),
            _ => None,
        }
    }
}

/// Tracks sync progress and coordinates block downloads.
#[derive(Debug)]
pub struct SyncProgress {
    /// Current sync state.
    pub state: SyncState,
    /// Headers we've received but not yet downloaded blocks for.
    pub pending_headers: Vec<[u8; 32]>,
    /// Block hashes we've requested but not yet received.
    pub in_flight_blocks: HashSet<[u8; 32]>,
    /// When the current sync operation started.
    pub started_at: Option<Instant>,
    /// Number of blocks downloaded in current sync.
    pub blocks_downloaded: u64,
    /// Number of headers downloaded in current sync.
    pub headers_downloaded: u64,
}

impl SyncProgress {
    /// Create new sync progress tracker.
    pub fn new() -> Self {
        Self {
            state: SyncState::Idle,
            pending_headers: Vec::new(),
            in_flight_blocks: HashSet::new(),
            started_at: None,
            blocks_downloaded: 0,
            headers_downloaded: 0,
        }
    }

    /// Start syncing headers from a peer.
    pub fn start_header_sync(&mut self, peer_id: PeerId, start_height: u64, target_height: u64) {
        self.state = SyncState::SyncingHeaders {
            peer_id,
            start_height,
            target_height,
        };
        self.started_at = Some(Instant::now());
        self.headers_downloaded = 0;
        self.blocks_downloaded = 0;
        self.pending_headers.clear();
        self.in_flight_blocks.clear();
    }

    /// Transition to block sync after headers are received.
    pub fn start_block_sync(&mut self, peer_id: PeerId, start_height: u64, target_height: u64) {
        self.state = SyncState::SyncingBlocks {
            peer_id,
            start_height,
            target_height,
            current_height: start_height,
        };
    }

    /// Update block sync progress.
    pub fn update_block_progress(&mut self, height: u64) {
        if let SyncState::SyncingBlocks {
            current_height, ..
        } = &mut self.state
        {
            *current_height = height;
        }
        self.blocks_downloaded += 1;
    }

    /// Mark sync as complete.
    pub fn complete(&mut self) {
        self.state = SyncState::Synced;
        self.pending_headers.clear();
        self.in_flight_blocks.clear();
    }

    /// Reset to idle state (e.g., when sync peer disconnects).
    pub fn reset(&mut self) {
        self.state = SyncState::Idle;
        self.pending_headers.clear();
        self.in_flight_blocks.clear();
        self.started_at = None;
    }

    /// Add a block to in-flight requests.
    pub fn request_block(&mut self, hash: [u8; 32]) {
        self.in_flight_blocks.insert(hash);
    }

    /// Remove a block from in-flight requests.
    pub fn received_block(&mut self, hash: &[u8; 32]) -> bool {
        self.in_flight_blocks.remove(hash)
    }

    /// Get the number of in-flight block requests.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight_blocks.len()
    }

    /// Check if a block is being requested.
    pub fn is_block_in_flight(&self, hash: &[u8; 32]) -> bool {
        self.in_flight_blocks.contains(hash)
    }

    /// Get elapsed time since sync started.
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        self.started_at.map(|t| t.elapsed())
    }
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_state() {
        let state = SyncState::Idle;
        assert!(!state.is_syncing());
        assert!(!state.is_synced());

        let state = SyncState::SyncingHeaders {
            peer_id: PeerId::new(1),
            start_height: 0,
            target_height: 100,
        };
        assert!(state.is_syncing());
        assert_eq!(state.sync_peer(), Some(PeerId::new(1)));
    }

    #[test]
    fn test_sync_progress() {
        let state = SyncState::SyncingBlocks {
            peer_id: PeerId::new(1),
            start_height: 0,
            target_height: 100,
            current_height: 50,
        };

        assert_eq!(state.progress(), Some(50.0));
    }

    #[test]
    fn test_sync_tracker() {
        let mut tracker = SyncProgress::new();

        tracker.start_header_sync(PeerId::new(1), 0, 100);
        assert!(tracker.state.is_syncing());

        tracker.start_block_sync(PeerId::new(1), 0, 100);
        tracker.update_block_progress(50);

        if let SyncState::SyncingBlocks { current_height, .. } = tracker.state {
            assert_eq!(current_height, 50);
        } else {
            panic!("Expected SyncingBlocks state");
        }

        tracker.complete();
        assert!(tracker.state.is_synced());
    }
}
