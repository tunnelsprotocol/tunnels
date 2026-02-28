//! Revenue router state types.
//!
//! These types track the state of revenue distribution using the
//! reward-index accumulator pattern for O(1) distribution.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::u256::U256;

/// Accumulator state for a project/denomination pair.
///
/// Tracks the global reward index and total units for O(1)
/// revenue distribution regardless of contributor count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccumulatorState {
    /// Cumulative reward-per-unit index (128.128 fixed-point).
    /// Increments with each deposit: index += deposit / total_units.
    pub reward_per_unit_global: U256,

    /// Total finalized attestation units for this project.
    pub total_finalized_units: u64,

    /// Labor pool deposits that arrived before any attestations finalized.
    /// Distributed when the first attestation finalizes.
    pub unallocated: U256,

    /// Reserve pool balance for this denomination.
    pub reserve_balance: U256,
}

impl AccumulatorState {
    /// Create a new empty accumulator state.
    pub fn new() -> Self {
        Self {
            reward_per_unit_global: U256::zero(),
            total_finalized_units: 0,
            unallocated: U256::zero(),
            reserve_balance: U256::zero(),
        }
    }

    /// Check if any attestations have been finalized.
    #[inline]
    pub fn has_finalized_units(&self) -> bool {
        self.total_finalized_units > 0
    }
}

impl Default for AccumulatorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-contributor balance for a project/denomination.
///
/// Tracks a contributor's units and their entry point into the
/// reward index for computing claimable amounts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributorBalance {
    /// Total finalized units for this contributor in this project.
    pub units: u64,

    /// Snapshot of global reward index when units last changed.
    /// Used to compute claimable: units * (global - entry).
    pub reward_per_unit_at_entry: U256,

    /// Total amount already claimed.
    pub claimed: U256,
}

impl ContributorBalance {
    /// Create a new empty contributor balance.
    pub fn new() -> Self {
        Self {
            units: 0,
            reward_per_unit_at_entry: U256::zero(),
            claimed: U256::zero(),
        }
    }
}

impl Default for ContributorBalance {
    fn default() -> Self {
        Self::new()
    }
}

/// Protocol-level state for self-adjusting per-transaction PoW difficulty.
///
/// Difficulty adjusts based on transaction volume to throttle spam
/// without requiring governance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionDifficultyState {
    /// Rolling window of per-block transaction counts.
    pub recent_tx_count: VecDeque<u64>,

    /// Current per-transaction PoW difficulty target.
    pub current_tx_difficulty: u64,
}

impl TransactionDifficultyState {
    /// Default window size for difficulty calculation.
    pub const DEFAULT_WINDOW_SIZE: usize = 100;

    /// Minimum difficulty (trivial PoW).
    pub const MIN_DIFFICULTY: u64 = 8;

    /// Maximum difficulty.
    pub const MAX_DIFFICULTY: u64 = 64;

    /// Target transactions per block.
    pub const TARGET_TX_PER_BLOCK: u64 = 100;

    /// Create a new difficulty state with initial difficulty.
    pub fn new(initial_difficulty: u64) -> Self {
        Self {
            recent_tx_count: VecDeque::new(),
            current_tx_difficulty: initial_difficulty,
        }
    }

    /// Record a block's transaction count and recalculate difficulty.
    pub fn record_block(&mut self, tx_count: u64, window_size: usize) {
        self.recent_tx_count.push_back(tx_count);

        // Trim to window size
        while self.recent_tx_count.len() > window_size {
            self.recent_tx_count.pop_front();
        }

        // Recalculate difficulty
        self.current_tx_difficulty = self.calculate_difficulty();
    }

    /// Calculate difficulty based on average transaction volume.
    fn calculate_difficulty(&self) -> u64 {
        if self.recent_tx_count.is_empty() {
            return self.current_tx_difficulty;
        }

        let total: u64 = self.recent_tx_count.iter().sum();
        let avg = total / self.recent_tx_count.len() as u64;

        // Scale difficulty based on how far we are from target
        let new_difficulty = if avg > Self::TARGET_TX_PER_BLOCK {
            // Above target: increase difficulty
            let ratio = avg / Self::TARGET_TX_PER_BLOCK;
            self.current_tx_difficulty.saturating_add(ratio)
        } else if avg < Self::TARGET_TX_PER_BLOCK / 2 {
            // Well below target: decrease difficulty
            self.current_tx_difficulty.saturating_sub(1)
        } else {
            self.current_tx_difficulty
        };

        new_difficulty.clamp(Self::MIN_DIFFICULTY, Self::MAX_DIFFICULTY)
    }
}

impl Default for TransactionDifficultyState {
    fn default() -> Self {
        Self::new(Self::MIN_DIFFICULTY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulator_state_new() {
        let state = AccumulatorState::new();
        assert!(state.reward_per_unit_global.is_zero());
        assert_eq!(state.total_finalized_units, 0);
        assert!(state.unallocated.is_zero());
        assert!(!state.has_finalized_units());
    }

    #[test]
    fn test_contributor_balance_new() {
        let balance = ContributorBalance::new();
        assert_eq!(balance.units, 0);
        assert!(balance.reward_per_unit_at_entry.is_zero());
        assert!(balance.claimed.is_zero());
    }

    #[test]
    fn test_tx_difficulty_increase() {
        let mut state = TransactionDifficultyState::new(10);

        // Record blocks with high tx count (above target)
        for _ in 0..10 {
            state.record_block(500, 10); // 5x target
        }

        // Difficulty should have increased
        assert!(state.current_tx_difficulty > 10);
    }

    #[test]
    fn test_tx_difficulty_decrease() {
        let mut state = TransactionDifficultyState::new(30);

        // Record blocks with low tx count
        for _ in 0..10 {
            state.record_block(10, 10); // Well below target/2
        }

        // Difficulty should have decreased (but not below min)
        assert!(state.current_tx_difficulty < 30);
        assert!(state.current_tx_difficulty >= TransactionDifficultyState::MIN_DIFFICULTY);
    }

    #[test]
    fn test_serialization() {
        let acc = AccumulatorState {
            reward_per_unit_global: U256::from(12345u64),
            total_finalized_units: 1000,
            unallocated: U256::from(500u64),
            reserve_balance: U256::from(250u64),
        };

        let bytes = crate::serialization::serialize(&acc).unwrap();
        let recovered: AccumulatorState = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(acc, recovered);
    }

}
