//! Ed25519 key pair generation and management.

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::CryptoError;

/// Type alias for Ed25519 secret/signing key.
pub type SecretKey = SigningKey;

/// Ed25519 public key wrapper with custom serialization.
///
/// The wrapper ensures deterministic binary serialization (raw bytes)
/// rather than the default hex string format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(pub VerifyingKey);

impl PublicKey {
    /// Create a PublicKey from raw bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, CryptoError> {
        VerifyingKey::from_bytes(bytes)
            .map(PublicKey)
            .map_err(|_| CryptoError::InvalidPublicKey)
    }

    /// Get the raw bytes of the public key.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Get the inner VerifyingKey.
    #[inline]
    pub fn inner(&self) -> &VerifyingKey {
        &self.0
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<VerifyingKey> for PublicKey {
    fn from(key: VerifyingKey) -> Self {
        PublicKey(key)
    }
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.as_bytes())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PublicKeyVisitor;

        impl<'de> serde::de::Visitor<'de> for PublicKeyVisitor {
            type Value = PublicKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("32 bytes")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<PublicKey, E> {
                if v.len() != 32 {
                    return Err(E::invalid_length(v.len(), &self));
                }
                let bytes: [u8; 32] = v.try_into().unwrap();
                PublicKey::from_bytes(&bytes).map_err(E::custom)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<PublicKey, A::Error> {
                let mut bytes = [0u8; 32];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                PublicKey::from_bytes(&bytes).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_bytes(PublicKeyVisitor)
    }
}

/// Ed25519 key pair for identity management.
///
/// Contains both the secret (signing) key and the public (verifying) key.
/// The secret key should be kept secure and never transmitted.
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    /// Generate a new random key pair using the OS random number generator.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        KeyPair { signing_key }
    }

    /// Create a key pair from a 32-byte secret key.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, CryptoError> {
        let signing_key = SigningKey::from_bytes(bytes);
        Ok(KeyPair { signing_key })
    }

    /// Get the public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.signing_key.verifying_key())
    }

    /// Get the signing (secret) key.
    ///
    /// Use with caution - the secret key should be kept secure.
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Get the raw bytes of the secret key.
    ///
    /// Use with extreme caution - exposing these bytes compromises the identity.
    pub fn secret_bytes(&self) -> &[u8; 32] {
        self.signing_key.as_bytes()
    }
}

impl Clone for KeyPair {
    fn clone(&self) -> Self {
        KeyPair {
            signing_key: SigningKey::from_bytes(self.signing_key.as_bytes()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let kp = KeyPair::generate();
        assert_eq!(kp.public_key().as_bytes().len(), 32);
        assert_eq!(kp.secret_bytes().len(), 32);
    }

    #[test]
    fn test_key_generation_uniqueness() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        assert_ne!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
    }

    #[test]
    fn test_keypair_from_bytes() {
        let kp1 = KeyPair::generate();
        let bytes = kp1.secret_bytes();

        let kp2 = KeyPair::from_bytes(bytes).unwrap();
        assert_eq!(kp1.public_key(), kp2.public_key());
    }

    #[test]
    fn test_public_key_serialization() {
        let kp = KeyPair::generate();
        let pk = kp.public_key();

        // Serialize
        let bytes = bincode::serialize(&pk).unwrap();

        // Deserialize
        let recovered: PublicKey = bincode::deserialize(&bytes).unwrap();

        assert_eq!(pk, recovered);
    }

    #[test]
    fn test_keypair_clone() {
        let kp1 = KeyPair::generate();
        let kp2 = kp1.clone();
        assert_eq!(kp1.public_key(), kp2.public_key());
        assert_eq!(kp1.secret_bytes(), kp2.secret_bytes());
    }
}
