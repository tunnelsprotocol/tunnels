//! Block production.
//!
//! Provides utilities for creating new blocks from pending transactions.

use tunnels_core::block::{Block, BlockHeader};
use tunnels_core::crypto::merkle_root;
use tunnels_core::transaction::SignedTransaction;
use tunnels_state::{apply_transaction, apply_transaction_with_context, ExecutionContext, ProtocolState, StateWriter};

use crate::chain::ChainState;
use crate::difficulty::calculate_next_difficulty;
use crate::error::{ChainError, ChainResult};
use crate::mempool::Mempool;
use crate::validation::{compute_state_root, MAX_BLOCK_SIZE, PROTOCOL_VERSION};

/// Builder for constructing new blocks.
pub struct BlockBuilder {
    /// Transactions to include.
    transactions: Vec<SignedTransaction>,

    /// State after applying transactions.
    state: ProtocolState,

    /// Parent block hash.
    prev_block_hash: [u8; 32],

    /// Block height.
    height: u64,

    /// Block timestamp.
    timestamp: u64,

    /// Block difficulty.
    difficulty: u64,

    /// Total size of transactions.
    total_size: usize,

    /// Whether devnet mode is enabled.
    devnet: bool,
}

impl BlockBuilder {
    /// Create a new block builder.
    pub fn new(chain: &ChainState, timestamp: u64) -> Self {
        let tip = chain.tip_block();
        let height = tip.height() + 1;
        let difficulty = calculate_next_difficulty(chain, height);
        let state = chain.state().clone();
        let devnet = chain.is_devnet();

        Self {
            transactions: Vec::new(),
            state,
            prev_block_hash: tip.hash,
            height,
            timestamp,
            difficulty,
            total_size: 0,
            devnet,
        }
    }

    /// Add a transaction to the block.
    ///
    /// The transaction is validated before being added.
    pub fn add_transaction(&mut self, tx: SignedTransaction) -> ChainResult<()> {
        let tx_size = tunnels_core::serialization::serialized_size(&tx)
            .map_err(|_| ChainError::BlockTooLarge {
                size: usize::MAX,
                max: MAX_BLOCK_SIZE,
            })? as usize;

        // Check if adding this tx would exceed block size
        // Account for header overhead (~200 bytes)
        if self.total_size + tx_size + 200 > MAX_BLOCK_SIZE {
            return Err(ChainError::BlockTooLarge {
                size: self.total_size + tx_size,
                max: MAX_BLOCK_SIZE,
            });
        }

        // Validate and apply transaction
        if self.devnet {
            let ctx = ExecutionContext::devnet(self.timestamp, self.height, 1);
            apply_transaction_with_context(&mut self.state, &tx, &ctx)?;
        } else {
            apply_transaction(&mut self.state, &tx, self.timestamp)?;
        }

        self.transactions.push(tx);
        self.total_size += tx_size;

        Ok(())
    }

    /// Add multiple transactions, skipping invalid ones.
    ///
    /// Returns the number of transactions added.
    pub fn add_transactions(&mut self, txs: Vec<SignedTransaction>) -> usize {
        let mut added = 0;
        for tx in txs {
            if self.add_transaction(tx).is_ok() {
                added += 1;
            }
        }
        added
    }

    /// Get the number of transactions.
    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }

    /// Build the block header (without mining).
    fn build_header(&mut self) -> BlockHeader {
        let tx_ids: Vec<[u8; 32]> = self.transactions.iter().map(|tx| tx.id()).collect();
        let tx_root = merkle_root(&tx_ids);

        // Update tx difficulty state based on tx count before computing state root.
        // This mirrors the logic in validation.rs validate_transactions_with_devnet.
        // Skip in devnet mode to avoid difficulty ramp-up during testing.
        if !self.devnet {
            let tx_count = self.transactions.len() as u64;
            self.state.update_tx_difficulty_state(|diff_state| {
                diff_state.record_block(
                    tx_count,
                    tunnels_core::TransactionDifficultyState::DEFAULT_WINDOW_SIZE,
                );
            });
        }

        let state_root = compute_state_root(&mut self.state);

        BlockHeader {
            version: PROTOCOL_VERSION,
            height: self.height,
            timestamp: self.timestamp,
            prev_block_hash: self.prev_block_hash,
            state_root,
            tx_root,
            difficulty: self.difficulty,
            nonce: 0, // Will be set during mining
        }
    }

    /// Build and mine the block.
    ///
    /// This iterates through nonces until finding one that meets the difficulty.
    pub fn build(mut self) -> Block {
        let mut header = self.build_header();

        // Mine: find a nonce that meets difficulty
        loop {
            if header.meets_difficulty() {
                break;
            }
            header.nonce = header.nonce.wrapping_add(1);
        }

        Block {
            header,
            transactions: self.transactions,
        }
    }

    /// Build the block with a specific nonce (for testing).
    #[cfg(test)]
    pub fn build_with_nonce(mut self, nonce: u64) -> Block {
        let mut header = self.build_header();
        header.nonce = nonce;

        Block {
            header,
            transactions: self.transactions,
        }
    }
}

/// Produce a block from the mempool.
///
/// Selects transactions from the mempool, validates them,
/// and mines a new block.
pub fn produce_block(
    chain: &ChainState,
    mempool: &Mempool,
    timestamp: u64,
) -> ChainResult<Block> {
    let mut builder = BlockBuilder::new(chain, timestamp);

    // Get transactions from mempool
    let txs = mempool.get_transactions_for_block(MAX_BLOCK_SIZE);

    // Add transactions (invalid ones will be skipped)
    builder.add_transactions(txs);

    Ok(builder.build())
}

/// Mine a block with specific difficulty (for testing).
///
/// Iterates through nonces until the header hash meets the target.
#[allow(dead_code)]
pub fn mine_block_header(header: &mut BlockHeader, max_iterations: u64) -> ChainResult<()> {
    for _ in 0..max_iterations {
        if header.meets_difficulty() {
            return Ok(());
        }
        header.nonce = header.nonce.wrapping_add(1);
    }

    Err(ChainError::MiningFailed {
        iterations: max_iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::GENESIS_DIFFICULTY;
    use tunnels_core::crypto::{sign, KeyPair};
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::{mine_pow, Transaction};

    fn create_test_tx(kp: &KeyPair) -> SignedTransaction {
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &kp.public_key(), &signature, GENESIS_DIFFICULTY);

        SignedTransaction {
            tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        }
    }

    #[test]
    fn test_block_builder_empty() {
        let chain = ChainState::new();
        let builder = BlockBuilder::new(&chain, 1700000060);

        assert_eq!(builder.tx_count(), 0);
        assert_eq!(builder.height, 1);
    }

    #[test]
    fn test_block_builder_with_transaction() {
        let chain = ChainState::new();
        let mut builder = BlockBuilder::new(&chain, 1700000060);

        let kp = KeyPair::generate();
        let tx = create_test_tx(&kp);

        builder.add_transaction(tx).unwrap();
        assert_eq!(builder.tx_count(), 1);
    }

    #[test]
    fn test_block_builder_build() {
        let chain = ChainState::new();
        let builder = BlockBuilder::new(&chain, 1700000060);
        let block = builder.build();

        assert_eq!(block.height(), 1);
        assert_eq!(block.header.prev_block_hash, chain.genesis_hash());
        assert!(block.header.meets_difficulty());
    }

    #[test]
    fn test_produce_block_empty_mempool() {
        let chain = ChainState::new();
        let mempool = Mempool::with_defaults();

        let block = produce_block(&chain, &mempool, 1700000060).unwrap();

        assert_eq!(block.height(), 1);
        assert_eq!(block.tx_count(), 0);
        assert!(block.header.meets_difficulty());
    }

    #[test]
    fn test_block_verifies_tx_root() {
        let chain = ChainState::new();
        let mut builder = BlockBuilder::new(&chain, 1700000060);

        let kp = KeyPair::generate();
        let tx = create_test_tx(&kp);
        builder.add_transaction(tx).unwrap();

        let block = builder.build();
        assert!(block.verify_tx_root());
    }
}
