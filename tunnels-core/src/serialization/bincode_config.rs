//! Deterministic bincode configuration.
//!
//! Uses fixed-size integer encoding and little-endian byte order
//! for consistent cross-platform serialization.

use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::SerializationError;

/// Get the deterministic bincode configuration.
///
/// Configuration:
/// - Fixed-size integer encoding (not variable-length)
/// - Little-endian byte order
/// - Reject trailing bytes on deserialization
fn config() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
}

/// Serialize a value to bytes using deterministic configuration.
///
/// The output is guaranteed to be identical for identical inputs
/// across all platforms.
pub fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, SerializationError> {
    config()
        .serialize(value)
        .map_err(|e| SerializationError::EncodeFailed(e.to_string()))
}

/// Deserialize a value from bytes.
///
/// Returns an error if:
/// - The bytes are malformed
/// - There are trailing bytes after the value
/// - The value doesn't match the expected type
pub fn deserialize<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, SerializationError> {
    config()
        .deserialize(bytes)
        .map_err(|e| SerializationError::DecodeFailed(e.to_string()))
}

/// Get the serialized size of a value without actually serializing it.
///
/// Useful for size estimation and validation.
pub fn serialized_size<T: Serialize>(value: &T) -> Result<u64, SerializationError> {
    config()
        .serialized_size(value)
        .map_err(|e| SerializationError::EncodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestStruct {
        a: u64,
        b: [u8; 20],
        c: Option<u32>,
    }

    #[test]
    fn test_roundtrip() {
        let original = TestStruct {
            a: 12345,
            b: [1u8; 20],
            c: Some(42),
        };

        let bytes = serialize(&original).unwrap();
        let recovered: TestStruct = deserialize(&bytes).unwrap();

        assert_eq!(original, recovered);
    }

    #[test]
    fn test_determinism() {
        let value = TestStruct {
            a: 999999,
            b: [2u8; 20],
            c: None,
        };

        let bytes1 = serialize(&value).unwrap();
        let bytes2 = serialize(&value).unwrap();

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_rejects_trailing_bytes() {
        let value = 42u64;
        let mut bytes = serialize(&value).unwrap();

        // Add trailing garbage
        bytes.push(0xFF);

        let result: Result<u64, _> = deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_fixed_int_encoding() {
        // With fixed int encoding, u64 should always be 8 bytes
        let small: u64 = 1;
        let large: u64 = u64::MAX;

        let small_bytes = serialize(&small).unwrap();
        let large_bytes = serialize(&large).unwrap();

        assert_eq!(small_bytes.len(), large_bytes.len());
        assert_eq!(small_bytes.len(), 8);
    }

    #[test]
    fn test_little_endian() {
        let value: u32 = 0x01020304;
        let bytes = serialize(&value).unwrap();

        // Little-endian: least significant byte first
        assert_eq!(bytes, vec![0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn test_serialized_size() {
        let value = TestStruct {
            a: 12345,
            b: [1u8; 20],
            c: Some(42),
        };

        let size = serialized_size(&value).unwrap();
        let actual = serialize(&value).unwrap().len() as u64;

        assert_eq!(size, actual);
    }

    #[test]
    fn test_invalid_bytes() {
        // Try to deserialize garbage as a structured type
        let garbage = vec![0xFF, 0xFF, 0xFF];
        let result: Result<TestStruct, _> = deserialize(&garbage);
        assert!(result.is_err());
    }

    #[test]
    fn test_vec_serialization() {
        let original: Vec<u64> = vec![1, 2, 3, 4, 5];
        let bytes = serialize(&original).unwrap();
        let recovered: Vec<u64> = deserialize(&bytes).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_nested_struct() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Outer {
            inner: TestStruct,
            extra: u8,
        }

        let original = Outer {
            inner: TestStruct {
                a: 100,
                b: [3u8; 20],
                c: Some(200),
            },
            extra: 255,
        };

        let bytes = serialize(&original).unwrap();
        let recovered: Outer = deserialize(&bytes).unwrap();

        assert_eq!(original, recovered);
    }
}
