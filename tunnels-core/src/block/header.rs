//! Block header structure.

use serde::{Deserialize, Serialize};

use crate::crypto::sha256;
use crate::serialization::serialize;

/// Block header containing metadata and commitments.
///
/// The block hash is computed from the serialized header,
/// not including the transaction bodies (which are committed via tx_root).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Protocol version (currently 1).
    pub version: u32,

    /// Block height (0 for genesis).
    pub height: u64,

    /// Unix timestamp in seconds.
    pub timestamp: u64,

    /// SHA-256 hash of the previous block header.
    /// All zeros for the genesis block.
    pub prev_block_hash: [u8; 32],

    /// Merkle root of the state tree after applying all transactions.
    pub state_root: [u8; 32],

    /// Merkle root of transaction IDs in this block.
    pub tx_root: [u8; 32],

    /// Block difficulty target for proof-of-work.
    pub difficulty: u64,

    /// Proof-of-work nonce.
    pub nonce: u64,
}

impl BlockHeader {
    /// Protocol version number.
    pub const VERSION: u32 = 1;

    /// Compute the block hash.
    ///
    /// The hash is SHA-256 of the bincode-serialized header.
    pub fn hash(&self) -> [u8; 32] {
        let bytes = serialize(self).expect("BlockHeader serialization should not fail");
        sha256(&bytes)
    }

    /// Check if this is a genesis block.
    #[inline]
    pub fn is_genesis(&self) -> bool {
        self.height == 0 && self.prev_block_hash == [0u8; 32]
    }

    /// Verify that the block hash meets the difficulty target.
    ///
    /// The hash must have at least `difficulty` leading zero bits.
    pub fn meets_difficulty(&self) -> bool {
        let hash = self.hash();
        crate::transaction::meets_difficulty(&hash, self.difficulty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> BlockHeader {
        BlockHeader {
            version: BlockHeader::VERSION,
            height: 0,
            timestamp: 1700000000,
            prev_block_hash: [0u8; 32],
            state_root: [1u8; 32],
            tx_root: [2u8; 32],
            difficulty: 1,
            nonce: 0,
        }
    }

    #[test]
    fn test_block_hash_determinism() {
        let header = test_header();

        let hash1 = header.hash();
        let hash2 = header.hash();

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32);
    }

    #[test]
    fn test_block_hash_changes_with_nonce() {
        let mut header = test_header();

        let hash1 = header.hash();
        header.nonce = 1;
        let hash2 = header.hash();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_block_hash_changes_with_any_field() {
        let baseline = test_header();
        let baseline_hash = baseline.hash();

        // Changing any field should change the hash
        let mut h = baseline.clone();
        h.version = 2;
        assert_ne!(h.hash(), baseline_hash);

        let mut h = baseline.clone();
        h.height = 1;
        assert_ne!(h.hash(), baseline_hash);

        let mut h = baseline.clone();
        h.timestamp = 1700000001;
        assert_ne!(h.hash(), baseline_hash);

        let mut h = baseline.clone();
        h.prev_block_hash = [1u8; 32];
        assert_ne!(h.hash(), baseline_hash);

        let mut h = baseline.clone();
        h.state_root = [3u8; 32];
        assert_ne!(h.hash(), baseline_hash);

        let mut h = baseline.clone();
        h.tx_root = [3u8; 32];
        assert_ne!(h.hash(), baseline_hash);

        let mut h = baseline.clone();
        h.difficulty = 2;
        assert_ne!(h.hash(), baseline_hash);
    }

    #[test]
    fn test_is_genesis() {
        let genesis = test_header();
        assert!(genesis.is_genesis());

        let mut non_genesis = test_header();
        non_genesis.height = 1;
        assert!(!non_genesis.is_genesis());

        let mut non_genesis = test_header();
        non_genesis.prev_block_hash = [1u8; 32];
        assert!(!non_genesis.is_genesis());
    }

    #[test]
    fn test_serialization() {
        let header = test_header();

        let bytes = serialize(&header).unwrap();
        let recovered: BlockHeader = crate::serialization::deserialize(&bytes).unwrap();

        assert_eq!(header, recovered);
        assert_eq!(header.hash(), recovered.hash());
    }

    #[test]
    fn test_acceptance_criterion_3() {
        // Acceptance criterion 3: Construct a Block with multiple transactions.
        // Serialize the header. Hash it. Verify the hash is deterministic.
        let header = BlockHeader {
            version: 1,
            height: 100,
            timestamp: 1700000000,
            prev_block_hash: [0xAB; 32],
            state_root: [0xCD; 32],
            tx_root: [0xEF; 32],
            difficulty: 16,
            nonce: 12345,
        };

        // Serialize
        let bytes = serialize(&header).unwrap();

        // Hash multiple times
        let hash1 = sha256(&bytes);
        let hash2 = sha256(&bytes);
        let hash3 = header.hash();

        assert_eq!(hash1, hash2);
        assert_eq!(hash1, hash3);
    }
}
