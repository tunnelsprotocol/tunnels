//! Blockchain and chain management for the Tunnels protocol.
//!
//! This crate implements:
//! - Genesis block creation
//! - Block validation (11 rules)
//! - Block difficulty adjustment (every 2,016 blocks)
//! - Per-transaction difficulty adjustment
//! - Chain management with fork handling and reorganization
//! - Transaction mempool
//! - Block production
//!
//! # Example
//!
//! ```ignore
//! use tunnels_chain::{ChainState, create_genesis_block};
//!
//! let mut chain = ChainState::new();
//! let genesis = create_genesis_block();
//! assert_eq!(chain.tip_hash(), genesis.hash());
//! ```

mod error;
mod genesis;
mod validation;
mod difficulty;
mod chain;
mod mempool;
mod production;

pub use error::{ChainError, ChainResult};
pub use genesis::{create_genesis_block, GENESIS_BLOCK_HASH, GENESIS_DIFFICULTY, GENESIS_TIMESTAMP};
pub use validation::{
    validate_block, validate_block_with_state, validate_block_header, validate_transactions,
    compute_state_root, MAX_BLOCK_SIZE, MAX_FUTURE_BLOCK_TIME, MEDIAN_TIME_PAST_BLOCKS,
    PROTOCOL_VERSION,
};
pub use difficulty::{
    calculate_next_difficulty, DIFFICULTY_ADJUSTMENT_INTERVAL, TARGET_BLOCK_TIME_SECS,
    TARGET_TIMESPAN, MAX_DIFFICULTY_ADJUSTMENT,
};
pub use chain::{ChainState, StoredBlock};
pub use mempool::{Mempool, MempoolEntry};
pub use production::{BlockBuilder, produce_block};
