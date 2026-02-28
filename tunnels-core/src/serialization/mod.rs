//! Deterministic binary serialization for the Tunnels protocol.
//!
//! All protocol data structures are serialized using bincode with a
//! deterministic configuration. This ensures:
//! - Same input always produces same output (for transaction IDs, block hashes)
//! - Cross-platform consistency
//! - Compact binary representation

mod bincode_config;

pub use bincode_config::{serialize, deserialize, serialized_size};
