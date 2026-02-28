//! Execution context for transaction processing.

/// Default minimum verification window (24 hours in seconds).
pub const DEFAULT_MIN_VERIFICATION_WINDOW: u64 = 86_400;

/// Devnet minimum verification window (10 seconds).
pub const DEVNET_MIN_VERIFICATION_WINDOW: u64 = 10;

/// Execution context carrying block-level information.
///
/// This context is passed to all transaction handlers and provides
/// the block timestamp and other block-level metadata needed for
/// validation.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    /// Current block timestamp (unix seconds).
    pub timestamp: u64,

    /// Current block height.
    pub block_height: u64,

    /// Transaction PoW difficulty for this block.
    pub tx_difficulty: u64,

    /// Minimum verification window for projects (seconds).
    /// In devnet mode, this can be as low as 10 seconds.
    pub min_verification_window: u64,
}

impl ExecutionContext {
    /// Create a new execution context.
    pub fn new(timestamp: u64, block_height: u64, tx_difficulty: u64) -> Self {
        Self {
            timestamp,
            block_height,
            tx_difficulty,
            min_verification_window: DEFAULT_MIN_VERIFICATION_WINDOW,
        }
    }

    /// Create an execution context with a specific timestamp.
    pub fn with_timestamp(timestamp: u64) -> Self {
        Self {
            timestamp,
            block_height: 0,
            tx_difficulty: 1,
            min_verification_window: DEFAULT_MIN_VERIFICATION_WINDOW,
        }
    }

    /// Create a devnet execution context with relaxed constraints.
    pub fn devnet(timestamp: u64, block_height: u64, tx_difficulty: u64) -> Self {
        Self {
            timestamp,
            block_height,
            tx_difficulty,
            min_verification_window: DEVNET_MIN_VERIFICATION_WINDOW,
        }
    }

    /// Create an execution context for testing with minimal values.
    #[cfg(test)]
    pub fn test_context() -> Self {
        Self {
            timestamp: 1700000000,
            block_height: 1,
            tx_difficulty: 1,
            min_verification_window: DEFAULT_MIN_VERIFICATION_WINDOW,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context_new() {
        let ctx = ExecutionContext::new(1700000000, 100, 16);
        assert_eq!(ctx.timestamp, 1700000000);
        assert_eq!(ctx.block_height, 100);
        assert_eq!(ctx.tx_difficulty, 16);
    }

    #[test]
    fn test_execution_context_clone() {
        let ctx = ExecutionContext::new(1700000000, 100, 16);
        let cloned = ctx.clone();
        assert_eq!(ctx.timestamp, cloned.timestamp);
        assert_eq!(ctx.block_height, cloned.block_height);
        assert_eq!(ctx.tx_difficulty, cloned.tx_difficulty);
    }
}
