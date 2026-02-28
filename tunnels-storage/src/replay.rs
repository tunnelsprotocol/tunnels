//! Chain replay from genesis.
//!
//! This module enables replaying the blockchain from genesis to rebuild
//! state and verify state roots at each height.

use std::sync::Arc;

use tunnels_core::Block;

use crate::block::BlockStore;
use crate::error::StorageError;
use crate::kv::KvBackend;
use crate::state::PersistentState;
use crate::trie::EMPTY_ROOT;

/// Chain replay utility.
///
/// Replays blocks from genesis to rebuild state and optionally verify
/// that computed state roots match stored state roots.
pub struct ChainReplay<B: KvBackend> {
    block_store: BlockStore<B>,
    state_backend: Arc<B>,
}

impl<B: KvBackend> ChainReplay<B> {
    /// Create a new chain replay utility.
    ///
    /// # Arguments
    /// * `block_store_backend` - Backend for block storage
    /// * `state_backend` - Backend for state storage (can be separate or same)
    pub fn new(block_store_backend: Arc<B>, state_backend: Arc<B>) -> Self {
        Self {
            block_store: BlockStore::new(block_store_backend),
            state_backend,
        }
    }

    /// Create a chain replay using a single backend for both blocks and state.
    pub fn with_shared_backend(backend: Arc<B>) -> Self {
        Self {
            block_store: BlockStore::new(Arc::clone(&backend)),
            state_backend: backend,
        }
    }

    /// Get the block store.
    pub fn block_store(&self) -> &BlockStore<B> {
        &self.block_store
    }

    /// Replay all blocks from genesis to latest.
    ///
    /// Returns the final state root after replay.
    pub fn replay_all<F>(&self, apply_block: F) -> Result<ReplayResult, StorageError>
    where
        F: Fn(&mut PersistentState<B>, &Block) -> Result<(), StorageError>,
    {
        let latest = self.block_store.get_latest_height()?;
        match latest {
            Some(height) => self.replay_range(0, height, apply_block),
            None => Ok(ReplayResult {
                blocks_replayed: 0,
                final_root: EMPTY_ROOT,
                root_mismatches: vec![],
            }),
        }
    }

    /// Replay blocks in a specific range (inclusive).
    ///
    /// For replay to work correctly, you must start from genesis (height 0)
    /// or provide a pre-populated state at start_height.
    pub fn replay_range<F>(
        &self,
        start_height: u64,
        end_height: u64,
        apply_block: F,
    ) -> Result<ReplayResult, StorageError>
    where
        F: Fn(&mut PersistentState<B>, &Block) -> Result<(), StorageError>,
    {
        let mut state = if start_height == 0 {
            PersistentState::new(Arc::clone(&self.state_backend))
        } else {
            // Load state from previous height
            let prev_root = self.block_store
                .get_state_root(start_height - 1)?
                .ok_or_else(|| StorageError::Block(
                    format!("Missing state root at height {}", start_height - 1)
                ))?;
            PersistentState::from_root(Arc::clone(&self.state_backend), prev_root)?
        };

        let mut blocks_replayed = 0;
        let mut root_mismatches = Vec::new();

        for height in start_height..=end_height {
            let block = self.block_store
                .get_block_by_height(height)?
                .ok_or_else(|| StorageError::Block(format!("Missing block at height {}", height)))?;

            // Apply the block
            apply_block(&mut state, &block)?;

            // Commit and get computed root
            let computed_root = state.commit()?;

            // Compare with stored root
            if let Some(stored_root) = self.block_store.get_state_root(height)? {
                if computed_root != stored_root {
                    root_mismatches.push(RootMismatch {
                        height,
                        expected: stored_root,
                        computed: computed_root,
                    });
                }
            }

            blocks_replayed += 1;
        }

        Ok(ReplayResult {
            blocks_replayed,
            final_root: state.state_root(),
            root_mismatches,
        })
    }

    /// Verify state roots match by replaying blocks.
    ///
    /// Returns true if all computed roots match stored roots.
    pub fn verify_roots<F>(&self, apply_block: F) -> Result<bool, StorageError>
    where
        F: Fn(&mut PersistentState<B>, &Block) -> Result<(), StorageError>,
    {
        let result = self.replay_all(apply_block)?;
        Ok(result.root_mismatches.is_empty())
    }

    /// Replay from genesis and return the resulting state.
    ///
    /// Useful for rebuilding state after corruption or for testing.
    pub fn rebuild_state<F>(&self, apply_block: F) -> Result<PersistentState<B>, StorageError>
    where
        F: Fn(&mut PersistentState<B>, &Block) -> Result<(), StorageError>,
    {
        let mut state = PersistentState::new(Arc::clone(&self.state_backend));

        let latest = self.block_store.get_latest_height()?;
        if let Some(end_height) = latest {
            for height in 0..=end_height {
                let block = self.block_store
                    .get_block_by_height(height)?
                    .ok_or_else(|| StorageError::Block(format!("Missing block at height {}", height)))?;

                apply_block(&mut state, &block)?;
                state.commit()?;
            }
        }

        Ok(state)
    }
}

/// Result of a chain replay operation.
#[derive(Debug)]
pub struct ReplayResult {
    /// Number of blocks successfully replayed.
    pub blocks_replayed: u64,
    /// Final state root after replay.
    pub final_root: [u8; 32],
    /// List of height where computed root didn't match stored root.
    pub root_mismatches: Vec<RootMismatch>,
}

impl ReplayResult {
    /// Check if all roots matched.
    pub fn all_roots_matched(&self) -> bool {
        self.root_mismatches.is_empty()
    }
}

/// A mismatch between computed and stored state root.
#[derive(Debug, Clone)]
pub struct RootMismatch {
    /// Block height where mismatch occurred.
    pub height: u64,
    /// Expected (stored) root.
    pub expected: [u8; 32],
    /// Computed root.
    pub computed: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::MemoryBackend;
    use tunnels_core::{Block, BlockHeader, Identity, KeyPair};
    use tunnels_state::StateWriter;

    fn create_test_block(height: u64, prev_hash: [u8; 32]) -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                height,
                timestamp: 1700000000 + height,
                prev_block_hash: prev_hash,
                state_root: [0u8; 32], // Will be updated
                tx_root: [0u8; 32],
                difficulty: 1,
                nonce: 0,
            },
            transactions: vec![],
        }
    }

    // Shared identities for tests - must be reused to ensure deterministic state
    fn get_test_identities() -> Vec<Identity> {
        use std::sync::OnceLock;
        static IDENTITIES: OnceLock<Vec<Identity>> = OnceLock::new();
        IDENTITIES.get_or_init(|| {
            (0..10).map(|seed| {
                let kp = KeyPair::generate();
                let mut address = [0u8; 20];
                address[0] = seed;
                Identity::new_human(address, kp.public_key())
            }).collect()
        }).clone()
    }

    #[test]
    fn test_replay_empty_chain() {
        let backend = Arc::new(MemoryBackend::new());
        let replay = ChainReplay::with_shared_backend(backend);

        let result = replay.replay_all(|_, _| Ok(())).unwrap();
        assert_eq!(result.blocks_replayed, 0);
        assert_eq!(result.final_root, EMPTY_ROOT);
        assert!(result.all_roots_matched());
    }

    #[test]
    fn test_replay_single_block() {
        let backend = Arc::new(MemoryBackend::new());
        let replay = ChainReplay::with_shared_backend(Arc::clone(&backend));

        // Store genesis with a state
        let genesis = create_test_block(0, [0u8; 32]);
        let identities = get_test_identities();
        let identity1 = identities[1].clone();

        // Create state and add identity
        let mut state = PersistentState::new(Arc::clone(&backend));
        state.insert_identity(identity1.clone());
        let root = state.commit().unwrap();

        replay.block_store().store_block(&genesis, root).unwrap();

        // Replay using the same identity
        let result = replay.replay_all(move |state, _| {
            state.insert_identity(identity1.clone());
            Ok(())
        }).unwrap();

        assert_eq!(result.blocks_replayed, 1);
        assert!(result.all_roots_matched());
    }

    #[test]
    fn test_replay_detects_mismatch() {
        let backend = Arc::new(MemoryBackend::new());
        let replay = ChainReplay::with_shared_backend(Arc::clone(&backend));
        let identities = get_test_identities();

        // Store genesis with one state root
        let genesis = create_test_block(0, [0u8; 32]);
        let fake_root = [0xAB; 32];
        replay.block_store().store_block(&genesis, fake_root).unwrap();

        // Replay with different state modifications
        let identity = identities[9].clone();
        let result = replay.replay_all(move |state, _| {
            state.insert_identity(identity.clone());
            Ok(())
        }).unwrap();

        assert_eq!(result.blocks_replayed, 1);
        assert!(!result.all_roots_matched());
        assert_eq!(result.root_mismatches.len(), 1);
        assert_eq!(result.root_mismatches[0].height, 0);
        assert_eq!(result.root_mismatches[0].expected, fake_root);
    }

    #[test]
    fn test_rebuild_state() {
        let backend = Arc::new(MemoryBackend::new());
        let replay = ChainReplay::with_shared_backend(Arc::clone(&backend));
        let identities = get_test_identities();

        // Build a chain
        let genesis = create_test_block(0, [0u8; 32]);
        let mut state = PersistentState::new(Arc::clone(&backend));
        state.insert_identity(identities[1].clone());
        let root0 = state.commit().unwrap();
        replay.block_store().store_block(&genesis, root0).unwrap();

        let block1 = create_test_block(1, genesis.hash());
        state.insert_identity(identities[2].clone());
        let root1 = state.commit().unwrap();
        replay.block_store().store_block(&block1, root1).unwrap();

        // Create new backend and rebuild
        let fresh_backend = Arc::new(MemoryBackend::new());
        let fresh_replay = ChainReplay::new(
            Arc::clone(&backend), // blocks from original
            fresh_backend,         // fresh state backend
        );

        let identities_for_rebuild = identities.clone();
        let rebuilt_state = fresh_replay.rebuild_state(move |state, block| {
            let id_seed = (block.height() + 1) as usize;
            state.insert_identity(identities_for_rebuild[id_seed].clone());
            Ok(())
        }).unwrap();

        assert_eq!(rebuilt_state.state_root(), root1);
        assert_eq!(rebuilt_state.identity_count(), 2);
    }

    #[test]
    fn test_verify_roots() {
        let backend = Arc::new(MemoryBackend::new());
        let replay = ChainReplay::with_shared_backend(Arc::clone(&backend));
        let identities = get_test_identities();

        // Store chain with correct roots
        let genesis = create_test_block(0, [0u8; 32]);
        let mut state = PersistentState::new(Arc::clone(&backend));
        state.insert_identity(identities[1].clone());
        let root = state.commit().unwrap();
        replay.block_store().store_block(&genesis, root).unwrap();

        // Verify with matching replay
        let identity1 = identities[1].clone();
        let verified = replay.verify_roots(move |state, _| {
            state.insert_identity(identity1.clone());
            Ok(())
        }).unwrap();

        assert!(verified);

        // Verify with non-matching replay
        let identity2 = identities[2].clone();
        let not_verified = replay.verify_roots(move |state, _| {
            state.insert_identity(identity2.clone()); // Different!
            Ok(())
        }).unwrap();

        assert!(!not_verified);
    }
}
