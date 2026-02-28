//! Block difficulty adjustment.
//!
//! Implements Bitcoin-style difficulty adjustment:
//! - Adjustment every 2,016 blocks
//! - Target 60-second block time
//! - 4x maximum adjustment in either direction

use crate::chain::ChainState;
use crate::genesis::GENESIS_DIFFICULTY;

/// Number of blocks between difficulty adjustments.
pub const DIFFICULTY_ADJUSTMENT_INTERVAL: u64 = 2016;

/// Target block time in seconds (1 minute).
pub const TARGET_BLOCK_TIME_SECS: u64 = 60;

/// Target timespan for difficulty period (2016 blocks × 60 seconds).
pub const TARGET_TIMESPAN: u64 = DIFFICULTY_ADJUSTMENT_INTERVAL * TARGET_BLOCK_TIME_SECS;

/// Maximum difficulty adjustment factor (4x in either direction).
pub const MAX_DIFFICULTY_ADJUSTMENT: u64 = 4;

/// Calculate the expected difficulty for a given block height.
///
/// Difficulty is adjusted every 2,016 blocks based on how long
/// the previous 2,016 blocks took compared to the target.
///
/// # Algorithm
///
/// At adjustment boundaries (heights divisible by 2,016):
/// 1. Calculate actual time for previous 2,016 blocks
/// 2. Compare to target time (2,016 × 60 = 120,960 seconds)
/// 3. Adjust difficulty: `new = old × (target / actual)`
/// 4. Clamp adjustment to 4x in either direction
///
/// Between adjustment boundaries, difficulty stays constant.
pub fn calculate_next_difficulty(chain: &ChainState, height: u64) -> u64 {
    // Genesis block uses genesis difficulty
    if height == 0 {
        return GENESIS_DIFFICULTY;
    }

    // Height 1 and non-adjustment blocks use previous difficulty
    if height % DIFFICULTY_ADJUSTMENT_INTERVAL != 0 {
        // Use difficulty from the previous block
        if let Some(prev_block) = chain.get_block_at_height(height - 1) {
            return prev_block.block.header.difficulty;
        }
        // Fallback to tip difficulty
        return chain.tip_difficulty();
    }

    // Adjustment block: calculate new difficulty
    let period_end_height = height - 1;
    let period_start_height = height.saturating_sub(DIFFICULTY_ADJUSTMENT_INTERVAL);

    // Get timestamps for start and end of period
    let period_end = match chain.get_block_at_height(period_end_height) {
        Some(block) => block.block.header.timestamp,
        None => return chain.tip_difficulty(),
    };

    let period_start = match chain.get_block_at_height(period_start_height) {
        Some(block) => block.block.header.timestamp,
        None => return chain.tip_difficulty(),
    };

    // Get current difficulty
    let current_difficulty = match chain.get_block_at_height(period_end_height) {
        Some(block) => block.block.header.difficulty,
        None => return chain.tip_difficulty(),
    };

    // Calculate actual time for the period
    let actual_timespan = period_end.saturating_sub(period_start);

    // Prevent division by zero
    if actual_timespan == 0 {
        return current_difficulty;
    }

    // Calculate new difficulty
    // new_difficulty = current_difficulty × (TARGET_TIMESPAN / actual_timespan)
    //
    // If blocks came faster than target (actual < target):
    //   ratio > 1, difficulty increases
    // If blocks came slower than target (actual > target):
    //   ratio < 1, difficulty decreases
    //
    // To avoid floating point, we compute:
    // new_difficulty = (current_difficulty × TARGET_TIMESPAN) / actual_timespan

    // Clamp actual timespan to prevent extreme adjustments
    let clamped_timespan = actual_timespan.clamp(
        TARGET_TIMESPAN / MAX_DIFFICULTY_ADJUSTMENT,
        TARGET_TIMESPAN * MAX_DIFFICULTY_ADJUSTMENT,
    );

    // Calculate new difficulty using u128 to prevent overflow
    let new_difficulty = (current_difficulty as u128 * TARGET_TIMESPAN as u128)
        / clamped_timespan as u128;

    // Clamp to valid range (minimum 1)
    new_difficulty.max(1) as u64
}

/// Calculate difficulty adjustment ratio for debugging/logging.
///
/// Returns (actual_timespan, expected_timespan, adjustment_ratio_percent)
#[allow(dead_code)]
pub fn difficulty_adjustment_info(
    chain: &ChainState,
    height: u64,
) -> Option<(u64, u64, i64)> {
    if height % DIFFICULTY_ADJUSTMENT_INTERVAL != 0 || height == 0 {
        return None;
    }

    let period_end_height = height - 1;
    let period_start_height = height.saturating_sub(DIFFICULTY_ADJUSTMENT_INTERVAL);

    let period_end = chain.get_block_at_height(period_end_height)?.block.header.timestamp;
    let period_start = chain.get_block_at_height(period_start_height)?.block.header.timestamp;

    let actual_timespan = period_end.saturating_sub(period_start);

    // Ratio as percentage: (target / actual - 1) × 100
    // Positive means difficulty increases, negative means decrease
    let ratio = if actual_timespan > 0 {
        ((TARGET_TIMESPAN as i128 - actual_timespan as i128) * 100 / actual_timespan as i128) as i64
    } else {
        100 // Max increase if timespan is 0
    };

    Some((actual_timespan, TARGET_TIMESPAN, ratio))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require ChainState, which will be tested
    // in the chain module. Here we test the math directly.

    #[test]
    fn test_constants() {
        // Verify expected values
        assert_eq!(DIFFICULTY_ADJUSTMENT_INTERVAL, 2016);
        assert_eq!(TARGET_BLOCK_TIME_SECS, 60);
        assert_eq!(TARGET_TIMESPAN, 2016 * 60);
        assert_eq!(TARGET_TIMESPAN, 120_960);
        assert_eq!(MAX_DIFFICULTY_ADJUSTMENT, 4);
    }

    #[test]
    fn test_adjustment_math() {
        // Test the clamping logic manually
        let current_diff: u64 = 16;

        // Perfect timing: difficulty stays same
        let actual = TARGET_TIMESPAN;
        let new = (current_diff as u128 * TARGET_TIMESPAN as u128 / actual as u128) as u64;
        assert_eq!(new, current_diff);

        // 2x faster: difficulty should double
        let actual = TARGET_TIMESPAN / 2;
        let new = (current_diff as u128 * TARGET_TIMESPAN as u128 / actual as u128) as u64;
        assert_eq!(new, current_diff * 2);

        // 2x slower: difficulty should halve
        let actual = TARGET_TIMESPAN * 2;
        let new = (current_diff as u128 * TARGET_TIMESPAN as u128 / actual as u128) as u64;
        assert_eq!(new, current_diff / 2);
    }

    #[test]
    fn test_clamping() {
        let current_diff: u64 = 16;

        // 10x faster: clamped to 4x increase
        let actual = TARGET_TIMESPAN / 10;
        let clamped = actual.max(TARGET_TIMESPAN / MAX_DIFFICULTY_ADJUSTMENT);
        let new = (current_diff as u128 * TARGET_TIMESPAN as u128 / clamped as u128) as u64;
        assert_eq!(new, current_diff * 4);

        // 10x slower: clamped to 4x decrease
        let actual = TARGET_TIMESPAN * 10;
        let clamped = actual.min(TARGET_TIMESPAN * MAX_DIFFICULTY_ADJUSTMENT);
        let new = (current_diff as u128 * TARGET_TIMESPAN as u128 / clamped as u128) as u64;
        assert_eq!(new, current_diff / 4);
    }
}
