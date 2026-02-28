//! Key-value storage backends.
//!
//! This module provides an abstraction over key-value storage with two implementations:
//! - `MemoryBackend`: In-memory HashMap-based storage for testing
//! - `RocksBackend`: RocksDB-based persistent storage for production

mod memory_backend;
mod rocks_backend;

pub use memory_backend::MemoryBackend;
pub use rocks_backend::RocksBackend;

use crate::error::StorageError;

/// Type alias for the iterator returned by prefix_iterator.
pub type PrefixIterator<'a> = Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a>;

/// Trait for key-value storage backends.
///
/// Implementations must provide atomic batch writes and ordered iteration.
pub trait KvBackend: Send + Sync {
    /// Get a value by key.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError>;

    /// Put a key-value pair.
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError>;

    /// Delete a key.
    fn delete(&self, key: &[u8]) -> Result<(), StorageError>;

    /// Check if a key exists.
    fn exists(&self, key: &[u8]) -> Result<bool, StorageError> {
        Ok(self.get(key)?.is_some())
    }

    /// Apply a batch of writes atomically.
    fn write_batch(&self, batch: WriteBatch) -> Result<(), StorageError>;

    /// Iterate over all keys with a given prefix.
    fn prefix_iterator(&self, prefix: &[u8]) -> Result<PrefixIterator<'_>, StorageError>;

    /// Flush any buffered data to disk (if applicable).
    fn flush(&self) -> Result<(), StorageError> {
        Ok(())
    }
}

/// A batch of write operations to be applied atomically.
#[derive(Clone, Debug, Default)]
pub struct WriteBatch {
    /// Operations in the batch.
    pub operations: Vec<BatchOp>,
}

/// A single operation in a write batch.
#[derive(Clone, Debug)]
pub enum BatchOp {
    /// Put a key-value pair.
    Put {
        /// The key to write.
        key: Vec<u8>,
        /// The value to write.
        value: Vec<u8>,
    },
    /// Delete a key.
    Delete {
        /// The key to delete.
        key: Vec<u8>,
    },
}

impl WriteBatch {
    /// Create a new empty write batch.
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    /// Add a put operation to the batch.
    pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.operations.push(BatchOp::Put { key, value });
    }

    /// Add a delete operation to the batch.
    pub fn delete(&mut self, key: Vec<u8>) {
        self.operations.push(BatchOp::Delete { key });
    }

    /// Check if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Get the number of operations in the batch.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Clear all operations from the batch.
    pub fn clear(&mut self) {
        self.operations.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_backend_basic<B: KvBackend>(backend: B) {
        // Test put and get
        backend.put(b"key1", b"value1").unwrap();
        let value = backend.get(b"key1").unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));

        // Test non-existent key
        let value = backend.get(b"nonexistent").unwrap();
        assert!(value.is_none());

        // Test exists
        assert!(backend.exists(b"key1").unwrap());
        assert!(!backend.exists(b"nonexistent").unwrap());

        // Test delete
        backend.delete(b"key1").unwrap();
        assert!(!backend.exists(b"key1").unwrap());
    }

    fn test_backend_batch<B: KvBackend>(backend: B) {
        let mut batch = WriteBatch::new();
        batch.put(b"batch1".to_vec(), b"value1".to_vec());
        batch.put(b"batch2".to_vec(), b"value2".to_vec());
        batch.put(b"batch3".to_vec(), b"value3".to_vec());

        backend.write_batch(batch).unwrap();

        assert_eq!(backend.get(b"batch1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(backend.get(b"batch2").unwrap(), Some(b"value2".to_vec()));
        assert_eq!(backend.get(b"batch3").unwrap(), Some(b"value3".to_vec()));

        // Test batch with deletes
        let mut batch = WriteBatch::new();
        batch.delete(b"batch1".to_vec());
        batch.put(b"batch4".to_vec(), b"value4".to_vec());

        backend.write_batch(batch).unwrap();

        assert!(backend.get(b"batch1").unwrap().is_none());
        assert_eq!(backend.get(b"batch4").unwrap(), Some(b"value4".to_vec()));
    }

    fn test_backend_prefix_iter<B: KvBackend>(backend: B) {
        backend.put(b"prefix:a", b"1").unwrap();
        backend.put(b"prefix:b", b"2").unwrap();
        backend.put(b"prefix:c", b"3").unwrap();
        backend.put(b"other:x", b"4").unwrap();

        let iter = backend.prefix_iterator(b"prefix:").unwrap();
        let items: Vec<_> = iter.collect();

        assert_eq!(items.len(), 3);
        // Items should be sorted by key
        assert!(items.iter().any(|(k, v)| k == b"prefix:a" && v == b"1"));
        assert!(items.iter().any(|(k, v)| k == b"prefix:b" && v == b"2"));
        assert!(items.iter().any(|(k, v)| k == b"prefix:c" && v == b"3"));
    }

    #[test]
    fn test_memory_backend_basic() {
        test_backend_basic(MemoryBackend::new());
    }

    #[test]
    fn test_memory_backend_batch() {
        test_backend_batch(MemoryBackend::new());
    }

    #[test]
    fn test_memory_backend_prefix_iter() {
        test_backend_prefix_iter(MemoryBackend::new());
    }
}
