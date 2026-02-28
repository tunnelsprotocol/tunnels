//! Block synchronization.
//!
//! Implements headers-first synchronization:
//! 1. Build block locator from known chain
//! 2. Request headers from peer
//! 3. Validate received headers form valid chain
//! 4. Download blocks for validated headers
//! 5. Add blocks to chain

pub mod blocks;
pub mod headers;
pub mod state;

use std::time::{Duration, Instant};

use tunnels_chain::ChainState;

use crate::error::P2pResult;
use crate::peer::PeerId;
use crate::protocol::{HeadersMessage, Message};

pub use blocks::{create_block_data, create_get_block, validate_block, BlockDownloader};
pub use headers::{
    build_block_locator, create_get_headers, respond_to_get_headers, validate_headers,
    MAX_HEADERS_PER_REQUEST,
};
pub use state::{SyncProgress, SyncState};

/// Default sync stall timeout.
const DEFAULT_SYNC_TIMEOUT: Duration = Duration::from_secs(30);

/// Sync manager coordinates blockchain synchronization.
pub struct SyncManager {
    /// Sync progress tracking.
    progress: SyncProgress,
    /// Block downloader.
    downloader: BlockDownloader,
    /// Timeout for sync operations.
    timeout: Duration,
    /// Last time we received sync data.
    last_activity: Option<Instant>,
}

impl SyncManager {
    /// Create a new sync manager.
    pub fn new() -> Self {
        Self {
            progress: SyncProgress::new(),
            downloader: BlockDownloader::new(),
            timeout: DEFAULT_SYNC_TIMEOUT,
            last_activity: None,
        }
    }

    /// Create with custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            progress: SyncProgress::new(),
            downloader: BlockDownloader::new(),
            timeout,
            last_activity: None,
        }
    }

    /// Record sync activity (data received from sync peer).
    pub fn record_activity(&mut self) {
        self.last_activity = Some(Instant::now());
    }

    /// Check if sync is stalled (no activity within timeout period).
    pub fn is_stalled(&self) -> bool {
        if !self.is_syncing() {
            return false;
        }
        match self.last_activity {
            Some(last) => last.elapsed() > self.timeout,
            None => false, // Not started yet
        }
    }

    /// Get elapsed time since last activity.
    pub fn time_since_activity(&self) -> Option<Duration> {
        self.last_activity.map(|t| t.elapsed())
    }

    /// Get the sync timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Get the current sync state.
    pub fn state(&self) -> &SyncState {
        &self.progress.state
    }

    /// Check if we're syncing.
    pub fn is_syncing(&self) -> bool {
        self.progress.state.is_syncing()
    }

    /// Check if we're synced.
    pub fn is_synced(&self) -> bool {
        self.progress.state.is_synced()
    }

    /// Get the sync peer if syncing.
    pub fn sync_peer(&self) -> Option<PeerId> {
        self.progress.state.sync_peer()
    }

    /// Get sync progress percentage.
    pub fn progress_percent(&self) -> Option<f64> {
        self.progress.state.progress()
    }

    /// Start syncing headers from a peer.
    pub fn start_sync(&mut self, peer_id: PeerId, our_height: u64, peer_height: u64) {
        tracing::info!(
            peer = %peer_id,
            our_height,
            peer_height,
            "Starting sync"
        );
        self.progress
            .start_header_sync(peer_id, our_height, peer_height);
        self.last_activity = Some(Instant::now());
    }

    /// Create a GetHeaders message for the current sync.
    pub fn create_headers_request(&self, chain: &ChainState) -> Message {
        Message::GetHeaders(create_get_headers(chain, [0u8; 32]))
    }

    /// Process received headers.
    pub fn process_headers(
        &mut self,
        chain: &ChainState,
        headers: &HeadersMessage,
    ) -> P2pResult<()> {
        // Record activity for stall detection
        self.record_activity();

        if headers.headers.is_empty() {
            // No more headers, transition to block sync or complete
            if self.downloader.has_work() {
                if let Some(peer_id) = self.sync_peer() {
                    let (start, target) = match &self.progress.state {
                        SyncState::SyncingHeaders {
                            start_height,
                            target_height,
                            ..
                        } => (*start_height, *target_height),
                        _ => (0, 0),
                    };
                    self.progress.start_block_sync(peer_id, start, target);
                }
            } else {
                self.progress.complete();
            }
            return Ok(());
        }

        // Validate headers
        let hashes = validate_headers(chain, &headers.headers)?;
        self.progress.headers_downloaded += headers.headers.len() as u64;

        // Queue blocks for download
        self.downloader.enqueue(hashes);

        Ok(())
    }

    /// Get the next block requests to send.
    pub fn next_block_requests(&mut self, peer_id: PeerId, max: usize) -> Vec<Message> {
        self.downloader
            .next_requests(peer_id, max)
            .into_iter()
            .map(Message::GetBlock)
            .collect()
    }

    /// Process a received block.
    pub fn process_block(&mut self, hash: &[u8; 32], height: u64) {
        // Record activity for stall detection
        self.record_activity();

        self.downloader.received(hash);
        self.progress.update_block_progress(height);
        self.progress.blocks_downloaded += 1;

        // Check if sync is complete
        if !self.downloader.has_work() {
            self.progress.complete();
        }
    }

    /// Handle sync peer disconnection.
    pub fn peer_disconnected(&mut self, peer_id: &PeerId) {
        if self.sync_peer() == Some(*peer_id) {
            tracing::warn!(peer = %peer_id, "Sync peer disconnected");
            self.downloader.cancel_peer(peer_id);
            self.progress.reset();
        }
    }

    /// Check if we need to sync based on our height and peer heights.
    pub fn needs_sync(&self, our_height: u64, best_peer_height: u64) -> bool {
        !self.is_syncing() && best_peer_height > our_height
    }

    /// Check if there are pending blocks to download.
    pub fn has_pending_blocks(&self) -> bool {
        self.downloader.has_work()
    }

    /// Reset sync state.
    pub fn reset(&mut self) {
        self.progress.reset();
        self.downloader.clear();
        self.last_activity = None;
    }

    /// Get the block downloader.
    pub fn downloader(&self) -> &BlockDownloader {
        &self.downloader
    }

    /// Get mutable block downloader.
    pub fn downloader_mut(&mut self) -> &mut BlockDownloader {
        &mut self.downloader
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_manager() {
        let mut sync = SyncManager::new();

        assert!(!sync.is_syncing());
        assert!(!sync.is_synced());
        assert!(sync.needs_sync(0, 100));

        sync.start_sync(PeerId::new(1), 0, 100);
        assert!(sync.is_syncing());
        assert_eq!(sync.sync_peer(), Some(PeerId::new(1)));
        assert!(!sync.needs_sync(0, 100)); // Already syncing

        sync.peer_disconnected(&PeerId::new(1));
        assert!(!sync.is_syncing());
    }

    #[test]
    fn test_sync_stall_detection() {
        // Use a very short timeout for testing
        let mut sync = SyncManager::with_timeout(Duration::from_millis(10));

        // Not stalled when not syncing
        assert!(!sync.is_stalled());

        // Start sync
        sync.start_sync(PeerId::new(1), 0, 100);
        assert!(!sync.is_stalled()); // Just started, not stalled

        // Record activity
        sync.record_activity();
        assert!(!sync.is_stalled());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));
        assert!(sync.is_stalled()); // Should be stalled now

        // Record new activity
        sync.record_activity();
        assert!(!sync.is_stalled()); // Not stalled after activity

        // Reset clears activity tracking
        sync.reset();
        assert!(!sync.is_stalled()); // Not stalled when not syncing
        assert!(sync.time_since_activity().is_none());
    }
}
