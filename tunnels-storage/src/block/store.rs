//! Append-only block storage.
//!
//! Stores blocks by hash and provides efficient retrieval.

use std::sync::Arc;

use tunnels_core::{Block, serialization::{serialize, deserialize}};

use crate::error::StorageError;
use crate::keys::{block_by_hash_key, block_hash_by_height_key, latest_block_height_key, state_root_by_height_key};
use crate::kv::{KvBackend, WriteBatch};

/// Append-only block storage.
///
/// Blocks are stored by hash and indexed by height for efficient retrieval.
pub struct BlockStore<B: KvBackend> {
    backend: Arc<B>,
}

impl<B: KvBackend> BlockStore<B> {
    /// Create a new block store.
    pub fn new(backend: Arc<B>) -> Self {
        Self { backend }
    }

    /// Get the backend.
    pub fn backend(&self) -> &Arc<B> {
        &self.backend
    }

    /// Get a block by hash.
    pub fn get_block(&self, hash: &[u8; 32]) -> Result<Option<Block>, StorageError> {
        let key = block_by_hash_key(hash);
        match self.backend.get(&key)? {
            Some(bytes) => {
                let block: Block = deserialize(&bytes)?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    /// Get a block by height.
    pub fn get_block_by_height(&self, height: u64) -> Result<Option<Block>, StorageError> {
        let hash = self.get_block_hash_by_height(height)?;
        match hash {
            Some(h) => self.get_block(&h),
            None => Ok(None),
        }
    }

    /// Get a block hash by height.
    pub fn get_block_hash_by_height(&self, height: u64) -> Result<Option<[u8; 32]>, StorageError> {
        let key = block_hash_by_height_key(height);
        match self.backend.get(&key)? {
            Some(bytes) => {
                if bytes.len() != 32 {
                    return Err(StorageError::Block("Invalid block hash length".into()));
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&bytes);
                Ok(Some(hash))
            }
            None => Ok(None),
        }
    }

    /// Get the latest block height.
    pub fn get_latest_height(&self) -> Result<Option<u64>, StorageError> {
        let key = latest_block_height_key();
        match self.backend.get(&key)? {
            Some(bytes) => {
                if bytes.len() != 8 {
                    return Err(StorageError::Block("Invalid height encoding".into()));
                }
                let height = u64::from_be_bytes(bytes.try_into().unwrap());
                Ok(Some(height))
            }
            None => Ok(None),
        }
    }

    /// Get the latest block.
    pub fn get_latest_block(&self) -> Result<Option<Block>, StorageError> {
        match self.get_latest_height()? {
            Some(height) => self.get_block_by_height(height),
            None => Ok(None),
        }
    }

    /// Get the state root at a specific height.
    pub fn get_state_root(&self, height: u64) -> Result<Option<[u8; 32]>, StorageError> {
        let key = state_root_by_height_key(height);
        match self.backend.get(&key)? {
            Some(bytes) => {
                if bytes.len() != 32 {
                    return Err(StorageError::Block("Invalid state root length".into()));
                }
                let mut root = [0u8; 32];
                root.copy_from_slice(&bytes);
                Ok(Some(root))
            }
            None => Ok(None),
        }
    }

    /// Store a block with its state root.
    ///
    /// This is an append-only operation. The block must be the next block
    /// in sequence (height = latest_height + 1 or height = 0 for genesis).
    pub fn store_block(&self, block: &Block, state_root: [u8; 32]) -> Result<(), StorageError> {
        let height = block.height();
        let hash = block.hash();

        // Validate height is sequential
        let expected_height = match self.get_latest_height()? {
            Some(latest) => latest + 1,
            None => 0,
        };

        if height != expected_height {
            return Err(StorageError::Block(format!(
                "Block height {} does not follow latest height {}",
                height,
                expected_height.saturating_sub(1)
            )));
        }

        // Validate prev_block_hash for non-genesis
        if height > 0 {
            let prev_hash = self.get_block_hash_by_height(height - 1)?
                .ok_or_else(|| StorageError::Block("Previous block not found".into()))?;
            if block.header.prev_block_hash != prev_hash {
                return Err(StorageError::Block("Previous block hash mismatch".into()));
            }
        }

        // Build batch write
        let mut batch = WriteBatch::new();

        // Store block by hash
        let block_bytes = serialize(block)?;
        batch.put(block_by_hash_key(&hash), block_bytes);

        // Store hash by height
        batch.put(block_hash_by_height_key(height), hash.to_vec());

        // Store state root by height
        batch.put(state_root_by_height_key(height), state_root.to_vec());

        // Update latest height
        batch.put(latest_block_height_key(), height.to_be_bytes().to_vec());

        self.backend.write_batch(batch)?;

        Ok(())
    }

    /// Check if a block exists by hash.
    pub fn has_block(&self, hash: &[u8; 32]) -> Result<bool, StorageError> {
        let key = block_by_hash_key(hash);
        self.backend.exists(&key)
    }

    /// Check if a block exists at height.
    pub fn has_height(&self, height: u64) -> Result<bool, StorageError> {
        let key = block_hash_by_height_key(height);
        self.backend.exists(&key)
    }

    /// Get the genesis block.
    pub fn get_genesis(&self) -> Result<Option<Block>, StorageError> {
        self.get_block_by_height(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::MemoryBackend;
    use tunnels_core::BlockHeader;

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

    fn create_store() -> BlockStore<MemoryBackend> {
        let backend = Arc::new(MemoryBackend::new());
        BlockStore::new(backend)
    }

    #[test]
    fn test_empty_store() {
        let store = create_store();
        assert!(store.get_latest_height().unwrap().is_none());
        assert!(store.get_genesis().unwrap().is_none());
    }

    #[test]
    fn test_store_genesis() {
        let store = create_store();
        let genesis = create_test_block(0, [0u8; 32]);
        let state_root = [1u8; 32];

        store.store_block(&genesis, state_root).unwrap();

        assert_eq!(store.get_latest_height().unwrap(), Some(0));
        assert!(store.has_height(0).unwrap());

        let loaded = store.get_genesis().unwrap().unwrap();
        assert_eq!(loaded.height(), 0);

        let loaded_root = store.get_state_root(0).unwrap().unwrap();
        assert_eq!(loaded_root, state_root);
    }

    #[test]
    fn test_store_chain() {
        let store = create_store();

        // Genesis
        let genesis = create_test_block(0, [0u8; 32]);
        store.store_block(&genesis, [1u8; 32]).unwrap();

        // Block 1
        let block1 = create_test_block(1, genesis.hash());
        store.store_block(&block1, [2u8; 32]).unwrap();

        // Block 2
        let block2 = create_test_block(2, block1.hash());
        store.store_block(&block2, [3u8; 32]).unwrap();

        assert_eq!(store.get_latest_height().unwrap(), Some(2));

        let loaded0 = store.get_block_by_height(0).unwrap().unwrap();
        let loaded1 = store.get_block_by_height(1).unwrap().unwrap();
        let loaded2 = store.get_block_by_height(2).unwrap().unwrap();

        assert_eq!(loaded0.hash(), genesis.hash());
        assert_eq!(loaded1.hash(), block1.hash());
        assert_eq!(loaded2.hash(), block2.hash());
    }

    #[test]
    fn test_reject_non_sequential() {
        let store = create_store();

        // Try to store block 1 without genesis
        let block = create_test_block(1, [0u8; 32]);
        let result = store.store_block(&block, [1u8; 32]);

        assert!(result.is_err());
    }

    #[test]
    fn test_reject_wrong_prev_hash() {
        let store = create_store();

        // Store genesis
        let genesis = create_test_block(0, [0u8; 32]);
        store.store_block(&genesis, [1u8; 32]).unwrap();

        // Try to store block with wrong prev_hash
        let bad_block = create_test_block(1, [0xFF; 32]); // Wrong prev hash
        let result = store.store_block(&bad_block, [2u8; 32]);

        assert!(result.is_err());
    }

    #[test]
    fn test_get_by_hash() {
        let store = create_store();
        let genesis = create_test_block(0, [0u8; 32]);
        let hash = genesis.hash();

        store.store_block(&genesis, [1u8; 32]).unwrap();

        assert!(store.has_block(&hash).unwrap());
        let loaded = store.get_block(&hash).unwrap().unwrap();
        assert_eq!(loaded.hash(), hash);

        // Non-existent hash
        assert!(!store.has_block(&[0xFF; 32]).unwrap());
    }
}
