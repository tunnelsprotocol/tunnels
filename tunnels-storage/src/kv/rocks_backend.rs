//! RocksDB key-value backend for production use.

use std::path::Path;
use std::sync::Arc;

use rocksdb::{DB, Options, IteratorMode};

use super::{BatchOp, KvBackend, WriteBatch};
use crate::error::StorageError;

/// RocksDB-based key-value backend.
///
/// This implementation provides persistent, crash-safe storage with
/// excellent performance for blockchain workloads.
pub struct RocksBackend {
    db: Arc<DB>,
}

impl RocksBackend {
    /// Open or create a RocksDB database at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        // Optimize for blockchain workloads
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB
        opts.set_level_compaction_dynamic_level_bytes(true);

        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Open a RocksDB database with custom options.
    pub fn open_with_opts<P: AsRef<Path>>(path: P, opts: Options) -> Result<Self, StorageError> {
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Get a reference to the underlying RocksDB instance.
    pub fn inner(&self) -> &DB {
        &self.db
    }

    /// Get database statistics as a string.
    pub fn stats(&self) -> Option<String> {
        self.db.property_value("rocksdb.stats").ok().flatten()
    }

    /// Get estimated number of keys in the database.
    pub fn estimate_num_keys(&self) -> Option<u64> {
        self.db
            .property_int_value("rocksdb.estimate-num-keys")
            .ok()
            .flatten()
    }

    /// Compact the entire database.
    pub fn compact(&self) {
        self.db.compact_range::<&[u8], &[u8]>(None, None);
    }
}

impl KvBackend for RocksBackend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.db.get(key)?)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.db.put(key, value)?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<(), StorageError> {
        self.db.delete(key)?;
        Ok(())
    }

    fn write_batch(&self, batch: WriteBatch) -> Result<(), StorageError> {
        let mut rocks_batch = rocksdb::WriteBatch::default();
        for op in batch.operations {
            match op {
                BatchOp::Put { key, value } => {
                    rocks_batch.put(&key, &value);
                }
                BatchOp::Delete { key } => {
                    rocks_batch.delete(&key);
                }
            }
        }
        self.db.write(rocks_batch)?;
        Ok(())
    }

    fn prefix_iterator(&self, prefix: &[u8]) -> Result<Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_>, StorageError> {
        let prefix_vec = prefix.to_vec();
        let iter = self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward));

        let prefix_iter = iter
            .map(|result| {
                result.map(|(k, v)| (k.to_vec(), v.to_vec()))
            })
            .take_while(move |result| {
                match result {
                    Ok((k, _)) => k.starts_with(&prefix_vec),
                    Err(_) => false,
                }
            })
            .filter_map(|result| result.ok());

        Ok(Box::new(prefix_iter))
    }

    fn flush(&self) -> Result<(), StorageError> {
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_backend() -> (RocksBackend, TempDir) {
        let dir = TempDir::new().unwrap();
        let backend = RocksBackend::open(dir.path()).unwrap();
        (backend, dir)
    }

    #[test]
    fn test_basic_operations() {
        let (backend, _dir) = create_temp_backend();

        // Put and get
        backend.put(b"key1", b"value1").unwrap();
        assert_eq!(backend.get(b"key1").unwrap(), Some(b"value1".to_vec()));

        // Non-existent key
        assert!(backend.get(b"nonexistent").unwrap().is_none());

        // Delete
        backend.delete(b"key1").unwrap();
        assert!(backend.get(b"key1").unwrap().is_none());
    }

    #[test]
    fn test_write_batch() {
        let (backend, _dir) = create_temp_backend();

        let mut batch = WriteBatch::new();
        batch.put(b"key1".to_vec(), b"value1".to_vec());
        batch.put(b"key2".to_vec(), b"value2".to_vec());
        batch.delete(b"key3".to_vec()); // Delete non-existent is ok

        backend.write_batch(batch).unwrap();

        assert_eq!(backend.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(backend.get(b"key2").unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_prefix_iterator() {
        let (backend, _dir) = create_temp_backend();

        backend.put(b"prefix:a", b"1").unwrap();
        backend.put(b"prefix:b", b"2").unwrap();
        backend.put(b"prefix:c", b"3").unwrap();
        backend.put(b"other:x", b"4").unwrap();

        let iter = backend.prefix_iterator(b"prefix:").unwrap();
        let items: Vec<_> = iter.collect();

        assert_eq!(items.len(), 3);
        assert!(items.contains(&(b"prefix:a".to_vec(), b"1".to_vec())));
        assert!(items.contains(&(b"prefix:b".to_vec(), b"2".to_vec())));
        assert!(items.contains(&(b"prefix:c".to_vec(), b"3".to_vec())));
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();

        // Write some data
        {
            let backend = RocksBackend::open(dir.path()).unwrap();
            backend.put(b"persistent", b"data").unwrap();
            backend.flush().unwrap();
        }

        // Reopen and verify
        {
            let backend = RocksBackend::open(dir.path()).unwrap();
            assert_eq!(backend.get(b"persistent").unwrap(), Some(b"data".to_vec()));
        }
    }

    #[test]
    fn test_exists() {
        let (backend, _dir) = create_temp_backend();

        assert!(!backend.exists(b"key").unwrap());
        backend.put(b"key", b"value").unwrap();
        assert!(backend.exists(b"key").unwrap());
    }
}
