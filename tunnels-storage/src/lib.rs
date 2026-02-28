//! # Tunnels Storage
//!
//! Persistent storage with Merkle Patricia Trie (MPT) commitments for the Tunnels protocol.
//!
//! This crate provides:
//! - Disk-backed storage via RocksDB
//! - Merkle Patricia Trie for state commitments
//! - Inclusion/exclusion proofs for state entries
//! - Block storage with height/hash indexing
//! - Chain replay from genesis
//!
//! ## Architecture
//!
//! The storage layer implements the `StateReader` and `StateWriter` traits from
//! `tunnels-state`, allowing it to be used as a drop-in replacement for the
//! in-memory `ProtocolState`.
//!
//! State is stored in a RocksDB database with a Merkle Patricia Trie overlay
//! that enables cryptographic commitments to the full state. Each block commit
//! produces a state root that can be used to verify state integrity.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod keys;
pub mod kv;
pub mod trie;
pub mod state;
pub mod block;
pub mod replay;

pub use error::StorageError;
pub use keys::{KeyPrefix, StateKey};
pub use kv::{KvBackend, MemoryBackend, RocksBackend};
pub use trie::{MerkleTrie, TrieNode, Nibbles, MerkleProof};
pub use state::PersistentState;
pub use block::{BlockStore, BlockIndex};
pub use replay::ChainReplay;
