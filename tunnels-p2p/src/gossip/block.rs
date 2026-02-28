//! Block gossip.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tunnels_chain::ChainState;
use tunnels_core::block::Block;

use crate::error::{P2pError, P2pResult};
use crate::gossip::SeenFilter;
use crate::protocol::{Message, NewBlockMessage};

/// Block relay manager.
pub struct BlockRelay {
    /// Filter for already-seen blocks.
    seen: SeenFilter,
}

impl BlockRelay {
    /// Create a new block relay manager.
    pub fn new() -> Self {
        Self {
            seen: SeenFilter::with_default_capacity(),
        }
    }

    /// Create a new block relay with custom capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: SeenFilter::new(capacity),
        }
    }

    /// Check if a block should be relayed (not seen before).
    pub fn should_relay(&mut self, block_hash: &[u8; 32]) -> bool {
        self.seen.check(block_hash)
    }

    /// Mark a block as seen without checking.
    pub fn mark_seen(&mut self, block_hash: [u8; 32]) {
        self.seen.mark_seen(block_hash);
    }

    /// Check if we've seen a block.
    pub fn has_seen(&self, block_hash: &[u8; 32]) -> bool {
        self.seen.contains(block_hash)
    }

    /// Process a received block.
    ///
    /// Returns Ok(Some(hash)) if the block is new and valid.
    /// Returns Ok(None) if the block was already seen or invalid.
    pub async fn process_block(
        &mut self,
        block: Block,
        chain: &Arc<RwLock<ChainState>>,
    ) -> P2pResult<Option<[u8; 32]>> {
        let block_hash = block.hash();
        let height = block.header.height;

        // Check if already seen
        if !self.should_relay(&block_hash) {
            return Ok(None);
        }

        // Get current time
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Try to add to chain
        let mut chain_guard = chain.write().await;

        // Check if we already have this block
        if chain_guard.has_block(&block_hash) {
            return Ok(None);
        }

        // Check if parent exists
        if !chain_guard.has_block(&block.header.prev_block_hash) {
            tracing::debug!(
                hash = ?&block_hash[..8],
                parent = ?&block.header.prev_block_hash[..8],
                "Block has unknown parent, might need sync"
            );
            // Don't error - this might trigger a sync
            return Ok(None);
        }

        match chain_guard.add_block(block, current_time) {
            Ok(()) => {
                tracing::info!(
                    hash = ?&block_hash[..8],
                    height,
                    "Added new block to chain"
                );
                Ok(Some(block_hash))
            }
            Err(e) => {
                tracing::warn!(
                    hash = ?&block_hash[..8],
                    height,
                    error = %e,
                    "Block rejected"
                );
                Err(P2pError::BlockValidationFailed(e.to_string()))
            }
        }
    }

    /// Create a NewBlock message for broadcasting.
    pub fn create_message(block: Block) -> Message {
        Message::NewBlock(NewBlockMessage { block })
    }

    /// Get the number of seen blocks.
    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }

    /// Clear the seen filter.
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

impl Default for BlockRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_relay_dedup() {
        let mut relay = BlockRelay::new();
        let hash = [1u8; 32];

        // First time should allow relay
        assert!(relay.should_relay(&hash));

        // Second time should not
        assert!(!relay.should_relay(&hash));
    }

    #[test]
    fn test_mark_seen() {
        let mut relay = BlockRelay::new();
        let hash = [1u8; 32];

        relay.mark_seen(hash);
        assert!(relay.has_seen(&hash));
        assert!(!relay.should_relay(&hash));
    }
}
