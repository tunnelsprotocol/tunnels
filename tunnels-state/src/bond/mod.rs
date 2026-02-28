//! Bond management module.
//!
//! Handles bond locking, unlocking, and forfeiture for attestations
//! and challenges.

mod ledger;

pub use ledger::{
    lock_attestation_bond,
    return_attestation_bond,
    forfeit_attestation_bond_to_challenger,
    lock_challenge_bond,
    return_challenge_bond,
    forfeit_challenge_bond_to_attestor,
};
