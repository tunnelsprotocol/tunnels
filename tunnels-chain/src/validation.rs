//! Block validation.
//!
//! Implements all 11 block validation rules:
//! 1. version == PROTOCOL_VERSION (1)
//! 2. height == prev_block.height + 1
//! 3. prev_block_hash == SHA-256(prev_block.header)
//! 4. timestamp > median of last 11 timestamps (median-time-past)
//! 5. timestamp < current_time + 7200 (no more than 2 hours in future)
//! 6. Block hash meets difficulty target
//! 7. difficulty matches expected for this height
//! 8. tx_root == Merkle root of transaction hashes
//! 9. Every transaction valid (signature, per-tx PoW, state validation)
//! 10. state_root == hash of state after applying all transactions
//! 11. Block size <= 1 MB

use tunnels_core::block::{Block, BlockHeader};
use tunnels_core::crypto::sha256;
use tunnels_core::serialization::serialized_size;
use tunnels_core::transaction::SignedTransaction;
use tunnels_state::{apply_transaction_with_context, ExecutionContext, ProtocolState, StateReader, StateWriter};

use crate::chain::ChainState;
use crate::difficulty::calculate_next_difficulty;
use crate::error::{ChainError, ChainResult};

/// Protocol version number.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum block size in bytes (1 MB).
pub const MAX_BLOCK_SIZE: usize = 1_000_000;

/// Maximum time in the future for a block timestamp (2 hours in seconds).
pub const MAX_FUTURE_BLOCK_TIME: u64 = 7200;

/// Number of blocks used for median-time-past calculation.
pub const MEDIAN_TIME_PAST_BLOCKS: usize = 11;

/// Validate a complete block against the chain.
///
/// This performs all 11 validation checks. The block must:
/// 1. Have correct version
/// 2. Have correct height (prev + 1)
/// 3. Reference a known previous block
/// 4. Have timestamp after median-time-past
/// 5. Have timestamp not too far in the future
/// 6. Meet the difficulty target (PoW)
/// 7. Have correct difficulty for its height
/// 8. Have correct tx_root
/// 9. Contain only valid transactions
/// 10. Have correct state_root
/// 11. Not exceed maximum size
///
/// This function requires the parent state to be cached. For blocks whose
/// parent state isn't cached (e.g., fork blocks), use `validate_block_with_state`.
pub fn validate_block(
    block: &Block,
    chain: &ChainState,
    current_time: u64,
) -> ChainResult<ProtocolState> {
    validate_block_devnet(block, chain, current_time, false)
}

/// Validate a complete block with optional devnet mode.
pub fn validate_block_devnet(
    block: &Block,
    chain: &ChainState,
    current_time: u64,
    devnet: bool,
) -> ChainResult<ProtocolState> {
    // Get parent state from cache
    let parent_hash = block.header.prev_block_hash;
    let parent_state = chain
        .state_at(&parent_hash)
        .ok_or(ChainError::AncestorStateNotCached { hash: parent_hash })?
        .clone();

    validate_block_with_state_devnet(block, chain, current_time, parent_state, devnet)
}

/// Validate a complete block with a pre-computed parent state.
///
/// This is used when validating blocks whose parent state isn't cached
/// (e.g., during reorg or when extending a fork). The parent state is
/// computed via replay before calling this function.
///
/// Performs all 11 validation checks.
pub fn validate_block_with_state(
    block: &Block,
    chain: &ChainState,
    current_time: u64,
    parent_state: ProtocolState,
) -> ChainResult<ProtocolState> {
    validate_block_with_state_devnet(block, chain, current_time, parent_state, false)
}

/// Validate a complete block with a pre-computed parent state and devnet mode.
pub fn validate_block_with_state_devnet(
    block: &Block,
    chain: &ChainState,
    current_time: u64,
    parent_state: ProtocolState,
    devnet: bool,
) -> ChainResult<ProtocolState> {
    // Rule 11: Block size
    let block_size = serialized_size(block).map_err(|_| ChainError::BlockTooLarge {
        size: usize::MAX,
        max: MAX_BLOCK_SIZE,
    })? as usize;

    if block_size > MAX_BLOCK_SIZE {
        return Err(ChainError::BlockTooLarge {
            size: block_size,
            max: MAX_BLOCK_SIZE,
        });
    }

    // Get previous block (or handle genesis)
    let (prev_block, expected_height) = if block.header.is_genesis() {
        return Err(ChainError::GenesisAlreadyExists);
    } else {
        let prev = chain.get_block(&block.header.prev_block_hash).ok_or(
            ChainError::UnknownPrevBlock {
                hash: block.header.prev_block_hash,
            },
        )?;
        (prev, prev.block.height() + 1)
    };

    // Validate header
    validate_block_header(
        &block.header,
        &prev_block.block.header,
        expected_height,
        chain,
        current_time,
    )?;

    // Rule 8: tx_root
    let computed_tx_root = block.compute_tx_root();
    if block.header.tx_root != computed_tx_root {
        return Err(ChainError::InvalidTxRoot {
            expected: block.header.tx_root,
            actual: computed_tx_root,
        });
    }

    // Clone the parent state to apply transactions
    let mut state = parent_state;

    // Rule 9: Validate and apply all transactions
    validate_transactions_with_devnet(block, &mut state, chain, devnet)?;

    // Rule 10: state_root
    let computed_state_root = compute_state_root(&mut state);
    if block.header.state_root != computed_state_root {
        return Err(ChainError::InvalidStateRoot {
            expected: block.header.state_root,
            actual: computed_state_root,
        });
    }

    Ok(state)
}

/// Validate a block header against expected values.
///
/// Checks rules 1-7 (version, height, prev_hash, timestamps, PoW, difficulty).
pub fn validate_block_header(
    header: &BlockHeader,
    prev_header: &BlockHeader,
    expected_height: u64,
    chain: &ChainState,
    current_time: u64,
) -> ChainResult<()> {
    // Rule 1: version
    if header.version != PROTOCOL_VERSION {
        return Err(ChainError::InvalidVersion {
            expected: PROTOCOL_VERSION,
            actual: header.version,
        });
    }

    // Rule 2: height
    if header.height != expected_height {
        return Err(ChainError::InvalidHeight {
            expected: expected_height,
            actual: header.height,
        });
    }

    // Rule 3: prev_block_hash
    let expected_prev_hash = prev_header.hash();
    if header.prev_block_hash != expected_prev_hash {
        return Err(ChainError::InvalidPrevHash {
            expected: expected_prev_hash,
            actual: header.prev_block_hash,
        });
    }

    // Rule 4: timestamp > median-time-past
    let median_past = chain.compute_median_time_past(&header.prev_block_hash);
    if header.timestamp <= median_past {
        return Err(ChainError::TimestampTooEarly {
            block_timestamp: header.timestamp,
            median_past,
        });
    }

    // Rule 5: timestamp < current_time + 2 hours
    let max_timestamp = current_time.saturating_add(MAX_FUTURE_BLOCK_TIME);
    if header.timestamp > max_timestamp {
        return Err(ChainError::TimestampTooFuture {
            block_timestamp: header.timestamp,
            max_allowed: max_timestamp,
        });
    }

    // Rule 6: PoW
    if !header.meets_difficulty() {
        let hash = header.hash();
        let actual_zeros = count_leading_zero_bits(&hash);
        return Err(ChainError::InsufficientPow {
            required: header.difficulty,
            actual_leading_zeros: actual_zeros,
        });
    }

    // Rule 7: difficulty matches expected
    let expected_difficulty = calculate_next_difficulty(chain, header.height);
    if header.difficulty != expected_difficulty {
        return Err(ChainError::InvalidDifficulty {
            expected: expected_difficulty,
            actual: header.difficulty,
        });
    }

    Ok(())
}

/// Validate all transactions in a block and apply them to state.
///
/// Each transaction must:
/// - Have a valid signature
/// - Meet per-transaction PoW difficulty
/// - Pass state validation
pub fn validate_transactions(
    block: &Block,
    state: &mut ProtocolState,
    _chain: &ChainState,
) -> ChainResult<()> {
    validate_transactions_with_devnet(block, state, _chain, false)
}

/// Validate all transactions in a block with optional devnet mode.
///
/// In devnet mode, shorter verification windows are allowed for projects.
pub fn validate_transactions_with_devnet(
    block: &Block,
    state: &mut ProtocolState,
    _chain: &ChainState,
    devnet: bool,
) -> ChainResult<()> {
    let tx_difficulty = state.get_tx_difficulty_state().current_tx_difficulty;

    // Create execution context - use devnet variant if enabled
    let ctx = if devnet {
        ExecutionContext::devnet(
            block.header.timestamp,
            block.header.height,
            tx_difficulty,
        )
    } else {
        ExecutionContext::new(
            block.header.timestamp,
            block.header.height,
            tx_difficulty,
        )
    };

    for (index, tx) in block.transactions.iter().enumerate() {
        // Verify signature
        if tx.verify_signature().is_err() {
            return Err(ChainError::InvalidTransaction {
                index,
                error: "invalid signature".to_string(),
            });
        }

        // Verify per-transaction PoW
        if !tx.verify_pow(tx_difficulty) {
            let actual = count_tx_leading_zeros(tx);
            return Err(ChainError::InvalidTransaction {
                index,
                error: format!(
                    "insufficient proof-of-work: required {} bits, got {}",
                    tx_difficulty, actual
                ),
            });
        }

        // Apply transaction to state
        apply_transaction_with_context(state, tx, &ctx).map_err(|e| {
            ChainError::InvalidTransaction {
                index,
                error: e.to_string(),
            }
        })?;
    }

    // Update transaction difficulty state based on this block's tx count.
    // Skip in devnet mode to avoid difficulty ramp-up during testing.
    if !devnet {
        let tx_count = block.transactions.len() as u64;
        state.update_tx_difficulty_state(|diff_state| {
            diff_state.record_block(tx_count, tunnels_core::TransactionDifficultyState::DEFAULT_WINDOW_SIZE);
        });
    }

    Ok(())
}

/// Compute the state root for a given protocol state.
///
/// For Phase 3, this is a simple hash based on state metrics.
/// Phase 4 replaces this with a proper Merkle Patricia Trie.
///
/// The state root commits to:
/// - Identity count
/// - Project count
/// - Attestation count
/// - Current tx difficulty
pub fn compute_state_root(state: &mut ProtocolState) -> [u8; 32] {
    // Create a deterministic commitment to state
    // This is a placeholder - Phase 4 implements proper Merkle trie
    let mut data = Vec::with_capacity(32);

    // Include counts
    data.extend_from_slice(&(state.identity_count() as u64).to_le_bytes());
    data.extend_from_slice(&(state.project_count() as u64).to_le_bytes());
    data.extend_from_slice(&(state.attestation_count() as u64).to_le_bytes());

    // Include tx difficulty
    data.extend_from_slice(&state.get_tx_difficulty_state().current_tx_difficulty.to_le_bytes());

    sha256(&data)
}

/// Count leading zero bits in a hash.
fn count_leading_zero_bits(hash: &[u8; 32]) -> u64 {
    let mut count = 0u64;
    for byte in hash.iter() {
        if *byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros() as u64;
            break;
        }
    }
    count
}

/// Count leading zero bits in a transaction's PoW hash.
fn count_tx_leading_zeros(tx: &SignedTransaction) -> u64 {
    count_leading_zero_bits(&tx.pow_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_leading_zero_bits() {
        assert_eq!(count_leading_zero_bits(&[0u8; 32]), 256);
        assert_eq!(count_leading_zero_bits(&[0xFF; 32]), 0);

        let mut hash = [0u8; 32];
        hash[0] = 0x01; // 7 leading zeros in first byte
        assert_eq!(count_leading_zero_bits(&hash), 7);

        hash[0] = 0x00;
        hash[1] = 0x01; // 8 zeros in first byte + 7 in second
        assert_eq!(count_leading_zero_bits(&hash), 15);

        hash[0] = 0x00;
        hash[1] = 0x80; // 8 zeros in first byte + 0 in second (0x80 = 0b10000000)
        assert_eq!(count_leading_zero_bits(&hash), 8);
    }

    #[test]
    fn test_compute_state_root_determinism() {
        let mut state = ProtocolState::new();
        let root1 = compute_state_root(&mut state);
        let root2 = compute_state_root(&mut state);
        assert_eq!(root1, root2);
    }
}
