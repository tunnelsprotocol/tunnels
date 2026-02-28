//! Signed transaction wrapper.

use serde::{Deserialize, Serialize};

use crate::crypto::{sha256, verify, PublicKey, Signature};
use crate::error::CryptoError;
use crate::serialization::serialize;
use crate::transaction::{compute_pow_hash, meets_difficulty, Transaction};

/// A signed transaction with proof-of-work.
///
/// Every transaction in the Tunnels protocol must be signed by
/// the appropriate identity and include a valid proof-of-work nonce.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedTransaction {
    /// The transaction payload.
    pub tx: Transaction,

    /// The public key of the signer.
    pub signer: PublicKey,

    /// Ed25519 signature of the serialized transaction.
    pub signature: Signature,

    /// Proof-of-work nonce.
    pub pow_nonce: u64,

    /// Proof-of-work hash (SHA-256 of tx || signer || signature || nonce).
    pub pow_hash: [u8; 32],
}

impl SignedTransaction {
    /// Compute the transaction ID.
    ///
    /// The ID is the SHA-256 hash of the serialized SignedTransaction.
    pub fn id(&self) -> [u8; 32] {
        let bytes = serialize(self).expect("SignedTransaction serialization should not fail");
        sha256(&bytes)
    }

    /// Verify the signature.
    ///
    /// Checks that the signature was created by the signer's private key
    /// over the serialized transaction payload.
    pub fn verify_signature(&self) -> Result<(), CryptoError> {
        let tx_bytes = serialize(&self.tx).expect("transaction serialization should not fail");
        verify(&self.signer, &tx_bytes, &self.signature)
    }

    /// Verify the proof-of-work.
    ///
    /// Checks that:
    /// 1. The pow_hash matches the computed hash
    /// 2. The hash meets the difficulty target
    pub fn verify_pow(&self, difficulty: u64) -> bool {
        // Recompute the hash
        let computed_hash = compute_pow_hash(&self.tx, &self.signer, &self.signature, self.pow_nonce);

        // Check hash matches
        if computed_hash != self.pow_hash {
            return false;
        }

        // Check difficulty
        meets_difficulty(&self.pow_hash, difficulty)
    }

    /// Fully verify the signed transaction.
    ///
    /// Verifies both signature and proof-of-work.
    pub fn verify(&self, difficulty: u64) -> Result<(), SignedTransactionError> {
        self.verify_signature()
            .map_err(|_| SignedTransactionError::InvalidSignature)?;

        if !self.verify_pow(difficulty) {
            return Err(SignedTransactionError::InvalidPow);
        }

        Ok(())
    }
}

/// Errors that can occur when verifying a signed transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignedTransactionError {
    /// Signature verification failed.
    InvalidSignature,
    /// Proof-of-work verification failed.
    InvalidPow,
}

impl std::fmt::Display for SignedTransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignedTransactionError::InvalidSignature => write!(f, "invalid signature"),
            SignedTransactionError::InvalidPow => write!(f, "invalid proof-of-work"),
        }
    }
}

impl std::error::Error for SignedTransactionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{sign, KeyPair};
    use crate::transaction::mine_pow;

    fn create_signed_tx(difficulty: u64) -> SignedTransaction {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &kp.public_key(), &signature, difficulty);

        SignedTransaction {
            tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        }
    }

    #[test]
    fn test_signed_transaction_id_determinism() {
        let signed_tx = create_signed_tx(1);

        let id1 = signed_tx.id();
        let id2 = signed_tx.id();

        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 32);
    }

    #[test]
    fn test_signed_transaction_verify_signature() {
        let signed_tx = create_signed_tx(1);
        assert!(signed_tx.verify_signature().is_ok());
    }

    #[test]
    fn test_signed_transaction_verify_signature_wrong_signer() {
        let mut signed_tx = create_signed_tx(1);

        // Replace signer with different key
        let other_kp = KeyPair::generate();
        signed_tx.signer = other_kp.public_key();

        assert!(signed_tx.verify_signature().is_err());
    }

    #[test]
    fn test_signed_transaction_verify_pow() {
        let signed_tx = create_signed_tx(8);
        assert!(signed_tx.verify_pow(8));
        assert!(signed_tx.verify_pow(1)); // Lower difficulty also passes
        assert!(!signed_tx.verify_pow(16)); // Higher difficulty fails
    }

    #[test]
    fn test_signed_transaction_verify_pow_wrong_hash() {
        let mut signed_tx = create_signed_tx(4);
        signed_tx.pow_hash = [0xFF; 32]; // Invalid hash

        assert!(!signed_tx.verify_pow(4));
    }

    #[test]
    fn test_signed_transaction_full_verify() {
        let signed_tx = create_signed_tx(4);
        assert!(signed_tx.verify(4).is_ok());
    }

    #[test]
    fn test_signed_transaction_verify_fails_low_difficulty() {
        // Mine with difficulty 4 (4 leading zero bits required)
        let signed_tx = create_signed_tx(4);

        // Verify with difficulty 32 should fail.
        // The probability of a hash mined for difficulty 4 accidentally
        // meeting difficulty 32 is 1/2^28 ≈ 0.00000004%, making this
        // test deterministically reliable.
        assert!(
            signed_tx.verify(32).is_err(),
            "Transaction mined with difficulty 4 should not pass verification at difficulty 32"
        );

        // Also verify the transaction passes at its mined difficulty
        assert!(
            signed_tx.verify(4).is_ok(),
            "Transaction should pass verification at its mined difficulty"
        );
    }

    #[test]
    fn test_serialization() {
        let signed_tx = create_signed_tx(1);

        let bytes = serialize(&signed_tx).unwrap();
        let recovered: SignedTransaction = crate::serialization::deserialize(&bytes).unwrap();

        assert_eq!(signed_tx, recovered);
        assert_eq!(signed_tx.id(), recovered.id());
    }

    #[test]
    fn test_acceptance_criterion_3_tx_id() {
        // Acceptance criterion: Transaction ID is SHA-256 of serialized SignedTransaction
        let signed_tx = create_signed_tx(1);

        // Get the ID
        let id = signed_tx.id();

        // Manually compute
        let bytes = serialize(&signed_tx).unwrap();
        let manual_id = sha256(&bytes);

        assert_eq!(id, manual_id);
    }
}
