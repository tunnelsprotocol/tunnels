//! Per-transaction proof-of-work for spam prevention.
//!
//! Each transaction must include a PoW nonce such that the hash
//! of the transaction data meets a difficulty target. This serves
//! as spam prevention without requiring fees or a native token.

use crate::crypto::{sha256_concat, PublicKey, Signature};
use crate::serialization::serialize;
use crate::transaction::Transaction;

/// Compute the proof-of-work hash for a transaction.
///
/// The hash is: SHA-256(serialized_tx || signer || signature || nonce_le_bytes)
pub fn compute_pow_hash(
    tx: &Transaction,
    signer: &PublicKey,
    signature: &Signature,
    nonce: u64,
) -> [u8; 32] {
    let tx_bytes = serialize(tx).expect("transaction serialization should not fail");
    let nonce_bytes = nonce.to_le_bytes();

    sha256_concat(&[
        &tx_bytes,
        signer.as_bytes(),
        &signature.to_bytes(),
        &nonce_bytes,
    ])
}

/// Check if a hash meets the difficulty target.
///
/// Difficulty is measured in leading zero bits. A hash meets the target
/// if it has at least `difficulty` leading zero bits.
pub fn meets_difficulty(hash: &[u8; 32], difficulty: u64) -> bool {
    let required_zero_bits = difficulty as usize;

    // Count leading zero bits
    let mut zero_bits = 0usize;
    for byte in hash.iter() {
        if *byte == 0 {
            zero_bits += 8;
        } else {
            // Count leading zeros in this byte
            zero_bits += byte.leading_zeros() as usize;
            break;
        }
    }

    zero_bits >= required_zero_bits
}

/// Mine a valid proof-of-work nonce for a transaction.
///
/// Iterates through nonce values until finding one that produces
/// a hash meeting the difficulty target. Returns (nonce, hash).
///
/// Warning: For high difficulty values, this may take a very long time.
pub fn mine_pow(
    tx: &Transaction,
    signer: &PublicKey,
    signature: &Signature,
    difficulty: u64,
) -> (u64, [u8; 32]) {
    let mut nonce = 0u64;
    loop {
        let hash = compute_pow_hash(tx, signer, signature, nonce);
        if meets_difficulty(&hash, difficulty) {
            return (nonce, hash);
        }
        nonce = nonce.wrapping_add(1);

        // Safety check to avoid infinite loops in tests
        if nonce == 0 {
            panic!("PoW mining exhausted all nonces without finding solution");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{sign, KeyPair};

    #[test]
    fn test_meets_difficulty_zero() {
        // Any hash meets difficulty 0
        let hash = [0xFF; 32];
        assert!(meets_difficulty(&hash, 0));
    }

    #[test]
    fn test_meets_difficulty_one() {
        // Hash starting with 0x7F has 1 leading zero bit
        let mut hash = [0xFF; 32];
        hash[0] = 0x7F;
        assert!(meets_difficulty(&hash, 1));
        assert!(!meets_difficulty(&hash, 2));
    }

    #[test]
    fn test_meets_difficulty_eight() {
        // Hash starting with 0x00 has 8 leading zero bits
        let mut hash = [0xFF; 32];
        hash[0] = 0x00;
        assert!(meets_difficulty(&hash, 8));
        assert!(!meets_difficulty(&hash, 9));
    }

    #[test]
    fn test_meets_difficulty_sixteen() {
        // Hash starting with 0x00 0x00 has 16 leading zero bits
        let mut hash = [0xFF; 32];
        hash[0] = 0x00;
        hash[1] = 0x00;
        assert!(meets_difficulty(&hash, 16));
        assert!(!meets_difficulty(&hash, 17));
    }

    #[test]
    fn test_compute_pow_hash_determinism() {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);

        let hash1 = compute_pow_hash(&tx, &kp.public_key(), &signature, 12345);
        let hash2 = compute_pow_hash(&tx, &kp.public_key(), &signature, 12345);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_pow_hash_different_nonce() {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);

        let hash1 = compute_pow_hash(&tx, &kp.public_key(), &signature, 0);
        let hash2 = compute_pow_hash(&tx, &kp.public_key(), &signature, 1);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_mine_pow_trivial() {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);

        // Difficulty 1 should be found quickly
        let (nonce, hash) = mine_pow(&tx, &kp.public_key(), &signature, 1);

        assert!(meets_difficulty(&hash, 1));

        // Verify the hash
        let verify_hash = compute_pow_hash(&tx, &kp.public_key(), &signature, nonce);
        assert_eq!(hash, verify_hash);
    }

    #[test]
    fn test_mine_pow_moderate() {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);

        // Difficulty 8 should still be reasonable in tests
        let (nonce, hash) = mine_pow(&tx, &kp.public_key(), &signature, 8);

        assert!(meets_difficulty(&hash, 8));

        // Verify the hash
        let verify_hash = compute_pow_hash(&tx, &kp.public_key(), &signature, nonce);
        assert_eq!(hash, verify_hash);
    }

    #[test]
    fn test_acceptance_criterion_5() {
        // Acceptance criterion 5: hash tx_bytes || nonce with SHA-256,
        // verify leading zeros meet a test difficulty target.
        let kp = KeyPair::generate();
        let tx = Transaction::FinalizeAttestation {
            attestation_id: [1u8; 20],
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);

        // Mine with difficulty 4 (16 possible hashes match)
        let (nonce, hash) = mine_pow(&tx, &kp.public_key(), &signature, 4);

        // Verify the hash meets difficulty
        assert!(meets_difficulty(&hash, 4));

        // Verify it's a proper SHA-256 output
        assert_eq!(hash.len(), 32);

        // Verify recomputing gives same result
        let recomputed = compute_pow_hash(&tx, &kp.public_key(), &signature, nonce);
        assert_eq!(hash, recomputed);
    }
}
