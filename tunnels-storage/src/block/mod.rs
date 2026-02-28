//! Block storage and indexing.
//!
//! This module provides:
//! - `BlockStore`: Append-only storage for blocks
//! - `BlockIndex`: Efficient block lookups by height and hash

mod index;
mod store;

pub use index::BlockIndex;
pub use store::BlockStore;
