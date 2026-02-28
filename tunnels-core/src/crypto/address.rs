//! Identity address derivation.
//!
//! An identity address is the first 20 bytes of the SHA-256 hash
//! of the Ed25519 public key. This gives 160-bit addresses (same
//! length as Bitcoin/Ethereum), keeps addresses compact, and avoids
//! exposing the full public key until the identity signs a transaction.

use super::hashing::sha256;
use super::keys::PublicKey;

/// Derive an identity address from a public key.
///
/// The address is the first 20 bytes of SHA-256(public_key_bytes).
pub fn derive_address(public_key: &PublicKey) -> [u8; 20] {
    let hash = sha256(public_key.as_bytes());
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;

    #[test]
    fn test_address_length() {
        let kp = KeyPair::generate();
        let address = derive_address(&kp.public_key());
        assert_eq!(address.len(), 20);
    }

    #[test]
    fn test_address_determinism() {
        let kp = KeyPair::generate();
        let addr1 = derive_address(&kp.public_key());
        let addr2 = derive_address(&kp.public_key());
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_different_keys_different_addresses() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let addr1 = derive_address(&kp1.public_key());
        let addr2 = derive_address(&kp2.public_key());
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn test_address_is_first_20_bytes_of_hash() {
        let kp = KeyPair::generate();
        let full_hash = sha256(kp.public_key().as_bytes());
        let address = derive_address(&kp.public_key());

        assert_eq!(&full_hash[..20], &address[..]);
    }
}
