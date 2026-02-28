//! Genesis block creation.
//!
//! The genesis block is the first block in the chain. It has:
//! - Height 0
//! - prev_block_hash of all zeros
//! - Empty transaction list
//! - Empty state (state_root is hash of empty state)
//! - Fixed timestamp for determinism
//! - Initial difficulty

use tunnels_core::block::{Block, BlockHeader};
use tunnels_core::crypto::merkle_root;
use tunnels_state::ProtocolState;

use crate::validation::compute_state_root;

/// Fixed timestamp for the genesis block (2023-11-14 22:13:20 UTC).
pub const GENESIS_TIMESTAMP: u64 = 1700000000;

/// Initial block difficulty (8 leading zero bits).
pub const GENESIS_DIFFICULTY: u64 = 8;

/// Genesis block nonce (pre-computed to meet difficulty).
/// This is the nonce that makes the genesis block hash meet GENESIS_DIFFICULTY.
const GENESIS_NONCE: u64 = 256;

/// The canonical genesis block hash.
///
/// This is the SHA-256 hash of the genesis block header. All nodes must
/// agree on this hash to be on the same network.
///
/// Computed from: create_genesis_block().hash()
pub const GENESIS_BLOCK_HASH: [u8; 32] = [
    0x00, 0xe5, 0xa9, 0xcc, 0x69, 0xb6, 0xf0, 0x80,
    0x05, 0xe6, 0x3d, 0xb8, 0x8c, 0x8d, 0x21, 0xfc,
    0x21, 0x39, 0x6a, 0xbd, 0xa3, 0x5e, 0xec, 0x05,
    0xc3, 0xfb, 0x45, 0x64, 0xec, 0x59, 0xca, 0xfa,
];

/// Compute the state root for an empty protocol state.
///
/// Uses the same state root computation as block validation.
fn empty_state_root() -> [u8; 32] {
    compute_state_root(&mut ProtocolState::new())
}

/// Create the genesis block.
///
/// The genesis block is deterministic: calling this function always
/// produces the same block with the same hash.
///
/// # Panics
///
/// This function should never panic. It constructs a valid genesis block
/// from constants.
pub fn create_genesis_block() -> Block {
    // Empty transaction list
    let transactions = vec![];

    // Merkle root of empty tx list is all zeros
    let tx_root = merkle_root(&[]);

    // State root is hash of empty state
    let state_root = empty_state_root();

    let header = BlockHeader {
        version: BlockHeader::VERSION,
        height: 0,
        timestamp: GENESIS_TIMESTAMP,
        prev_block_hash: [0u8; 32],
        state_root,
        tx_root,
        difficulty: GENESIS_DIFFICULTY,
        nonce: GENESIS_NONCE,
    };

    Block {
        header,
        transactions,
    }
}

/// Mine the genesis block to find a valid nonce.
///
/// This is only needed once to find GENESIS_NONCE. After that,
/// create_genesis_block() uses the pre-computed nonce.
#[allow(dead_code)]
fn mine_genesis_nonce() -> u64 {
    let tx_root = merkle_root(&[]);
    let state_root = empty_state_root();

    let mut header = BlockHeader {
        version: BlockHeader::VERSION,
        height: 0,
        timestamp: GENESIS_TIMESTAMP,
        prev_block_hash: [0u8; 32],
        state_root,
        tx_root,
        difficulty: GENESIS_DIFFICULTY,
        nonce: 0,
    };

    // Mine until we find a valid nonce
    loop {
        if header.meets_difficulty() {
            return header.nonce;
        }
        header.nonce = header.nonce.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_block_determinism() {
        // Acceptance criterion 1: Genesis block hash is deterministic
        let genesis1 = create_genesis_block();
        let genesis2 = create_genesis_block();

        assert_eq!(genesis1.hash(), genesis2.hash());
        assert_eq!(genesis1.header, genesis2.header);
        assert_eq!(genesis1.transactions, genesis2.transactions);
    }

    #[test]
    fn test_genesis_block_structure() {
        let genesis = create_genesis_block();

        // Check basic structure
        assert_eq!(genesis.header.version, BlockHeader::VERSION);
        assert_eq!(genesis.header.height, 0);
        assert_eq!(genesis.header.timestamp, GENESIS_TIMESTAMP);
        assert_eq!(genesis.header.prev_block_hash, [0u8; 32]);
        assert_eq!(genesis.header.difficulty, GENESIS_DIFFICULTY);
        assert!(genesis.transactions.is_empty());

        // Check tx_root is merkle root of empty list
        assert_eq!(genesis.header.tx_root, merkle_root(&[]));

        // Check state_root matches empty state
        assert_eq!(genesis.header.state_root, empty_state_root());
    }

    #[test]
    fn test_genesis_is_genesis() {
        let genesis = create_genesis_block();
        assert!(genesis.is_genesis());
        assert!(genesis.header.is_genesis());
    }

    #[test]
    fn test_genesis_tx_root_valid() {
        let genesis = create_genesis_block();
        assert!(genesis.verify_tx_root());
    }

    #[test]
    fn test_empty_state_root_determinism() {
        let root1 = empty_state_root();
        let root2 = empty_state_root();
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_genesis_hash_reproducible() {
        // Create genesis multiple times and verify hash is always the same
        let hashes: Vec<[u8; 32]> = (0..10).map(|_| create_genesis_block().hash()).collect();

        let first = hashes[0];
        for hash in &hashes {
            assert_eq!(*hash, first);
        }
    }

    #[test]
    fn test_genesis_meets_difficulty_or_find_nonce() {
        let genesis = create_genesis_block();

        // If the current nonce doesn't meet difficulty, we need to update GENESIS_NONCE
        if !genesis.header.meets_difficulty() {
            let valid_nonce = mine_genesis_nonce();
            panic!(
                "Genesis block does not meet difficulty. Update GENESIS_NONCE to {}",
                valid_nonce
            );
        }

        assert!(genesis.header.meets_difficulty());
    }

    #[test]
    fn test_genesis_hash_matches_constant() {
        let genesis = create_genesis_block();
        let hash = genesis.hash();

        // If this fails, update GENESIS_BLOCK_HASH with the printed value
        if hash != GENESIS_BLOCK_HASH {
            panic!(
                "Genesis block hash mismatch!\n\
                 Expected: {:?}\n\
                 Actual:   {:?}\n\
                 Update GENESIS_BLOCK_HASH to:\n\
                 pub const GENESIS_BLOCK_HASH: [u8; 32] = {:?};",
                GENESIS_BLOCK_HASH, hash, hash
            );
        }

        assert_eq!(hash, GENESIS_BLOCK_HASH);
    }

    #[test]
    fn test_genesis_determinism_across_chain_states() {
        // Create two independent ChainState instances
        use crate::ChainState;

        let chain1 = ChainState::new();
        let chain2 = ChainState::new();

        // Get genesis block from each
        let genesis1 = chain1.get_block(&chain1.genesis_hash()).unwrap();
        let genesis2 = chain2.get_block(&chain2.genesis_hash()).unwrap();

        // Verify they are byte-identical
        assert_eq!(genesis1.block.header, genesis2.block.header);
        assert_eq!(genesis1.block.transactions, genesis2.block.transactions);
        assert_eq!(genesis1.block.hash(), genesis2.block.hash());

        // Verify the hash matches the constant
        assert_eq!(genesis1.block.hash(), GENESIS_BLOCK_HASH);
        assert_eq!(genesis2.block.hash(), GENESIS_BLOCK_HASH);

        // Also verify serialization is identical
        let bytes1 = tunnels_core::serialization::serialize(&genesis1.block).unwrap();
        let bytes2 = tunnels_core::serialization::serialize(&genesis2.block).unwrap();
        assert_eq!(bytes1, bytes2);
    }
}
