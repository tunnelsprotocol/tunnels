// Allow manual assign operations - U256 doesn't implement AddAssign/SubAssign
#![allow(clippy::assign_op_pattern)]
// Allow functions with many parameters - transaction handlers need context
#![allow(clippy::too_many_arguments)]

//! State machine for the Tunnels protocol.
//!
//! This crate implements the complete state transition function. Given a current
//! state and a transaction, it produces the next state or an error. Every protocol
//! rule is enforced here, with no networking or persistence.
//!
//! # Key Components
//!
//! - [`ProtocolState`]: In-memory state container backed by HashMaps
//! - [`StateReader`]/[`StateWriter`]: Traits abstracting state access
//! - [`apply_transaction`]: Main entry point for executing transactions
//! - [`StateError`]: Comprehensive error type for validation failures
//!
//! # Example
//!
//! ```ignore
//! use tunnels_state::{ProtocolState, apply_transaction};
//! use tunnels_core::SignedTransaction;
//!
//! let mut state = ProtocolState::new();
//! let result = apply_transaction(&mut state, &signed_tx, block_timestamp);
//! ```

mod error;
mod state;
mod execute;
mod waterfall;
mod accumulator;
mod bond;

pub use error::{StateError, StateResult};
pub use state::{ProtocolState, StateReader, StateWriter, StateStore};
pub use execute::{apply_transaction, apply_transaction_with_context, ExecutionContext, DEFAULT_MIN_VERIFICATION_WINDOW, DEVNET_MIN_VERIFICATION_WINDOW};
