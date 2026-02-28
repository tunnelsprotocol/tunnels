//! 256-bit unsigned integer arithmetic for fixed-point accumulator math.
//!
//! The Tunnels protocol uses 256-bit fixed-point arithmetic with 128 integer bits
//! and 128 fractional bits for the reward-index accumulator. This provides sufficient
//! precision to avoid meaningful rounding dust even at extreme scale differentials.

// Allow clippy warnings from the uint crate's construct_uint macro
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::assign_op_pattern)]

mod fixed_point;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer.
    ///
    /// Used for:
    /// - Fixed-point accumulator math (128.128 format)
    /// - Bond amounts
    /// - Revenue amounts
    /// - Balance tracking
    pub struct U256(4);
}

/// Number of fractional bits in fixed-point representation.
pub const FIXED_POINT_FRACTIONAL_BITS: u32 = 128;

impl U256 {
    /// Create a U256 from a u64 value.
    #[inline]
    pub const fn from_u64(value: u64) -> Self {
        U256([value, 0, 0, 0])
    }

    /// Create a U256 from a u128 value.
    #[inline]
    pub fn from_u128(value: u128) -> Self {
        U256([value as u64, (value >> 64) as u64, 0, 0])
    }

    /// Convert to u64, returning None if the value doesn't fit.
    #[inline]
    pub fn to_u64(&self) -> Option<u64> {
        if self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0 {
            Some(self.0[0])
        } else {
            None
        }
    }

    /// Convert to u128, returning None if the value doesn't fit.
    #[inline]
    pub fn to_u128(&self) -> Option<u128> {
        if self.0[2] == 0 && self.0[3] == 0 {
            Some((self.0[1] as u128) << 64 | self.0[0] as u128)
        } else {
            None
        }
    }

    /// Convert a u64 to fixed-point format (shift left by 128 bits).
    #[inline]
    pub fn from_u64_fixed(value: u64) -> Self {
        U256([0, 0, value, 0])
    }

    /// Convert from fixed-point to u64 (shift right by 128 bits, truncating).
    #[inline]
    pub fn to_u64_truncated(&self) -> u64 {
        self.0[2]
    }

    /// Convert from fixed-point to u128 (shift right by 128 bits, truncating).
    #[inline]
    pub fn to_u128_truncated(&self) -> u128 {
        (self.0[3] as u128) << 64 | self.0[2] as u128
    }

    /// Serialize to big-endian bytes.
    pub fn to_be_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&self.0[3].to_be_bytes());
        bytes[8..16].copy_from_slice(&self.0[2].to_be_bytes());
        bytes[16..24].copy_from_slice(&self.0[1].to_be_bytes());
        bytes[24..32].copy_from_slice(&self.0[0].to_be_bytes());
        bytes
    }

    /// Deserialize from big-endian bytes.
    pub fn from_be_bytes(bytes: &[u8; 32]) -> Self {
        U256([
            u64::from_be_bytes(bytes[24..32].try_into().unwrap()),
            u64::from_be_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
            u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
        ])
    }

    /// Serialize to little-endian bytes.
    pub fn to_le_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&self.0[0].to_le_bytes());
        bytes[8..16].copy_from_slice(&self.0[1].to_le_bytes());
        bytes[16..24].copy_from_slice(&self.0[2].to_le_bytes());
        bytes[24..32].copy_from_slice(&self.0[3].to_le_bytes());
        bytes
    }

    /// Deserialize from little-endian bytes.
    pub fn from_le_bytes(bytes: &[u8; 32]) -> Self {
        U256([
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        ])
    }
}

// Custom serde implementation for deterministic serialization
impl Serialize for U256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize as little-endian bytes for consistency with bincode
        serializer.serialize_bytes(&self.to_le_bytes())
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct U256Visitor;

        impl<'de> serde::de::Visitor<'de> for U256Visitor {
            type Value = U256;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("32 bytes")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<U256, E> {
                if v.len() != 32 {
                    return Err(E::invalid_length(v.len(), &self));
                }
                let bytes: [u8; 32] = v.try_into().unwrap();
                Ok(U256::from_le_bytes(&bytes))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<U256, A::Error> {
                let mut bytes = [0u8; 32];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(U256::from_le_bytes(&bytes))
            }
        }

        deserializer.deserialize_bytes(U256Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_arithmetic() {
        let a = U256::from(100u64);
        let b = U256::from(50u64);
        assert_eq!(a + b, U256::from(150u64));
        assert_eq!(a - b, U256::from(50u64));
        assert_eq!(a * b, U256::from(5000u64));
        assert_eq!(a / b, U256::from(2u64));
    }

    #[test]
    fn test_shift_operations() {
        let a = U256::from(1u64);
        let shifted = a << 128;
        assert_eq!(shifted >> 128, a);
    }

    #[test]
    fn test_fixed_point_conversion() {
        let value = U256::from_u64_fixed(1_000_000);
        assert_eq!(value.to_u64_truncated(), 1_000_000);
    }

    #[test]
    fn test_acceptance_criterion_4() {
        // From acceptance criteria: 1_000_000 << 128 / 7 should have 128-bit precision
        let numerator = U256::from(1_000_000u64) << 128;
        let divisor = U256::from(7u64);
        let result = numerator / divisor;

        // Verify precision: result should be approximately 142857.142857... << 128
        // The integer part is 142857
        let integer_part = result >> 128;
        assert_eq!(integer_part, U256::from(142857u64));

        // The fractional part should be non-zero (0.142857... in fixed point)
        let fractional_part = result - (integer_part << 128);
        assert!(!fractional_part.is_zero());
    }

    #[test]
    fn test_byte_serialization() {
        let value = U256::from(0x123456789ABCDEFu64);
        let le_bytes = value.to_le_bytes();
        let recovered = U256::from_le_bytes(&le_bytes);
        assert_eq!(value, recovered);

        let be_bytes = value.to_be_bytes();
        let recovered = U256::from_be_bytes(&be_bytes);
        assert_eq!(value, recovered);
    }

    #[test]
    fn test_from_u64() {
        let value = U256::from_u64(12345);
        assert_eq!(value.to_u64(), Some(12345));
    }

    #[test]
    fn test_from_u128() {
        let value = U256::from_u128(u128::MAX);
        assert_eq!(value.to_u128(), Some(u128::MAX));
    }

    #[test]
    fn test_large_value_to_u64_fails() {
        let value = U256::from(1u64) << 128;
        assert_eq!(value.to_u64(), None);
    }

    #[test]
    fn test_comparison() {
        let a = U256::from(100u64);
        let b = U256::from(50u64);
        assert!(a > b);
        assert!(b < a);
        assert!(a >= a);
        assert!(a <= a);
        assert!(a != b);
    }
}
