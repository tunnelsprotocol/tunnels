//! Chain state management.
//!
//! This module provides chain state tracking including:
//! - Block storage and indexing
//! - Best chain tracking
//! - Fork detection and reorganization
//! - State caching for reorgs

mod stored_block;
mod state;
mod reorg;

pub use stored_block::StoredBlock;
pub use state::ChainState;
