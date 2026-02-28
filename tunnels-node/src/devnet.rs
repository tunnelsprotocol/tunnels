//! Devnet mode configuration and helpers.
//!
//! Devnet mode enables rapid development and testing by:
//! - Using trivial PoW (difficulty = 1)
//! - Auto-mining blocks when transactions enter the mempool
//! - Shortened timing windows

/// Devnet configuration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DevnetConfig {
    /// Use trivial proof-of-work (difficulty = 1).
    pub trivial_pow: bool,

    /// Automatically mine a block when transactions are pending.
    pub auto_mine: bool,

    /// Use faster difficulty adjustments.
    pub fast_difficulty: bool,

    /// Block difficulty override (if trivial_pow is true).
    pub block_difficulty: u64,

    /// Transaction difficulty override (if trivial_pow is true).
    pub tx_difficulty: u64,
}

impl DevnetConfig {
    /// Devnet block difficulty (1 leading zero bit).
    pub const DEVNET_BLOCK_DIFFICULTY: u64 = 1;

    /// Devnet transaction difficulty (1 leading zero bit).
    pub const DEVNET_TX_DIFFICULTY: u64 = 1;

    /// Create a new devnet configuration with all features enabled.
    pub fn new() -> Self {
        Self {
            trivial_pow: true,
            auto_mine: true,
            fast_difficulty: true,
            block_difficulty: Self::DEVNET_BLOCK_DIFFICULTY,
            tx_difficulty: Self::DEVNET_TX_DIFFICULTY,
        }
    }

    /// Get the effective block difficulty.
    #[allow(dead_code)]
    pub fn effective_block_difficulty(&self) -> Option<u64> {
        if self.trivial_pow {
            Some(self.block_difficulty)
        } else {
            None
        }
    }

    /// Get the effective transaction difficulty.
    #[allow(dead_code)]
    pub fn effective_tx_difficulty(&self) -> Option<u64> {
        if self.trivial_pow {
            Some(self.tx_difficulty)
        } else {
            None
        }
    }
}

impl Default for DevnetConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_devnet_config() {
        let config = DevnetConfig::default();
        assert!(config.trivial_pow);
        assert!(config.auto_mine);
        assert!(config.fast_difficulty);
        assert_eq!(config.block_difficulty, 1);
        assert_eq!(config.tx_difficulty, 1);
    }

    #[test]
    fn test_effective_difficulty() {
        let config = DevnetConfig::default();
        assert_eq!(config.effective_block_difficulty(), Some(1));
        assert_eq!(config.effective_tx_difficulty(), Some(1));

        let mut config = DevnetConfig::default();
        config.trivial_pow = false;
        assert_eq!(config.effective_block_difficulty(), None);
        assert_eq!(config.effective_tx_difficulty(), None);
    }
}
