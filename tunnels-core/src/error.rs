//! Error types for the Tunnels core crate.

use std::fmt;

/// Top-level error type for tunnels-core operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    /// Cryptographic operation failed.
    Crypto(CryptoError),
    /// Serialization or deserialization failed.
    Serialization(SerializationError),
    /// Invalid denomination string.
    Denomination(DenominationError),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Crypto(e) => write!(f, "crypto error: {}", e),
            CoreError::Serialization(e) => write!(f, "serialization error: {}", e),
            CoreError::Denomination(e) => write!(f, "denomination error: {}", e),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<CryptoError> for CoreError {
    fn from(e: CryptoError) -> Self {
        CoreError::Crypto(e)
    }
}

impl From<SerializationError> for CoreError {
    fn from(e: SerializationError) -> Self {
        CoreError::Serialization(e)
    }
}

impl From<DenominationError> for CoreError {
    fn from(e: DenominationError) -> Self {
        CoreError::Denomination(e)
    }
}

/// Errors related to cryptographic operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CryptoError {
    /// The signature is malformed or invalid.
    InvalidSignature,
    /// The public key is malformed or invalid.
    InvalidPublicKey,
    /// The secret key is malformed or invalid.
    InvalidSecretKey,
    /// Signature verification failed (signature doesn't match message/key).
    SignatureVerificationFailed,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::InvalidSignature => write!(f, "invalid signature format"),
            CryptoError::InvalidPublicKey => write!(f, "invalid public key format"),
            CryptoError::InvalidSecretKey => write!(f, "invalid secret key format"),
            CryptoError::SignatureVerificationFailed => write!(f, "signature verification failed"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Errors related to serialization and deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SerializationError {
    /// Failed to encode data to bytes.
    EncodeFailed(String),
    /// Failed to decode data from bytes.
    DecodeFailed(String),
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializationError::EncodeFailed(msg) => write!(f, "encode failed: {}", msg),
            SerializationError::DecodeFailed(msg) => write!(f, "decode failed: {}", msg),
        }
    }
}

impl std::error::Error for SerializationError {}

/// Errors related to denomination string parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DenominationError {
    /// Denomination string exceeds 8 bytes.
    TooLong,
    /// Denomination string contains invalid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for DenominationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DenominationError::TooLong => write!(f, "denomination exceeds 8 bytes"),
            DenominationError::InvalidUtf8 => write!(f, "denomination contains invalid UTF-8"),
        }
    }
}

impl std::error::Error for DenominationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = CoreError::Crypto(CryptoError::InvalidSignature);
        assert!(e.to_string().contains("invalid signature"));

        let e = CoreError::Serialization(SerializationError::EncodeFailed("test".into()));
        assert!(e.to_string().contains("encode failed"));

        let e = CoreError::Denomination(DenominationError::TooLong);
        assert!(e.to_string().contains("exceeds 8 bytes"));
    }

    #[test]
    fn test_error_conversion() {
        let crypto_err = CryptoError::InvalidPublicKey;
        let core_err: CoreError = crypto_err.into();
        assert!(matches!(core_err, CoreError::Crypto(CryptoError::InvalidPublicKey)));
    }
}
