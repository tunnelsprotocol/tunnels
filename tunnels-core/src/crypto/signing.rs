//! Ed25519 signature creation and verification.

use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::keys::{PublicKey, SecretKey};
use crate::error::CryptoError;

/// Ed25519 signature wrapper with custom serialization.
///
/// Ensures deterministic binary serialization (raw 64 bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature(pub ed25519_dalek::Signature);

impl Signature {
    /// Create a Signature from raw bytes.
    pub fn from_bytes(bytes: &[u8; 64]) -> Self {
        Signature(ed25519_dalek::Signature::from_bytes(bytes))
    }

    /// Get the raw bytes of the signature.
    #[inline]
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }
}

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.to_bytes())
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SignatureVisitor;

        impl<'de> serde::de::Visitor<'de> for SignatureVisitor {
            type Value = Signature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("64 bytes")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Signature, E> {
                if v.len() != 64 {
                    return Err(E::invalid_length(v.len(), &self));
                }
                let bytes: [u8; 64] = v.try_into().unwrap();
                Ok(Signature::from_bytes(&bytes))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Signature, A::Error> {
                let mut bytes = [0u8; 64];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(Signature::from_bytes(&bytes))
            }
        }

        deserializer.deserialize_bytes(SignatureVisitor)
    }
}

/// Sign a message with a secret key.
///
/// Returns an Ed25519 signature that can be verified with the corresponding public key.
pub fn sign(secret_key: &SecretKey, message: &[u8]) -> Signature {
    Signature(secret_key.sign(message))
}

/// Verify a signature against a message and public key.
///
/// Returns Ok(()) if the signature is valid, or an error if verification fails.
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> Result<(), CryptoError> {
    public_key
        .inner()
        .verify(message, &signature.0)
        .map_err(|_| CryptoError::SignatureVerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;

    #[test]
    fn test_sign_verify_roundtrip() {
        let kp = KeyPair::generate();
        let message = b"test message";

        let signature = sign(kp.signing_key(), message);
        let result = verify(&kp.public_key(), message, &signature);

        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_wrong_message_fails() {
        let kp = KeyPair::generate();
        let message = b"test message";
        let wrong_message = b"wrong message";

        let signature = sign(kp.signing_key(), message);
        let result = verify(&kp.public_key(), wrong_message, &signature);

        assert!(result.is_err());
        assert!(matches!(result, Err(CryptoError::SignatureVerificationFailed)));
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let message = b"test message";

        let signature = sign(kp1.signing_key(), message);
        let result = verify(&kp2.public_key(), message, &signature);

        assert!(result.is_err());
    }

    #[test]
    fn test_signature_determinism() {
        let kp = KeyPair::generate();
        let message = b"test message";

        let sig1 = sign(kp.signing_key(), message);
        let sig2 = sign(kp.signing_key(), message);

        // Ed25519 signatures are deterministic
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_signature_serialization() {
        let kp = KeyPair::generate();
        let message = b"test message";
        let signature = sign(kp.signing_key(), message);

        // Serialize
        let bytes = bincode::serialize(&signature).unwrap();

        // Deserialize
        let recovered: Signature = bincode::deserialize(&bytes).unwrap();

        assert_eq!(signature, recovered);

        // Verify still works
        assert!(verify(&kp.public_key(), message, &recovered).is_ok());
    }

    #[test]
    fn test_signature_from_bytes() {
        let kp = KeyPair::generate();
        let message = b"test message";
        let signature = sign(kp.signing_key(), message);

        let bytes = signature.to_bytes();
        let recovered = Signature::from_bytes(&bytes);

        assert_eq!(signature, recovered);
    }
}
