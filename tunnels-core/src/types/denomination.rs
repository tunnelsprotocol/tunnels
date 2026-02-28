//! Denomination type for revenue tracking.
//!
//! A denomination is an 8-byte string tag representing a currency
//! or unit of account (e.g., "USD", "BTC", "USDC", "EUR").

use crate::error::DenominationError;

/// 8-byte denomination tag.
///
/// Denominations identify the currency or unit of account for
/// revenue deposits and distributions. Examples: "USD", "BTC", "EUR".
pub type Denomination = [u8; 8];

/// Create a denomination from a string.
///
/// The string must be 8 bytes or less. Shorter strings are
/// padded with null bytes.
pub fn denomination_from_str(s: &str) -> Result<Denomination, DenominationError> {
    let bytes = s.as_bytes();
    if bytes.len() > 8 {
        return Err(DenominationError::TooLong);
    }

    let mut denom = [0u8; 8];
    denom[..bytes.len()].copy_from_slice(bytes);
    Ok(denom)
}

/// Convert a denomination to a string.
///
/// Trailing null bytes are stripped.
pub fn denomination_to_string(denom: &Denomination) -> String {
    // Find the first null byte or end of array
    let end = denom.iter().position(|&b| b == 0).unwrap_or(8);
    String::from_utf8_lossy(&denom[..end]).into_owned()
}

/// Common denominations for convenience.
pub mod common {
    use super::Denomination;

    /// US Dollar
    pub const USD: Denomination = [b'U', b'S', b'D', 0, 0, 0, 0, 0];

    /// Bitcoin
    pub const BTC: Denomination = [b'B', b'T', b'C', 0, 0, 0, 0, 0];

    /// Ethereum
    pub const ETH: Denomination = [b'E', b'T', b'H', 0, 0, 0, 0, 0];

    /// USDC Stablecoin
    pub const USDC: Denomination = [b'U', b'S', b'D', b'C', 0, 0, 0, 0];

    /// Euro
    pub const EUR: Denomination = [b'E', b'U', b'R', 0, 0, 0, 0, 0];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_denomination_from_str() {
        let denom = denomination_from_str("USD").unwrap();
        assert_eq!(&denom[..3], b"USD");
        assert_eq!(&denom[3..], &[0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_denomination_from_str_max_length() {
        let denom = denomination_from_str("12345678").unwrap();
        assert_eq!(&denom, b"12345678");
    }

    #[test]
    fn test_denomination_from_str_too_long() {
        let result = denomination_from_str("123456789");
        assert!(matches!(result, Err(DenominationError::TooLong)));
    }

    #[test]
    fn test_denomination_to_string() {
        let denom = [b'U', b'S', b'D', 0, 0, 0, 0, 0];
        assert_eq!(denomination_to_string(&denom), "USD");
    }

    #[test]
    fn test_denomination_to_string_full() {
        let denom = *b"USDC1234";
        assert_eq!(denomination_to_string(&denom), "USDC1234");
    }

    #[test]
    fn test_roundtrip() {
        let original = "EUR";
        let denom = denomination_from_str(original).unwrap();
        let recovered = denomination_to_string(&denom);
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_common_denominations() {
        assert_eq!(denomination_to_string(&common::USD), "USD");
        assert_eq!(denomination_to_string(&common::BTC), "BTC");
        assert_eq!(denomination_to_string(&common::ETH), "ETH");
        assert_eq!(denomination_to_string(&common::USDC), "USDC");
        assert_eq!(denomination_to_string(&common::EUR), "EUR");
    }

    #[test]
    fn test_serialization() {
        let denom = denomination_from_str("USD").unwrap();
        let bytes = crate::serialization::serialize(&denom).unwrap();
        let recovered: Denomination = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(denom, recovered);
    }
}
