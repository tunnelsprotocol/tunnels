//! Storage error types.

use thiserror::Error;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum StorageError {
    /// RocksDB error.
    #[error("RocksDB error: {0}")]
    RocksDb(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Key not found.
    #[error("Key not found: {0}")]
    NotFound(String),

    /// Invalid key format.
    #[error("Invalid key format: {0}")]
    InvalidKey(String),

    /// Trie error.
    #[error("Trie error: {0}")]
    Trie(String),

    /// Block storage error.
    #[error("Block storage error: {0}")]
    Block(String),

    /// Invalid state.
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rocksdb::Error> for StorageError {
    fn from(e: rocksdb::Error) -> Self {
        StorageError::RocksDb(e.to_string())
    }
}

impl From<tunnels_core::SerializationError> for StorageError {
    fn from(e: tunnels_core::SerializationError) -> Self {
        StorageError::Serialization(e.to_string())
    }
}
