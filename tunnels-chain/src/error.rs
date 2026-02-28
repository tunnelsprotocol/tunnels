//! Chain error types.

use std::fmt;

use tunnels_state::StateError;

/// Result type for chain operations.
pub type ChainResult<T> = Result<T, ChainError>;

/// Errors that can occur during chain operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainError {
    // Block validation errors (rules 1-11)
    /// Block version does not match expected protocol version.
    InvalidVersion { expected: u32, actual: u32 },

    /// Block height does not match expected height.
    InvalidHeight { expected: u64, actual: u64 },

    /// Previous block hash does not match.
    InvalidPrevHash { expected: [u8; 32], actual: [u8; 32] },

    /// Block timestamp is earlier than median of last 11 blocks.
    TimestampTooEarly { block_timestamp: u64, median_past: u64 },

    /// Block timestamp is more than 2 hours in the future.
    TimestampTooFuture { block_timestamp: u64, max_allowed: u64 },

    /// Block hash does not meet difficulty target.
    InsufficientPow { required: u64, actual_leading_zeros: u64 },

    /// Block difficulty does not match expected difficulty for this height.
    InvalidDifficulty { expected: u64, actual: u64 },

    /// Transaction root does not match computed Merkle root.
    InvalidTxRoot { expected: [u8; 32], actual: [u8; 32] },

    /// A transaction in the block is invalid.
    InvalidTransaction { index: usize, error: String },

    /// State root does not match computed state root after applying transactions.
    InvalidStateRoot { expected: [u8; 32], actual: [u8; 32] },

    /// Block exceeds maximum allowed size.
    BlockTooLarge { size: usize, max: usize },

    // Chain errors
    /// The previous block referenced by this block is not known.
    UnknownPrevBlock { hash: [u8; 32] },

    /// Block with this hash already exists in the chain.
    BlockAlreadyExists { hash: [u8; 32] },

    /// Attempted to add a second genesis block.
    GenesisAlreadyExists,

    /// Block is not the genesis but references the zero hash.
    OrphanBlock { hash: [u8; 32] },

    /// State for ancestor block is not cached (reorg too deep).
    /// Phase 4 persistent storage enables replay from any block.
    AncestorStateNotCached { hash: [u8; 32] },

    // Mempool errors
    /// Transaction is already in the mempool.
    TransactionAlreadyInPool { tx_id: [u8; 32] },

    /// Transaction conflicts with another transaction in the pool.
    TransactionConflictsWithPool {
        tx_id: [u8; 32],
        conflicting_tx: [u8; 32],
    },

    /// Mempool is full and transaction priority is too low.
    MempoolFull { capacity: usize },

    /// Transaction failed validation against current state.
    TransactionValidationFailed { tx_id: [u8; 32], error: String },

    /// Transaction proof-of-work is insufficient.
    InsufficientTxPow { required: u64, actual: u64 },

    // State errors
    /// Error from the state machine.
    StateError(StateError),

    // Production errors
    /// No transactions available for block production.
    NoTransactionsAvailable,

    /// Failed to mine block within iteration limit.
    MiningFailed { iterations: u64 },
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainError::InvalidVersion { expected, actual } => {
                write!(f, "invalid block version: expected {expected}, got {actual}")
            }
            ChainError::InvalidHeight { expected, actual } => {
                write!(f, "invalid block height: expected {expected}, got {actual}")
            }
            ChainError::InvalidPrevHash { expected, actual } => {
                write!(
                    f,
                    "invalid prev_block_hash: expected {}, got {}",
                    hex(&expected[..4]),
                    hex(&actual[..4])
                )
            }
            ChainError::TimestampTooEarly {
                block_timestamp,
                median_past,
            } => {
                write!(
                    f,
                    "block timestamp {block_timestamp} is before median-time-past {median_past}"
                )
            }
            ChainError::TimestampTooFuture {
                block_timestamp,
                max_allowed,
            } => {
                write!(
                    f,
                    "block timestamp {block_timestamp} is too far in future (max {max_allowed})"
                )
            }
            ChainError::InsufficientPow {
                required,
                actual_leading_zeros,
            } => {
                write!(
                    f,
                    "insufficient proof-of-work: required {required} leading zero bits, got {actual_leading_zeros}"
                )
            }
            ChainError::InvalidDifficulty { expected, actual } => {
                write!(
                    f,
                    "invalid difficulty: expected {expected}, got {actual}"
                )
            }
            ChainError::InvalidTxRoot { expected, actual } => {
                write!(
                    f,
                    "invalid tx_root: expected {}, got {}",
                    hex(&expected[..4]),
                    hex(&actual[..4])
                )
            }
            ChainError::InvalidTransaction { index, error } => {
                write!(f, "invalid transaction at index {index}: {error}")
            }
            ChainError::InvalidStateRoot { expected, actual } => {
                write!(
                    f,
                    "invalid state_root: expected {}, got {}",
                    hex(&expected[..4]),
                    hex(&actual[..4])
                )
            }
            ChainError::BlockTooLarge { size, max } => {
                write!(f, "block too large: {size} bytes (max {max})")
            }
            ChainError::UnknownPrevBlock { hash } => {
                write!(f, "unknown previous block: {}", hex(&hash[..4]))
            }
            ChainError::BlockAlreadyExists { hash } => {
                write!(f, "block already exists: {}", hex(&hash[..4]))
            }
            ChainError::GenesisAlreadyExists => {
                write!(f, "genesis block already exists")
            }
            ChainError::OrphanBlock { hash } => {
                write!(f, "orphan block (no parent): {}", hex(&hash[..4]))
            }
            ChainError::AncestorStateNotCached { hash } => {
                write!(
                    f,
                    "ancestor state not cached (reorg too deep): {}",
                    hex(&hash[..4])
                )
            }
            ChainError::TransactionAlreadyInPool { tx_id } => {
                write!(f, "transaction already in mempool: {}", hex(&tx_id[..4]))
            }
            ChainError::TransactionConflictsWithPool {
                tx_id,
                conflicting_tx,
            } => {
                write!(
                    f,
                    "transaction {} conflicts with {} in mempool",
                    hex(&tx_id[..4]),
                    hex(&conflicting_tx[..4])
                )
            }
            ChainError::MempoolFull { capacity } => {
                write!(f, "mempool full (capacity {capacity})")
            }
            ChainError::TransactionValidationFailed { tx_id, error } => {
                write!(
                    f,
                    "transaction {} validation failed: {error}",
                    hex(&tx_id[..4])
                )
            }
            ChainError::InsufficientTxPow { required, actual } => {
                write!(
                    f,
                    "insufficient transaction PoW: required {required} bits, got {actual}"
                )
            }
            ChainError::StateError(e) => {
                write!(f, "state error: {e}")
            }
            ChainError::NoTransactionsAvailable => {
                write!(f, "no transactions available for block production")
            }
            ChainError::MiningFailed { iterations } => {
                write!(f, "mining failed after {iterations} iterations")
            }
        }
    }
}

impl std::error::Error for ChainError {}

impl From<StateError> for ChainError {
    fn from(err: StateError) -> Self {
        ChainError::StateError(err)
    }
}

/// Helper to format bytes as hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ChainError::InvalidVersion {
            expected: 1,
            actual: 2,
        };
        assert!(err.to_string().contains("version"));
        assert!(err.to_string().contains("1"));
        assert!(err.to_string().contains("2"));
    }

    #[test]
    fn test_error_from_state_error() {
        let state_err = StateError::ZeroDeposit;
        let chain_err: ChainError = state_err.into();
        assert!(matches!(chain_err, ChainError::StateError(_)));
    }
}
