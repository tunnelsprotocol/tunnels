//! State container and storage traits.
//!
//! This module provides:
//! - [`StateReader`]: Read-only access to protocol state
//! - [`StateWriter`]: Mutable access to protocol state
//! - [`StateStore`]: Combined trait for full state access
//! - [`ProtocolState`]: In-memory HashMap-backed implementation

mod store;
mod protocol_state;

pub use store::{StateReader, StateWriter, StateStore};
pub use protocol_state::ProtocolState;
