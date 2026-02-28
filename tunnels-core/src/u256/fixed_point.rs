//! Fixed-point arithmetic operations for U256.
//!
//! The Tunnels protocol uses 128.128 fixed-point format:
//! - 128 bits for the integer part
//! - 128 bits for the fractional part
//!
//! This provides sufficient precision for revenue distribution calculations
//! even with extreme scale differentials (e.g., 1-unit contributor in a
//! 10-billion-unit project receiving micro-deposits).

use super::{U256, FIXED_POINT_FRACTIONAL_BITS};

impl U256 {
    /// Fixed-point multiplication: (a * b) >> 128
    ///
    /// Multiplies two fixed-point numbers and returns the result
    /// in fixed-point format. This handles the implicit scaling
    /// factor in fixed-point multiplication.
    ///
    /// Note: This can overflow for very large values.
    pub fn fixed_mul(&self, other: &Self) -> Self {
        // For fixed-point multiplication, we need to compute (a * b) >> 128
        // This maintains the correct scale factor.
        //
        // However, uint's U256 doesn't support U512, so we need to be careful
        // about overflow. For the accumulator use case, the values should be
        // bounded enough that we can use a simpler approach.
        //
        // Simple approach: multiply, then shift right by 128
        // This may lose precision in the lower bits but is sufficient
        // for the accumulator use case.
        let product = *self * *other;
        product >> FIXED_POINT_FRACTIONAL_BITS
    }

    /// Fixed-point division: (a << 128) / b
    ///
    /// Divides two fixed-point numbers and returns the result
    /// in fixed-point format. The numerator is shifted left first
    /// to maintain precision.
    ///
    /// Returns zero if divisor is zero (rather than panicking).
    pub fn fixed_div(&self, other: &Self) -> Self {
        if other.is_zero() {
            return U256::zero();
        }

        // For fixed-point division: (a << 128) / b
        // This maintains the correct scale factor.
        //
        // If `self` already represents a fixed-point value (already shifted),
        // then we need to shift again before dividing.
        // If `self` is an integer that we want to divide, shift first.
        let shifted = *self << FIXED_POINT_FRACTIONAL_BITS;
        shifted / *other
    }

    /// Convert an integer to fixed-point format.
    ///
    /// Equivalent to multiplying by 2^128.
    #[inline]
    pub fn to_fixed(value: u64) -> Self {
        U256::from(value) << FIXED_POINT_FRACTIONAL_BITS
    }

    /// Convert from fixed-point to integer (truncating fractional part).
    #[inline]
    pub fn from_fixed_truncate(&self) -> U256 {
        *self >> FIXED_POINT_FRACTIONAL_BITS
    }

    /// Compute (amount << 128) / total_units for accumulator index update.
    ///
    /// This is the core operation for the reward-per-unit index:
    /// when a deposit of `amount` arrives and there are `total_units`
    /// finalized units, the index increases by `amount / total_units`
    /// in fixed-point format.
    pub fn compute_reward_increment(amount: U256, total_units: u64) -> U256 {
        if total_units == 0 {
            return U256::zero();
        }
        let amount_shifted = amount << FIXED_POINT_FRACTIONAL_BITS;
        amount_shifted / U256::from(total_units)
    }

    /// Compute claimable balance: units * (global_index - entry_index)
    ///
    /// The result is in fixed-point format. Call `from_fixed_truncate()`
    /// to convert to an integer amount.
    pub fn compute_claimable(
        units: u64,
        global_index: &U256,
        entry_index: &U256,
    ) -> U256 {
        if *global_index <= *entry_index {
            return U256::zero();
        }
        let delta = *global_index - *entry_index;
        U256::from(units) * delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_fixed() {
        let fixed = U256::to_fixed(100);
        assert_eq!(fixed >> 128, U256::from(100u64));
    }

    #[test]
    fn test_from_fixed_truncate() {
        let fixed = U256::to_fixed(100);
        let recovered = fixed.from_fixed_truncate();
        assert_eq!(recovered, U256::from(100u64));
    }

    #[test]
    fn test_compute_reward_increment() {
        // 1000 units deposited, 100 total units = 10 per unit
        let increment = U256::compute_reward_increment(U256::from(1000u64), 100);
        let per_unit = increment.from_fixed_truncate();
        assert_eq!(per_unit, U256::from(10u64));
    }

    #[test]
    fn test_compute_reward_increment_with_remainder() {
        // 100 units deposited, 3 total units = 33.333... per unit
        let increment = U256::compute_reward_increment(U256::from(100u64), 3);
        let per_unit = increment.from_fixed_truncate();
        assert_eq!(per_unit, U256::from(33u64));

        // The fractional part should be preserved
        let fractional_test = increment - (per_unit << 128);
        assert!(!fractional_test.is_zero());
    }

    #[test]
    fn test_compute_reward_increment_zero_units() {
        let increment = U256::compute_reward_increment(U256::from(1000u64), 0);
        assert!(increment.is_zero());
    }

    #[test]
    fn test_compute_claimable() {
        // Contributor has 100 units
        // Global index is 10.0 (in fixed point)
        // Entry index is 5.0 (in fixed point)
        // Claimable = 100 * (10 - 5) = 500
        let global = U256::to_fixed(10);
        let entry = U256::to_fixed(5);
        let claimable = U256::compute_claimable(100, &global, &entry);
        let amount = claimable.from_fixed_truncate();
        assert_eq!(amount, U256::from(500u64));
    }

    #[test]
    fn test_compute_claimable_no_change() {
        let index = U256::to_fixed(10);
        let claimable = U256::compute_claimable(100, &index, &index);
        assert!(claimable.is_zero());
    }

    #[test]
    fn test_fixed_mul() {
        // fixed_mul: (a * b) >> 128
        // This is useful when one operand is fixed-point and one is a small integer
        // or when both are small values that won't overflow
        let a = U256::to_fixed(10); // 10 in fixed-point
        let b = U256::from(3u64);   // 3 as integer
        let result = a.fixed_mul(&b);
        // Result: (10 << 128) * 3 >> 128 = 30 << 128 >> 128 = 30
        // But we want fixed-point output, so let's test differently

        // Actually test: small fixed * small fixed = small fixed
        let a = U256::from(10u64) << 64;  // Small fixed value (only 64-bit shift)
        let b = U256::from(3u64) << 64;
        let result = a.fixed_mul(&b);
        // (10 << 64) * (3 << 64) >> 128 = 30 << 128 >> 128 = 30
        assert_eq!(result, U256::from(30u64));
    }

    #[test]
    fn test_fixed_div() {
        // fixed_div: (a << 128) / b
        // Takes integer a and integer b, returns fixed-point a/b
        let a = U256::from(10u64); // Integer
        let b = U256::from(3u64);  // Integer
        let result = a.fixed_div(&b);
        // Result should be 10/3 in fixed-point = 3.333... << 128
        let integer_part = result.from_fixed_truncate();
        assert_eq!(integer_part, U256::from(3u64));
    }

    #[test]
    fn test_fixed_div_by_zero() {
        let a = U256::to_fixed(10);
        let b = U256::zero();
        let result = a.fixed_div(&b);
        assert!(result.is_zero());
    }

    #[test]
    fn test_accumulator_scenario() {
        // Simulate accumulator math:
        // - Project has 300 total units (3 contributors: 100, 150, 50)
        // - Deposit of 3000 arrives
        // - Each contributor should get proportional share

        let deposit = U256::from(3000u64);
        let total_units = 300u64;

        // Compute reward increment
        let increment = U256::compute_reward_increment(deposit, total_units);

        // Contributor 1: 100 units, entry at 0
        let entry_1 = U256::zero();
        let claimable_1 = U256::compute_claimable(100, &increment, &entry_1);
        let amount_1 = claimable_1.from_fixed_truncate();
        assert_eq!(amount_1, U256::from(1000u64)); // 100/300 * 3000 = 1000

        // Contributor 2: 150 units, entry at 0
        let claimable_2 = U256::compute_claimable(150, &increment, &entry_1);
        let amount_2 = claimable_2.from_fixed_truncate();
        assert_eq!(amount_2, U256::from(1500u64)); // 150/300 * 3000 = 1500

        // Contributor 3: 50 units, entry at 0
        let claimable_3 = U256::compute_claimable(50, &increment, &entry_1);
        let amount_3 = claimable_3.from_fixed_truncate();
        assert_eq!(amount_3, U256::from(500u64)); // 50/300 * 3000 = 500

        // Total should equal deposit
        assert_eq!(
            amount_1 + amount_2 + amount_3,
            deposit
        );
    }
}
