//! Gossip subsystem for transaction and block propagation.
//!
//! This module handles:
//! - Deduplication of received transactions and blocks
//! - Validation before relay
//! - Broadcasting to connected peers
//! - Orphan block management

pub mod block;
pub mod filter;
pub mod tx;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tunnels_chain::{ChainState, Mempool};
use tunnels_core::block::Block;
use tunnels_core::transaction::SignedTransaction;

use crate::error::P2pResult;
use crate::protocol::Message;

pub use block::BlockRelay;
pub use filter::SeenFilter;
pub use tx::TxRelay;

/// Maximum number of orphan blocks to keep.
const MAX_ORPHAN_BLOCKS: usize = 100;

/// Maximum age for orphan blocks before they're discarded.
const ORPHAN_EXPIRY_SECS: u64 = 300; // 5 minutes

/// An orphan block waiting for its parent.
struct OrphanBlock {
    /// The block itself.
    block: Block,
    /// When this orphan was received.
    received_at: Instant,
}

/// Gossip manager coordinates all gossip operations.
pub struct GossipManager {
    /// Transaction relay.
    tx_relay: TxRelay,
    /// Block relay.
    block_relay: BlockRelay,
    /// Orphan blocks indexed by parent hash.
    orphans: HashMap<[u8; 32], Vec<OrphanBlock>>,
}

impl GossipManager {
    /// Create a new gossip manager.
    pub fn new() -> Self {
        Self {
            tx_relay: TxRelay::new(),
            block_relay: BlockRelay::new(),
            orphans: HashMap::new(),
        }
    }

    /// Process a received transaction.
    ///
    /// Returns Some(tx_id, message) if the transaction should be relayed to other peers.
    pub async fn process_transaction(
        &mut self,
        tx: SignedTransaction,
        mempool: &Arc<RwLock<Mempool>>,
        chain: &Arc<RwLock<ChainState>>,
    ) -> P2pResult<Option<([u8; 32], Message)>> {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let tx_clone = tx.clone();

        match self
            .tx_relay
            .process_transaction(tx, mempool, chain, current_time)
            .await
        {
            Ok(Some(tx_id)) => {
                let message = TxRelay::create_message(tx_clone);
                Ok(Some((tx_id, message)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Process a received block.
    ///
    /// Returns Some(hash, message) if the block should be relayed to other peers.
    pub async fn process_block(
        &mut self,
        block: Block,
        chain: &Arc<RwLock<ChainState>>,
    ) -> P2pResult<Option<([u8; 32], Message)>> {
        let block_clone = block.clone();

        match self.block_relay.process_block(block, chain).await {
            Ok(Some(hash)) => {
                let message = BlockRelay::create_message(block_clone);
                Ok(Some((hash, message)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Check if a transaction has been seen.
    pub fn has_seen_tx(&self, tx_id: &[u8; 32]) -> bool {
        self.tx_relay.has_seen(tx_id)
    }

    /// Check if a block has been seen.
    pub fn has_seen_block(&self, hash: &[u8; 32]) -> bool {
        self.block_relay.has_seen(hash)
    }

    /// Mark a transaction as seen.
    pub fn mark_tx_seen(&mut self, tx_id: [u8; 32]) {
        self.tx_relay.mark_seen(tx_id);
    }

    /// Mark a block as seen.
    pub fn mark_block_seen(&mut self, hash: [u8; 32]) {
        self.block_relay.mark_seen(hash);
    }

    /// Create a message to broadcast a new transaction.
    pub fn create_tx_message(&mut self, tx: SignedTransaction) -> Message {
        let tx_id = tx.id();
        self.tx_relay.mark_seen(tx_id);
        TxRelay::create_message(tx)
    }

    /// Create a message to broadcast a new block.
    pub fn create_block_message(&mut self, block: Block) -> Message {
        let hash = block.hash();
        self.block_relay.mark_seen(hash);
        BlockRelay::create_message(block)
    }

    /// Queue an orphan block whose parent we don't have yet.
    pub fn queue_orphan(&mut self, block: Block) {
        // First, clean up expired orphans
        self.clean_expired_orphans();

        // Don't store if we already have too many orphans
        let total_orphans: usize = self.orphans.values().map(|v| v.len()).sum();
        if total_orphans >= MAX_ORPHAN_BLOCKS {
            tracing::debug!("Orphan pool full, discarding block");
            return;
        }

        let parent_hash = block.header.prev_block_hash;
        let block_hash = block.hash();

        // Check if we already have this orphan
        if let Some(orphans) = self.orphans.get(&parent_hash) {
            for orphan in orphans {
                if orphan.block.hash() == block_hash {
                    return; // Already have it
                }
            }
        }

        // Mark as seen so we don't re-request it
        self.block_relay.mark_seen(block_hash);

        let orphan = OrphanBlock {
            block,
            received_at: Instant::now(),
        };

        self.orphans.entry(parent_hash).or_default().push(orphan);

        tracing::debug!(
            hash = ?&block_hash[..8],
            parent = ?&parent_hash[..8],
            total_orphans = total_orphans + 1,
            "Queued orphan block"
        );
    }

    /// Get an orphan block whose parent now exists in the chain.
    ///
    /// Returns the block if found, removing it from the orphan pool.
    pub fn get_processable_orphan(&mut self, chain: &ChainState) -> Option<Block> {
        // Find a parent hash that exists in the chain
        let processable_parent = self
            .orphans
            .keys()
            .find(|parent_hash| chain.has_block(parent_hash))
            .copied();

        if let Some(parent_hash) = processable_parent {
            if let Some(orphans) = self.orphans.get_mut(&parent_hash) {
                if let Some(orphan) = orphans.pop() {
                    // If that was the last orphan for this parent, remove the entry
                    if orphans.is_empty() {
                        self.orphans.remove(&parent_hash);
                    }
                    return Some(orphan.block);
                }
            }
        }

        None
    }

    /// Clean up expired orphan blocks.
    fn clean_expired_orphans(&mut self) {
        let now = Instant::now();
        let expiry = std::time::Duration::from_secs(ORPHAN_EXPIRY_SECS);

        self.orphans.retain(|_, orphans| {
            orphans.retain(|o| now.duration_since(o.received_at) < expiry);
            !orphans.is_empty()
        });
    }

    /// Get the number of queued orphan blocks.
    pub fn orphan_count(&self) -> usize {
        self.orphans.values().map(|v| v.len()).sum()
    }

    /// Get statistics about seen items.
    pub fn stats(&self) -> GossipStats {
        GossipStats {
            seen_transactions: self.tx_relay.seen_count(),
            seen_blocks: self.block_relay.seen_count(),
        }
    }

    /// Clear all seen filters and orphan pool.
    pub fn clear(&mut self) {
        self.tx_relay.clear();
        self.block_relay.clear();
        self.orphans.clear();
    }
}

impl Default for GossipManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about gossip activity.
#[derive(Debug, Clone)]
pub struct GossipStats {
    /// Number of seen transactions.
    pub seen_transactions: usize,
    /// Number of seen blocks.
    pub seen_blocks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_manager() {
        let mut gossip = GossipManager::new();

        // Test marking items as seen
        let tx_id = [1u8; 32];
        let block_hash = [2u8; 32];

        assert!(!gossip.has_seen_tx(&tx_id));
        assert!(!gossip.has_seen_block(&block_hash));

        gossip.mark_tx_seen(tx_id);
        gossip.mark_block_seen(block_hash);

        assert!(gossip.has_seen_tx(&tx_id));
        assert!(gossip.has_seen_block(&block_hash));

        let stats = gossip.stats();
        assert_eq!(stats.seen_transactions, 1);
        assert_eq!(stats.seen_blocks, 1);
    }
}
