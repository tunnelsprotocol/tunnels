//! Block structure for the Tunnels protocol.
//!
//! Blocks contain ordered lists of transactions and commit to
//! the state via Merkle roots.

#[allow(clippy::module_inception)]
mod block;
mod header;

pub use block::Block;
pub use header::BlockHeader;
