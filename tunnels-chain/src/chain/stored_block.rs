//! Stored block with metadata.

use tunnels_core::block::Block;
use tunnels_core::U256;

/// A block stored in the chain with additional metadata.
#[derive(Clone, Debug)]
pub struct StoredBlock {
    /// The block itself.
    pub block: Block,

    /// Block hash (cached for efficiency).
    pub hash: [u8; 32],

    /// Total accumulated work from genesis to this block.
    ///
    /// Work is approximated as 2^difficulty for each block.
    /// Used for fork choice: the chain with most total work wins.
    pub total_work: U256,

    /// Whether this block is on the main chain.
    ///
    /// Blocks on forks have this set to false.
    pub is_main_chain: bool,

    /// Timestamp when this block was received/validated.
    pub received_at: u64,
}

impl StoredBlock {
    /// Create a new stored block.
    pub fn new(block: Block, total_work: U256, is_main_chain: bool, received_at: u64) -> Self {
        let hash = block.hash();
        Self {
            block,
            hash,
            total_work,
            is_main_chain,
            received_at,
        }
    }

    /// Get the block height.
    #[inline]
    pub fn height(&self) -> u64 {
        self.block.height()
    }

    /// Get the previous block hash.
    #[inline]
    pub fn prev_hash(&self) -> [u8; 32] {
        self.block.header.prev_block_hash
    }

    /// Get the block difficulty.
    #[inline]
    pub fn difficulty(&self) -> u64 {
        self.block.header.difficulty
    }

    /// Calculate the work for this single block.
    ///
    /// Work is approximated as 2^difficulty.
    pub fn block_work(&self) -> U256 {
        work_for_difficulty(self.difficulty())
    }
}

/// Calculate work for a given difficulty.
///
/// Work = 2^difficulty (approximation of expected hash attempts).
pub fn work_for_difficulty(difficulty: u64) -> U256 {
    if difficulty >= 256 {
        // Maximum possible work (all 256 bits)
        U256::MAX
    } else {
        U256::from(1u64) << difficulty as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunnels_core::block::BlockHeader;

    fn test_block(height: u64, difficulty: u64) -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                height,
                timestamp: 1700000000 + height * 60,
                prev_block_hash: [0u8; 32],
                state_root: [0u8; 32],
                tx_root: [0u8; 32],
                difficulty,
                nonce: 0,
            },
            transactions: vec![],
        }
    }

    #[test]
    fn test_stored_block_creation() {
        let block = test_block(1, 8);
        let stored = StoredBlock::new(block.clone(), U256::from(512u64), true, 1700000060);

        assert_eq!(stored.hash, block.hash());
        assert_eq!(stored.height(), 1);
        assert_eq!(stored.difficulty(), 8);
        assert!(stored.is_main_chain);
    }

    #[test]
    fn test_work_for_difficulty() {
        assert_eq!(work_for_difficulty(0), U256::from(1u64));
        assert_eq!(work_for_difficulty(1), U256::from(2u64));
        assert_eq!(work_for_difficulty(8), U256::from(256u64));
        assert_eq!(work_for_difficulty(16), U256::from(65536u64));
    }

    #[test]
    fn test_block_work() {
        let block = test_block(1, 10);
        let stored = StoredBlock::new(block, U256::zero(), true, 0);
        assert_eq!(stored.block_work(), U256::from(1024u64));
    }
}
