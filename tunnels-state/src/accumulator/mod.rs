//! Accumulator math module.
//!
//! The accumulator implements O(1) revenue distribution using
//! a reward-per-unit index with 128.128 fixed-point arithmetic.

mod math;

pub use math::{
    finalize_attestation_units,
    distribute_to_labor_pool,
    claim_labor_pool_revenue,
};
