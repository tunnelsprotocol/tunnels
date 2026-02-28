//! Block indexing utilities.
//!
//! Provides efficient lookups for blocks by various attributes.

use std::sync::Arc;

use crate::error::StorageError;
use crate::kv::KvBackend;

use super::BlockStore;

/// Block index providing various lookup methods.
///
/// This is a thin wrapper around `BlockStore` that provides
/// additional indexing capabilities.
pub struct BlockIndex<B: KvBackend> {
    store: BlockStore<B>,
}

impl<B: KvBackend> BlockIndex<B> {
    /// Create a new block index wrapping a store.
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            store: BlockStore::new(backend),
        }
    }

    /// Get the underlying block store.
    pub fn store(&self) -> &BlockStore<B> {
        &self.store
    }

    /// Get blocks in a height range (inclusive).
    pub fn get_blocks_in_range(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<tunnels_core::Block>, StorageError> {
        let mut blocks = Vec::new();
        for height in start_height..=end_height {
            if let Some(block) = self.store.get_block_by_height(height)? {
                blocks.push(block);
            } else {
                break;
            }
        }
        Ok(blocks)
    }

    /// Get the chain length (number of blocks).
    pub fn chain_length(&self) -> Result<u64, StorageError> {
        match self.store.get_latest_height()? {
            Some(height) => Ok(height + 1),
            None => Ok(0),
        }
    }

    /// Get block hashes in a height range.
    pub fn get_hashes_in_range(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<[u8; 32]>, StorageError> {
        let mut hashes = Vec::new();
        for height in start_height..=end_height {
            if let Some(hash) = self.store.get_block_hash_by_height(height)? {
                hashes.push(hash);
            } else {
                break;
            }
        }
        Ok(hashes)
    }

    /// Find the common ancestor height between the stored chain and a list of block hashes.
    ///
    /// Returns the height of the highest block that matches both chains.
    pub fn find_common_ancestor(&self, block_hashes: &[[u8; 32]]) -> Result<Option<u64>, StorageError> {
        // block_hashes[i] is the hash at height i
        for (height, hash) in block_hashes.iter().enumerate().rev() {
            if let Some(stored_hash) = self.store.get_block_hash_by_height(height as u64)? {
                if stored_hash == *hash {
                    return Ok(Some(height as u64));
                }
            }
        }
        Ok(None)
    }

    /// Verify chain integrity from start to end height.
    ///
    /// Checks that each block's prev_block_hash matches the previous block's hash.
    pub fn verify_chain_integrity(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<bool, StorageError> {
        let mut prev_hash = if start_height == 0 {
            [0u8; 32]
        } else {
            match self.store.get_block_hash_by_height(start_height - 1)? {
                Some(h) => h,
                None => return Ok(false),
            }
        };

        for height in start_height..=end_height {
            let block = match self.store.get_block_by_height(height)? {
                Some(b) => b,
                None => return Ok(false),
            };

            if block.header.prev_block_hash != prev_hash {
                return Ok(false);
            }

            prev_hash = block.hash();
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::MemoryBackend;
    use tunnels_core::{Block, BlockHeader};

    fn create_test_block(height: u64, prev_hash: [u8; 32]) -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                height,
                timestamp: 1700000000 + height,
                prev_block_hash: prev_hash,
                state_root: [height as u8; 32],
                tx_root: [0u8; 32],
                difficulty: 1,
                nonce: 0,
            },
            transactions: vec![],
        }
    }

    fn create_index_with_chain(num_blocks: u64) -> BlockIndex<MemoryBackend> {
        let backend = Arc::new(MemoryBackend::new());
        let index = BlockIndex::new(backend);

        let mut prev_hash = [0u8; 32];
        for height in 0..num_blocks {
            let block = create_test_block(height, prev_hash);
            prev_hash = block.hash();
            index.store.store_block(&block, [height as u8; 32]).unwrap();
        }

        index
    }

    #[test]
    fn test_chain_length() {
        let index = create_index_with_chain(5);
        assert_eq!(index.chain_length().unwrap(), 5);
    }

    #[test]
    fn test_empty_chain_length() {
        let backend = Arc::new(MemoryBackend::new());
        let index = BlockIndex::new(backend);
        assert_eq!(index.chain_length().unwrap(), 0);
    }

    #[test]
    fn test_get_blocks_in_range() {
        let index = create_index_with_chain(10);

        let blocks = index.get_blocks_in_range(3, 6).unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].height(), 3);
        assert_eq!(blocks[3].height(), 6);
    }

    #[test]
    fn test_get_hashes_in_range() {
        let index = create_index_with_chain(5);

        let hashes = index.get_hashes_in_range(0, 4).unwrap();
        assert_eq!(hashes.len(), 5);

        // Verify hashes match blocks
        for (i, hash) in hashes.iter().enumerate() {
            let block = index.store.get_block_by_height(i as u64).unwrap().unwrap();
            assert_eq!(*hash, block.hash());
        }
    }

    #[test]
    fn test_verify_chain_integrity() {
        let index = create_index_with_chain(10);
        assert!(index.verify_chain_integrity(0, 9).unwrap());
        assert!(index.verify_chain_integrity(5, 9).unwrap());
    }

    #[test]
    fn test_find_common_ancestor() {
        let index = create_index_with_chain(5);

        // Get actual hashes
        let hashes = index.get_hashes_in_range(0, 4).unwrap();

        // All match
        let ancestor = index.find_common_ancestor(&hashes).unwrap();
        assert_eq!(ancestor, Some(4));

        // Partial match (first 3 match)
        let mut diverged = hashes.clone();
        diverged[3] = [0xFF; 32];
        diverged[4] = [0xFE; 32];
        let ancestor = index.find_common_ancestor(&diverged).unwrap();
        assert_eq!(ancestor, Some(2));

        // No match
        let none_match = [[0xFF; 32]; 5];
        let ancestor = index.find_common_ancestor(&none_match).unwrap();
        assert_eq!(ancestor, None);
    }
}
