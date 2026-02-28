//! Block structure containing header and transactions.

use serde::{Deserialize, Serialize};

use crate::block::BlockHeader;
use crate::crypto::merkle_root;
use crate::transaction::SignedTransaction;

/// A block containing a header and ordered transactions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// Block header with metadata and commitments.
    pub header: BlockHeader,

    /// Ordered list of transactions in this block.
    pub transactions: Vec<SignedTransaction>,
}

impl Block {
    /// Compute the Merkle root of transaction IDs.
    ///
    /// Transaction IDs are SHA-256 hashes of serialized SignedTransactions.
    /// The Merkle root is computed from these IDs.
    pub fn compute_tx_root(&self) -> [u8; 32] {
        let tx_ids: Vec<[u8; 32]> = self.transactions.iter().map(|tx| tx.id()).collect();
        merkle_root(&tx_ids)
    }

    /// Verify that the header's tx_root matches the transactions.
    pub fn verify_tx_root(&self) -> bool {
        self.header.tx_root == self.compute_tx_root()
    }

    /// Get the block hash (delegates to header).
    #[inline]
    pub fn hash(&self) -> [u8; 32] {
        self.header.hash()
    }

    /// Get the block height.
    #[inline]
    pub fn height(&self) -> u64 {
        self.header.height
    }

    /// Check if this is a genesis block.
    #[inline]
    pub fn is_genesis(&self) -> bool {
        self.header.is_genesis()
    }

    /// Get the number of transactions.
    #[inline]
    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{sign, KeyPair};
    use crate::serialization::serialize;
    use crate::transaction::{mine_pow, Transaction};

    fn create_test_signed_tx() -> SignedTransaction {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &kp.public_key(), &signature, 1);

        SignedTransaction {
            tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        }
    }

    fn test_block(transactions: Vec<SignedTransaction>) -> Block {
        let tx_root = if transactions.is_empty() {
            [0u8; 32]
        } else {
            let tx_ids: Vec<[u8; 32]> = transactions.iter().map(|tx| tx.id()).collect();
            merkle_root(&tx_ids)
        };

        Block {
            header: BlockHeader {
                version: BlockHeader::VERSION,
                height: 1,
                timestamp: 1700000000,
                prev_block_hash: [0xAB; 32],
                state_root: [0xCD; 32],
                tx_root,
                difficulty: 1,
                nonce: 0,
            },
            transactions,
        }
    }

    #[test]
    fn test_empty_block() {
        let block = test_block(vec![]);
        assert_eq!(block.tx_count(), 0);
        assert_eq!(block.compute_tx_root(), [0u8; 32]);
        assert!(block.verify_tx_root());
    }

    #[test]
    fn test_single_transaction() {
        let tx = create_test_signed_tx();
        let block = test_block(vec![tx.clone()]);

        assert_eq!(block.tx_count(), 1);
        assert!(block.verify_tx_root());

        // Merkle root of single tx is just the tx ID
        assert_eq!(block.compute_tx_root(), tx.id());
    }

    #[test]
    fn test_multiple_transactions() {
        let transactions: Vec<SignedTransaction> =
            (0..5).map(|_| create_test_signed_tx()).collect();

        let block = test_block(transactions);

        assert_eq!(block.tx_count(), 5);
        assert!(block.verify_tx_root());
    }

    #[test]
    fn test_verify_tx_root_fails_with_wrong_root() {
        let tx = create_test_signed_tx();
        let mut block = test_block(vec![tx]);

        // Corrupt the tx_root
        block.header.tx_root = [0xFF; 32];

        assert!(!block.verify_tx_root());
    }

    #[test]
    fn test_block_hash() {
        let block = test_block(vec![]);

        // Block hash should equal header hash
        assert_eq!(block.hash(), block.header.hash());
    }

    #[test]
    fn test_tx_root_determinism() {
        let transactions: Vec<SignedTransaction> =
            (0..3).map(|_| create_test_signed_tx()).collect();

        let block1 = test_block(transactions.clone());
        let block2 = test_block(transactions);

        assert_eq!(block1.compute_tx_root(), block2.compute_tx_root());
    }

    #[test]
    fn test_serialization() {
        let transactions: Vec<SignedTransaction> =
            (0..3).map(|_| create_test_signed_tx()).collect();

        let block = test_block(transactions);

        let bytes = serialize(&block).unwrap();
        let recovered: Block = crate::serialization::deserialize(&bytes).unwrap();

        assert_eq!(block, recovered);
        assert_eq!(block.hash(), recovered.hash());
    }

    #[test]
    fn test_acceptance_criterion_3_full() {
        // Acceptance criterion 3: Construct a Block with multiple transactions.
        // Serialize the header. Hash it. Verify the hash is deterministic.

        // Create multiple transactions
        let transactions: Vec<SignedTransaction> =
            (0..10).map(|_| create_test_signed_tx()).collect();

        let block = test_block(transactions);

        // Serialize the header
        let header_bytes = serialize(&block.header).unwrap();

        // Hash multiple times
        let hash1 = crate::crypto::sha256(&header_bytes);
        let hash2 = crate::crypto::sha256(&header_bytes);
        let hash3 = block.header.hash();
        let hash4 = block.hash();

        // All should be identical
        assert_eq!(hash1, hash2);
        assert_eq!(hash1, hash3);
        assert_eq!(hash1, hash4);

        // Verify tx_root is correct
        assert!(block.verify_tx_root());
    }
}
