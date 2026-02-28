//! Chain state management.

use std::collections::{HashMap, VecDeque};

use tunnels_core::block::Block;
use tunnels_core::U256;
use tunnels_state::{ProtocolState, StateWriter};

use crate::error::{ChainError, ChainResult};
use crate::genesis::create_genesis_block;
use crate::validation::{validate_block_with_state_devnet, MEDIAN_TIME_PAST_BLOCKS};

use super::reorg::find_fork_point;
use super::stored_block::{work_for_difficulty, StoredBlock};

/// Maximum number of states to cache for shallow reorgs.
const STATE_CACHE_DEPTH: usize = 6;

/// Chain state tracking all blocks and the current best chain.
pub struct ChainState {
    /// Hash of the current best chain tip.
    tip: [u8; 32],

    /// Current protocol state at the tip.
    state: ProtocolState,

    /// All known blocks indexed by hash.
    blocks_by_hash: HashMap<[u8; 32], StoredBlock>,

    /// Block hashes at each height (supports forks).
    blocks_by_height: HashMap<u64, Vec<[u8; 32]>>,

    /// Cached states for recent blocks (for reorgs).
    /// Maps block hash to state after that block was applied.
    state_cache: HashMap<[u8; 32], ProtocolState>,

    /// Recent timestamps for median-time-past calculation.
    /// Contains timestamps of last MEDIAN_TIME_PAST_BLOCKS blocks on main chain.
    recent_timestamps: VecDeque<u64>,

    /// Genesis block hash.
    genesis_hash: [u8; 32],

    /// Whether devnet mode is enabled (allows shorter verification windows).
    devnet: bool,
}

impl ChainState {
    /// Create a new chain state initialized with the genesis block.
    pub fn new() -> Self {
        Self::new_with_devnet(false)
    }

    /// Create a new chain state with devnet mode.
    pub fn devnet() -> Self {
        Self::new_with_devnet(true)
    }

    /// Create a new chain state with optional devnet mode.
    fn new_with_devnet(devnet: bool) -> Self {
        let genesis = create_genesis_block();
        let genesis_hash = genesis.hash();
        let initial_state = ProtocolState::new();

        let genesis_work = work_for_difficulty(genesis.header.difficulty);
        let stored_genesis = StoredBlock::new(genesis.clone(), genesis_work, true, 0);

        let mut blocks_by_hash = HashMap::new();
        blocks_by_hash.insert(genesis_hash, stored_genesis);

        let mut blocks_by_height = HashMap::new();
        blocks_by_height.insert(0, vec![genesis_hash]);

        let mut state_cache = HashMap::new();
        state_cache.insert(genesis_hash, initial_state.clone());

        let mut recent_timestamps = VecDeque::with_capacity(MEDIAN_TIME_PAST_BLOCKS);
        recent_timestamps.push_back(genesis.header.timestamp);

        Self {
            tip: genesis_hash,
            state: initial_state,
            blocks_by_hash,
            blocks_by_height,
            state_cache,
            recent_timestamps,
            genesis_hash,
            devnet,
        }
    }

    /// Check if devnet mode is enabled.
    pub fn is_devnet(&self) -> bool {
        self.devnet
    }

    /// Get the current tip hash.
    #[inline]
    pub fn tip_hash(&self) -> [u8; 32] {
        self.tip
    }

    /// Get the current tip block.
    pub fn tip_block(&self) -> &StoredBlock {
        self.blocks_by_hash.get(&self.tip).expect("tip must exist")
    }

    /// Get the current chain height.
    pub fn height(&self) -> u64 {
        self.tip_block().height()
    }

    /// Get the current tip difficulty.
    pub fn tip_difficulty(&self) -> u64 {
        self.tip_block().difficulty()
    }

    /// Get the current protocol state at the tip.
    pub fn state(&self) -> &ProtocolState {
        &self.state
    }

    /// Get a mutable reference to the current protocol state.
    pub fn state_mut(&mut self) -> &mut ProtocolState {
        &mut self.state
    }

    /// Get the state at a specific block hash if cached.
    ///
    /// Returns the cached state if available. Returns `None` if the state
    /// is not cached (block is too old or not on the main chain).
    ///
    /// For the current tip, use `state()` instead.
    /// To get state with replay fallback, use `compute_state_at()`.
    pub fn state_at(&self, hash: &[u8; 32]) -> Option<&ProtocolState> {
        // Tip state is always available
        if *hash == self.tip {
            return Some(&self.state);
        }
        // Check cache for recent blocks
        self.state_cache.get(hash)
    }

    /// Compute the state at a specific block hash.
    ///
    /// If the state is cached, returns a clone of it.
    /// Otherwise, replays blocks from the nearest cached ancestor (or genesis)
    /// to compute the state. This enables reorgs and fork validation.
    pub fn compute_state_at(&self, hash: &[u8; 32]) -> ChainResult<ProtocolState> {
        // Fast path: state is cached
        if let Some(state) = self.state_at(hash) {
            return Ok(state.clone());
        }

        // Find the path from a cached state to the target
        let (cached_hash, blocks_to_replay) = self.find_replay_path(hash)?;

        // Get the cached starting state
        let mut state = self
            .state_at(&cached_hash)
            .ok_or(ChainError::AncestorStateNotCached { hash: cached_hash })?
            .clone();

        // Replay blocks to compute the target state
        for block_hash in blocks_to_replay {
            let stored = self
                .blocks_by_hash
                .get(&block_hash)
                .ok_or(ChainError::UnknownPrevBlock { hash: block_hash })?;

            state = self.apply_block_to_state(&stored.block, state)?;
        }

        Ok(state)
    }

    /// Find the replay path from a cached ancestor to the target block.
    ///
    /// Returns (cached_ancestor_hash, [blocks_to_replay_in_order]).
    /// The blocks_to_replay list is in application order (oldest first).
    fn find_replay_path(&self, target_hash: &[u8; 32]) -> ChainResult<([u8; 32], Vec<[u8; 32]>)> {
        let mut blocks_to_replay = Vec::new();
        let mut current_hash = *target_hash;

        // Walk back until we find a cached state
        loop {
            // Check if current state is cached
            if self.state_at(&current_hash).is_some() {
                // Found a cached state - reverse the path and return
                blocks_to_replay.reverse();
                return Ok((current_hash, blocks_to_replay));
            }

            // Get the block and its parent
            let block = self
                .blocks_by_hash
                .get(&current_hash)
                .ok_or(ChainError::UnknownPrevBlock { hash: current_hash })?;

            // Add this block to the replay list
            blocks_to_replay.push(current_hash);

            // Move to parent
            if block.block.is_genesis() {
                // Genesis state should always be cached
                return Err(ChainError::AncestorStateNotCached { hash: current_hash });
            }
            current_hash = block.prev_hash();
        }
    }

    /// Apply a block's transactions to a state, returning the new state.
    ///
    /// This is used during replay for reorg and fork validation.
    fn apply_block_to_state(&self, block: &Block, mut state: ProtocolState) -> ChainResult<ProtocolState> {
        use tunnels_state::{apply_transaction, ExecutionContext, StateReader};

        let tx_difficulty = state.get_tx_difficulty_state().current_tx_difficulty;
        let _ctx = ExecutionContext::new(
            block.header.timestamp,
            block.header.height,
            tx_difficulty,
        );

        // Apply each transaction
        for (index, tx) in block.transactions.iter().enumerate() {
            apply_transaction(&mut state, tx, block.header.timestamp).map_err(|e| {
                ChainError::InvalidTransaction {
                    index,
                    error: e.to_string(),
                }
            })?;
        }

        // Update transaction difficulty state
        let tx_count = block.transactions.len() as u64;
        state.update_tx_difficulty_state(|diff_state| {
            diff_state.record_block(tx_count, tunnels_core::TransactionDifficultyState::DEFAULT_WINDOW_SIZE);
        });

        Ok(state)
    }

    /// Get a block by its hash.
    pub fn get_block(&self, hash: &[u8; 32]) -> Option<&StoredBlock> {
        self.blocks_by_hash.get(hash)
    }

    /// Get a block on the main chain at a specific height.
    pub fn get_block_at_height(&self, height: u64) -> Option<&StoredBlock> {
        self.blocks_by_height
            .get(&height)?
            .iter()
            .filter_map(|hash| self.blocks_by_hash.get(hash))
            .find(|block| block.is_main_chain)
    }

    /// Check if a block hash is known.
    pub fn has_block(&self, hash: &[u8; 32]) -> bool {
        self.blocks_by_hash.contains_key(hash)
    }

    /// Get the total work of the current best chain.
    pub fn total_work(&self) -> U256 {
        self.tip_block().total_work
    }

    /// Compute the median-time-past for a block's parent.
    ///
    /// Returns the median of the last 11 block timestamps.
    /// For the genesis block or early blocks, returns 0.
    pub fn compute_median_time_past(&self, parent_hash: &[u8; 32]) -> u64 {
        let mut timestamps = Vec::with_capacity(MEDIAN_TIME_PAST_BLOCKS);

        let mut current_hash = *parent_hash;
        for _ in 0..MEDIAN_TIME_PAST_BLOCKS {
            if let Some(block) = self.blocks_by_hash.get(&current_hash) {
                timestamps.push(block.block.header.timestamp);
                if block.block.is_genesis() {
                    break;
                }
                current_hash = block.block.header.prev_block_hash;
            } else {
                break;
            }
        }

        if timestamps.is_empty() {
            return 0;
        }

        timestamps.sort_unstable();
        timestamps[timestamps.len() / 2]
    }

    /// Add a new block to the chain.
    ///
    /// The block is validated, and if valid:
    /// - If it extends the best chain, it becomes the new tip
    /// - If it creates a heavier fork, the chain reorganizes
    /// - Otherwise, it's stored as a fork block
    ///
    /// For blocks that extend a fork (parent state not cached), the parent
    /// state is computed via replay from the nearest cached ancestor.
    pub fn add_block(&mut self, block: Block, current_time: u64) -> ChainResult<()> {
        let block_hash = block.hash();

        // Check if block already exists
        if self.blocks_by_hash.contains_key(&block_hash) {
            return Err(ChainError::BlockAlreadyExists { hash: block_hash });
        }

        // Genesis block handling
        if block.is_genesis() {
            return Err(ChainError::GenesisAlreadyExists);
        }

        // Get parent state (compute via replay if not cached)
        let parent_hash = block.header.prev_block_hash;
        let parent_state = self.compute_state_at(&parent_hash)?;

        // Validate the block with the computed parent state
        let new_state = validate_block_with_state_devnet(&block, self, current_time, parent_state, self.devnet)?;

        // Get parent block
        let parent = self
            .blocks_by_hash
            .get(&block.header.prev_block_hash)
            .ok_or(ChainError::UnknownPrevBlock {
                hash: block.header.prev_block_hash,
            })?;

        // Calculate total work
        let block_work = work_for_difficulty(block.header.difficulty);
        let total_work = parent.total_work + block_work;

        // Determine if this extends the main chain or creates a fork
        let extends_tip = block.header.prev_block_hash == self.tip;
        let is_heavier = total_work > self.total_work();

        // Create stored block
        let is_main_chain = extends_tip || is_heavier;
        let stored_block = StoredBlock::new(block.clone(), total_work, is_main_chain, current_time);

        // Store the block
        self.blocks_by_hash.insert(block_hash, stored_block);
        self.blocks_by_height
            .entry(block.height())
            .or_default()
            .push(block_hash);

        if extends_tip {
            // Simple case: extends current best chain
            self.apply_new_tip(block_hash, new_state, block.header.timestamp);
        } else if is_heavier {
            // Reorg case: new chain is heavier
            self.reorganize(block_hash, new_state)?;
        }
        // Otherwise: just a fork block, already stored

        Ok(())
    }

    /// Apply a new block as the chain tip.
    fn apply_new_tip(&mut self, block_hash: [u8; 32], new_state: ProtocolState, timestamp: u64) {
        // Cache the old state before updating
        if self.state_cache.len() >= STATE_CACHE_DEPTH {
            // Remove oldest cached state
            let oldest_height = self.height().saturating_sub(STATE_CACHE_DEPTH as u64);
            if let Some(hashes) = self.blocks_by_height.get(&oldest_height) {
                for hash in hashes {
                    self.state_cache.remove(hash);
                }
            }
        }
        self.state_cache.insert(self.tip, self.state.clone());

        // Update tip
        self.tip = block_hash;
        self.state = new_state;

        // Update recent timestamps
        if self.recent_timestamps.len() >= MEDIAN_TIME_PAST_BLOCKS {
            self.recent_timestamps.pop_front();
        }
        self.recent_timestamps.push_back(timestamp);
    }

    /// Reorganize the chain to a new tip.
    ///
    /// This handles chain reorgs when a competing fork has more cumulative work:
    /// 1. Find the common ancestor (fork point) between old and new tips
    /// 2. Mark old chain blocks as not main chain
    /// 3. Mark new chain blocks as main chain
    /// 4. Update state cache: remove old chain states, add new chain states
    /// 5. Update tip and current state
    fn reorganize(&mut self, new_tip: [u8; 32], new_state: ProtocolState) -> ChainResult<()> {
        let old_tip = self.tip;

        // Find the fork point
        let fork_point = find_fork_point(self, &old_tip, &new_tip)?;

        // Collect blocks on the old chain (to remove from main chain and cache)
        let mut old_chain_blocks = Vec::new();
        let mut hash = old_tip;
        while hash != fork_point {
            old_chain_blocks.push(hash);
            if let Some(block) = self.blocks_by_hash.get(&hash) {
                hash = block.prev_hash();
            } else {
                break;
            }
        }

        // Collect blocks on the new chain (to add to main chain)
        let mut new_chain_blocks = Vec::new();
        let mut hash = new_tip;
        while hash != fork_point {
            new_chain_blocks.push(hash);
            if let Some(block) = self.blocks_by_hash.get(&hash) {
                hash = block.prev_hash();
            } else {
                break;
            }
        }
        // Reverse to get application order (oldest first)
        new_chain_blocks.reverse();

        // Mark old chain blocks as not main chain and remove from cache
        for block_hash in &old_chain_blocks {
            if let Some(block) = self.blocks_by_hash.get_mut(block_hash) {
                block.is_main_chain = false;
            }
            self.state_cache.remove(block_hash);
        }

        // Mark new chain blocks as main chain
        for block_hash in &new_chain_blocks {
            if let Some(block) = self.blocks_by_hash.get_mut(block_hash) {
                block.is_main_chain = true;
            }
        }

        // Rebuild the state cache for the new main chain
        // We need to replay from fork point to cache recent states
        self.rebuild_state_cache(&fork_point, &new_chain_blocks)?;

        // Update tip and state
        self.tip = new_tip;
        self.state = new_state;

        // Rebuild recent timestamps from new chain
        self.rebuild_recent_timestamps();

        Ok(())
    }

    /// Rebuild the state cache after a reorg.
    ///
    /// Replays blocks from the fork point forward and caches the most recent states.
    fn rebuild_state_cache(
        &mut self,
        fork_point: &[u8; 32],
        new_chain_blocks: &[[u8; 32]],
    ) -> ChainResult<()> {
        // Start from fork point state
        let mut current_state = self
            .state_at(fork_point)
            .ok_or(ChainError::AncestorStateNotCached { hash: *fork_point })?
            .clone();

        // Replay each block on the new chain
        for (i, block_hash) in new_chain_blocks.iter().enumerate() {
            let block = &self
                .blocks_by_hash
                .get(block_hash)
                .ok_or(ChainError::UnknownPrevBlock { hash: *block_hash })?
                .block
                .clone();

            current_state = self.apply_block_to_state(block, current_state)?;

            // Cache recent states (last STATE_CACHE_DEPTH blocks)
            let blocks_from_end = new_chain_blocks.len() - i - 1;
            if blocks_from_end < STATE_CACHE_DEPTH {
                self.state_cache.insert(*block_hash, current_state.clone());
            }
        }

        // Prune cache if too large
        while self.state_cache.len() > STATE_CACHE_DEPTH + 1 {
            // Remove oldest cached states (not on new main chain)
            let to_remove: Vec<_> = self
                .state_cache
                .keys()
                .filter(|h| {
                    // Keep fork point and new chain blocks
                    **h != *fork_point && !new_chain_blocks.contains(h)
                })
                .copied()
                .collect();

            for hash in to_remove.into_iter().take(1) {
                self.state_cache.remove(&hash);
            }

            // Safety: break if we can't remove anything
            if self.state_cache.len() > STATE_CACHE_DEPTH + 1 {
                break;
            }
        }

        Ok(())
    }

    /// Rebuild recent timestamps from the current main chain.
    fn rebuild_recent_timestamps(&mut self) {
        self.recent_timestamps.clear();

        let mut timestamps = Vec::with_capacity(MEDIAN_TIME_PAST_BLOCKS);
        let mut hash = self.tip;

        for _ in 0..MEDIAN_TIME_PAST_BLOCKS {
            if let Some(block) = self.blocks_by_hash.get(&hash) {
                timestamps.push(block.block.header.timestamp);
                if block.block.is_genesis() {
                    break;
                }
                hash = block.prev_hash();
            } else {
                break;
            }
        }

        // Reverse to get chronological order
        timestamps.reverse();
        for ts in timestamps {
            self.recent_timestamps.push_back(ts);
        }
    }

    /// Get the genesis block hash.
    pub fn genesis_hash(&self) -> [u8; 32] {
        self.genesis_hash
    }

    /// Get the number of stored blocks.
    pub fn block_count(&self) -> usize {
        self.blocks_by_hash.len()
    }
}

impl Default for ChainState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::create_genesis_block;

    #[test]
    fn test_chain_state_new() {
        let chain = ChainState::new();
        let genesis = create_genesis_block();

        assert_eq!(chain.tip_hash(), genesis.hash());
        assert_eq!(chain.height(), 0);
        assert_eq!(chain.block_count(), 1);
    }

    #[test]
    fn test_genesis_determinism() {
        // Acceptance criterion 1
        let chain1 = ChainState::new();
        let chain2 = ChainState::new();

        assert_eq!(chain1.genesis_hash(), chain2.genesis_hash());
        assert_eq!(chain1.tip_hash(), chain2.tip_hash());
    }

    #[test]
    fn test_get_block_at_height() {
        let chain = ChainState::new();

        let genesis = chain.get_block_at_height(0);
        assert!(genesis.is_some());
        assert!(genesis.unwrap().block.is_genesis());

        let nonexistent = chain.get_block_at_height(1);
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_median_time_past() {
        let chain = ChainState::new();
        let genesis = create_genesis_block();

        // For genesis, MTP should be the genesis timestamp
        let mtp = chain.compute_median_time_past(&genesis.hash());
        assert_eq!(mtp, genesis.header.timestamp);
    }

    #[test]
    fn test_has_block() {
        let chain = ChainState::new();
        let genesis = create_genesis_block();

        assert!(chain.has_block(&genesis.hash()));
        assert!(!chain.has_block(&[0xFFu8; 32]));
    }
}
