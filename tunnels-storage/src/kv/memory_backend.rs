//! In-memory key-value backend for testing.

use std::collections::BTreeMap;
use std::sync::RwLock;

use super::{BatchOp, KvBackend, WriteBatch};
use crate::error::StorageError;

/// In-memory key-value backend using a BTreeMap.
///
/// This implementation is thread-safe and maintains keys in sorted order
/// for efficient prefix iteration. Useful for testing and development.
pub struct MemoryBackend {
    data: RwLock<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl MemoryBackend {
    /// Create a new empty in-memory backend.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(BTreeMap::new()),
        }
    }

    /// Get the number of entries in the store.
    pub fn len(&self) -> usize {
        self.data.read().unwrap().len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.data.read().unwrap().is_empty()
    }

    /// Clear all entries from the store.
    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl KvBackend for MemoryBackend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.data.write().unwrap().insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<(), StorageError> {
        self.data.write().unwrap().remove(key);
        Ok(())
    }

    fn write_batch(&self, batch: WriteBatch) -> Result<(), StorageError> {
        let mut data = self.data.write().unwrap();
        for op in batch.operations {
            match op {
                BatchOp::Put { key, value } => {
                    data.insert(key, value);
                }
                BatchOp::Delete { key } => {
                    data.remove(&key);
                }
            }
        }
        Ok(())
    }

    fn prefix_iterator(&self, prefix: &[u8]) -> Result<Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_>, StorageError> {
        let data = self.data.read().unwrap();

        // Collect matching entries
        let prefix_vec = prefix.to_vec();
        let entries: Vec<(Vec<u8>, Vec<u8>)> = data
            .range(prefix_vec.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix_vec))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Box::new(entries.into_iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let backend = MemoryBackend::new();
        assert!(backend.is_empty());
        assert_eq!(backend.len(), 0);
    }

    #[test]
    fn test_put_increases_len() {
        let backend = MemoryBackend::new();
        backend.put(b"key", b"value").unwrap();
        assert_eq!(backend.len(), 1);
        assert!(!backend.is_empty());
    }

    #[test]
    fn test_clear() {
        let backend = MemoryBackend::new();
        backend.put(b"key1", b"value1").unwrap();
        backend.put(b"key2", b"value2").unwrap();
        assert_eq!(backend.len(), 2);

        backend.clear();
        assert!(backend.is_empty());
    }

    #[test]
    fn test_overwrite() {
        let backend = MemoryBackend::new();
        backend.put(b"key", b"value1").unwrap();
        backend.put(b"key", b"value2").unwrap();

        assert_eq!(backend.len(), 1);
        assert_eq!(backend.get(b"key").unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_prefix_iterator_empty_prefix() {
        let backend = MemoryBackend::new();
        backend.put(b"a", b"1").unwrap();
        backend.put(b"b", b"2").unwrap();

        let iter = backend.prefix_iterator(b"").unwrap();
        let items: Vec<_> = iter.collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_prefix_iterator_no_matches() {
        let backend = MemoryBackend::new();
        backend.put(b"abc", b"1").unwrap();
        backend.put(b"abd", b"2").unwrap();

        let iter = backend.prefix_iterator(b"xyz").unwrap();
        let items: Vec<_> = iter.collect();
        assert!(items.is_empty());
    }
}
