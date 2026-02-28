//! Waterfall revenue distribution module.
//!
//! The waterfall executes the revenue distribution algorithm:
//! 1. Predecessor slice (delta%) -> recursive deposit
//! 2. Reserve slice (rho%) -> reserve_balance
//! 3. Fee slice (phi%) -> phi_recipient claimable
//! 4. Genesis slice (gamma%) -> identity or recursive project
//! 5. Labor pool (remainder) -> accumulator or unallocated

mod router;

pub use router::execute_waterfall;
