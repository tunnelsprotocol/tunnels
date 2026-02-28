//! Transaction execution module.
//!
//! This module contains the transaction executor and all transaction-specific
//! validation and execution logic.

mod context;
mod executor;
mod identity;
mod project;
mod attestation;
mod challenge;
mod revenue;

pub use context::{ExecutionContext, DEFAULT_MIN_VERIFICATION_WINDOW, DEVNET_MIN_VERIFICATION_WINDOW};
pub use executor::{apply_transaction, apply_transaction_with_context};

// Re-export transaction handlers for testing within the crate.
// These are used by test modules but clippy doesn't track pub(crate) usage in tests.
#[allow(unused_imports)]
pub(crate) use identity::{
    execute_create_identity,
    execute_create_agent,
    execute_update_profile_hash,
    execute_deactivate_agent,
};
#[allow(unused_imports)]
pub(crate) use project::execute_register_project;
#[allow(unused_imports)]
pub(crate) use project::MIN_VERIFICATION_WINDOW;
