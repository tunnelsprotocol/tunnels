//! Block download coordination.

use std::collections::VecDeque;
use std::time::Instant;

use tunnels_chain::ChainState;
use tunnels_core::block::Block;

use crate::error::{P2pError, P2pResult};
use crate::peer::PeerId;
use crate::protocol::{BlockDataMessage, GetBlockMessage};

/// Maximum blocks to have in flight at once.
pub const MAX_BLOCKS_IN_FLIGHT: usize = 16;

/// Block download request.
#[derive(Debug, Clone)]
pub struct BlockRequest {
    /// Hash of the block.
    pub hash: [u8; 32],
    /// Height of the block (if known).
    pub height: Option<u64>,
    /// Peer we requested from.
    pub peer_id: PeerId,
    /// When the request was sent.
    pub requested_at: Instant,
}

/// Coordinates block downloads.
#[derive(Debug)]
pub struct BlockDownloader {
    /// Queue of hashes to download.
    queue: VecDeque<[u8; 32]>,
    /// Currently in-flight requests.
    in_flight: Vec<BlockRequest>,
    /// Maximum in-flight requests.
    max_in_flight: usize,
}

impl BlockDownloader {
    /// Create a new block downloader.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            in_flight: Vec::new(),
            max_in_flight: MAX_BLOCKS_IN_FLIGHT,
        }
    }

    /// Add blocks to the download queue.
    pub fn enqueue(&mut self, hashes: Vec<[u8; 32]>) {
        for hash in hashes {
            if !self.is_queued(&hash) && !self.is_in_flight(&hash) {
                self.queue.push_back(hash);
            }
        }
    }

    /// Check if a block is queued for download.
    pub fn is_queued(&self, hash: &[u8; 32]) -> bool {
        self.queue.contains(hash)
    }

    /// Check if a block is currently being downloaded.
    pub fn is_in_flight(&self, hash: &[u8; 32]) -> bool {
        self.in_flight.iter().any(|r| &r.hash == hash)
    }

    /// Get the next blocks to request.
    pub fn next_requests(&mut self, peer_id: PeerId, max: usize) -> Vec<GetBlockMessage> {
        let available = self.max_in_flight.saturating_sub(self.in_flight.len());
        let count = max.min(available).min(self.queue.len());

        let mut requests = Vec::with_capacity(count);
        let now = Instant::now();

        for _ in 0..count {
            if let Some(hash) = self.queue.pop_front() {
                self.in_flight.push(BlockRequest {
                    hash,
                    height: None,
                    peer_id,
                    requested_at: now,
                });
                requests.push(GetBlockMessage { hash });
            }
        }

        requests
    }

    /// Mark a block as received.
    pub fn received(&mut self, hash: &[u8; 32]) -> bool {
        if let Some(pos) = self.in_flight.iter().position(|r| &r.hash == hash) {
            self.in_flight.remove(pos);
            true
        } else {
            false
        }
    }

    /// Cancel requests from a specific peer.
    pub fn cancel_peer(&mut self, peer_id: &PeerId) {
        // Move in-flight requests back to queue
        let (from_peer, others): (Vec<_>, Vec<_>) = self
            .in_flight
            .drain(..)
            .partition(|r| &r.peer_id == peer_id);

        self.in_flight = others;

        for request in from_peer {
            self.queue.push_front(request.hash);
        }
    }

    /// Get number of queued blocks.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Get number of in-flight requests.
    pub fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }

    /// Check if there's more work to do.
    pub fn has_work(&self) -> bool {
        !self.queue.is_empty() || !self.in_flight.is_empty()
    }

    /// Check if we can make more requests.
    pub fn can_request(&self) -> bool {
        self.in_flight.len() < self.max_in_flight && !self.queue.is_empty()
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.in_flight.clear();
    }

    /// Get timed-out requests (older than specified duration).
    pub fn get_timeouts(&self, timeout: std::time::Duration) -> Vec<BlockRequest> {
        let now = Instant::now();
        self.in_flight
            .iter()
            .filter(|r| now.duration_since(r.requested_at) > timeout)
            .cloned()
            .collect()
    }
}

impl Default for BlockDownloader {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a received block.
pub fn validate_block(chain: &ChainState, block: &Block) -> P2pResult<()> {
    // Check if we already have this block
    let hash = block.hash();
    if chain.has_block(&hash) {
        return Ok(()); // Already have it, no error but don't re-add
    }

    // Check that parent exists
    if !chain.has_block(&block.header.prev_block_hash) {
        return Err(P2pError::SyncError(format!(
            "Block {} has unknown parent {:?}",
            block.header.height,
            &block.header.prev_block_hash[..8]
        )));
    }

    // Check proof of work
    if !block.header.meets_difficulty() {
        return Err(P2pError::BlockValidationFailed(format!(
            "Block {} doesn't meet difficulty",
            block.header.height
        )));
    }

    // Check transaction root
    if !block.verify_tx_root() {
        return Err(P2pError::BlockValidationFailed(format!(
            "Block {} has invalid transaction root",
            block.header.height
        )));
    }

    Ok(())
}

/// Create a GetBlock message.
pub fn create_get_block(hash: [u8; 32]) -> GetBlockMessage {
    GetBlockMessage { hash }
}

/// Create a BlockData response.
pub fn create_block_data(block: Block) -> BlockDataMessage {
    BlockDataMessage { block }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_downloader() {
        let mut downloader = BlockDownloader::new();

        // Enqueue some blocks
        let hashes = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        downloader.enqueue(hashes.clone());

        assert_eq!(downloader.queue_len(), 3);
        assert!(downloader.is_queued(&[1u8; 32]));

        // Request blocks
        let requests = downloader.next_requests(PeerId::new(1), 2);
        assert_eq!(requests.len(), 2);
        assert_eq!(downloader.queue_len(), 1);
        assert_eq!(downloader.in_flight_len(), 2);

        // Receive a block
        assert!(downloader.received(&[1u8; 32]));
        assert_eq!(downloader.in_flight_len(), 1);

        // Duplicate enqueue should be ignored
        downloader.enqueue(vec![[2u8; 32]]); // Already in-flight
        assert_eq!(downloader.queue_len(), 1);
    }

    #[test]
    fn test_cancel_peer() {
        let mut downloader = BlockDownloader::new();
        downloader.enqueue(vec![[1u8; 32], [2u8; 32]]);

        // Request from peer 1
        let _ = downloader.next_requests(PeerId::new(1), 2);
        assert_eq!(downloader.in_flight_len(), 2);

        // Cancel peer 1
        downloader.cancel_peer(&PeerId::new(1));
        assert_eq!(downloader.in_flight_len(), 0);
        assert_eq!(downloader.queue_len(), 2);
    }
}
