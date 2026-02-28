//! Transaction mempool.

use std::collections::HashMap;

use tunnels_core::transaction::SignedTransaction;
use tunnels_state::{apply_transaction, apply_transaction_with_context, ExecutionContext, ProtocolState, StateReader};

use crate::error::{ChainError, ChainResult};

use super::entry::MempoolEntry;

/// Default maximum mempool size (number of transactions).
pub const DEFAULT_MEMPOOL_SIZE: usize = 5000;

/// Default minimum transaction PoW difficulty.
pub const DEFAULT_MIN_TX_DIFFICULTY: u64 = 8;

/// Default transaction timeout (24 hours in seconds).
pub const DEFAULT_TX_TIMEOUT: u64 = 86400;

/// Transaction mempool.
///
/// Holds unconfirmed transactions waiting to be included in blocks.
pub struct Mempool {
    /// All transactions by ID.
    txs: HashMap<[u8; 32], MempoolEntry>,

    /// Index of tx IDs by signer address.
    by_signer: HashMap<[u8; 20], Vec<[u8; 32]>>,

    /// Block hash at which transactions were validated.
    validated_at_tip: [u8; 32],

    /// Maximum number of transactions.
    max_size: usize,

    /// Minimum transaction PoW difficulty accepted.
    min_difficulty: u64,

    /// Transaction timeout in seconds.
    tx_timeout: u64,

    /// Whether devnet mode is enabled (allows shorter verification windows).
    devnet: bool,
}

impl Mempool {
    /// Create a new empty mempool.
    pub fn new(max_size: usize, min_difficulty: u64) -> Self {
        Self {
            txs: HashMap::new(),
            by_signer: HashMap::new(),
            validated_at_tip: [0u8; 32],
            max_size,
            min_difficulty,
            tx_timeout: DEFAULT_TX_TIMEOUT,
            devnet: false,
        }
    }

    /// Create a mempool with default settings.
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MEMPOOL_SIZE, DEFAULT_MIN_TX_DIFFICULTY)
    }

    /// Create a mempool for devnet mode.
    pub fn devnet(max_size: usize, min_difficulty: u64) -> Self {
        Self {
            txs: HashMap::new(),
            by_signer: HashMap::new(),
            validated_at_tip: [0u8; 32],
            max_size,
            min_difficulty,
            tx_timeout: DEFAULT_TX_TIMEOUT,
            devnet: true,
        }
    }

    /// Check if devnet mode is enabled.
    pub fn is_devnet(&self) -> bool {
        self.devnet
    }

    /// Get the number of transactions in the mempool.
    #[inline]
    pub fn len(&self) -> usize {
        self.txs.len()
    }

    /// Check if the mempool is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    /// Check if the mempool contains a transaction.
    pub fn contains(&self, tx_id: &[u8; 32]) -> bool {
        self.txs.contains_key(tx_id)
    }

    /// Get a transaction by ID.
    pub fn get(&self, tx_id: &[u8; 32]) -> Option<&MempoolEntry> {
        self.txs.get(tx_id)
    }

    /// Add a transaction to the mempool.
    ///
    /// The transaction is validated against the current state before being added.
    pub fn add_transaction(
        &mut self,
        tx: SignedTransaction,
        state: &mut ProtocolState,
        tip: &[u8; 32],
        current_time: u64,
    ) -> ChainResult<[u8; 32]> {
        let tx_id = tx.id();

        // Check if already in pool
        if self.txs.contains_key(&tx_id) {
            return Err(ChainError::TransactionAlreadyInPool { tx_id });
        }

        // Check mempool capacity
        if self.txs.len() >= self.max_size {
            return Err(ChainError::MempoolFull {
                capacity: self.max_size,
            });
        }

        // Verify signature
        if tx.verify_signature().is_err() {
            return Err(ChainError::TransactionValidationFailed {
                tx_id,
                error: "invalid signature".to_string(),
            });
        }

        // Verify PoW meets minimum difficulty
        let tx_difficulty = state.get_tx_difficulty_state().current_tx_difficulty;
        let required_difficulty = tx_difficulty.max(self.min_difficulty);

        if !tx.verify_pow(required_difficulty) {
            return Err(ChainError::InsufficientTxPow {
                required: required_difficulty,
                actual: count_leading_zero_bits(&tx.pow_hash),
            });
        }

        // Validate against state (using a clone to not modify actual state)
        let mut test_state = state.clone();
        if self.devnet {
            let ctx = ExecutionContext::devnet(current_time, 0, self.min_difficulty);
            apply_transaction_with_context(&mut test_state, &tx, &ctx).map_err(|e| {
                ChainError::TransactionValidationFailed {
                    tx_id,
                    error: e.to_string(),
                }
            })?;
        } else {
            apply_transaction(&mut test_state, &tx, current_time).map_err(|e| {
                ChainError::TransactionValidationFailed {
                    tx_id,
                    error: e.to_string(),
                }
            })?;
        }

        // Create entry and add to mempool
        let entry = MempoolEntry::new(tx, current_time);
        let signer = entry.signer_address;

        self.txs.insert(tx_id, entry);
        self.by_signer.entry(signer).or_default().push(tx_id);
        self.validated_at_tip = *tip;

        Ok(tx_id)
    }

    /// Remove a transaction from the mempool.
    pub fn remove_transaction(&mut self, tx_id: &[u8; 32]) -> Option<MempoolEntry> {
        if let Some(entry) = self.txs.remove(tx_id) {
            // Remove from signer index
            if let Some(tx_ids) = self.by_signer.get_mut(&entry.signer_address) {
                tx_ids.retain(|id| id != tx_id);
                if tx_ids.is_empty() {
                    self.by_signer.remove(&entry.signer_address);
                }
            }
            Some(entry)
        } else {
            None
        }
    }

    /// Get transactions for block production.
    ///
    /// Returns transactions sorted by PoW difficulty (higher first),
    /// up to the given maximum block size.
    pub fn get_transactions_for_block(&self, max_size: usize) -> Vec<SignedTransaction> {
        let mut entries: Vec<&MempoolEntry> = self.txs.values().collect();

        // Sort by PoW difficulty (higher is better)
        entries.sort_by(|a, b| b.pow_difficulty.cmp(&a.pow_difficulty));

        let mut result = Vec::new();
        let mut total_size = 0;

        for entry in entries {
            if total_size + entry.size > max_size {
                continue;
            }
            result.push(entry.tx.clone());
            total_size += entry.size;
        }

        result
    }

    /// Remove transactions that were included in a block.
    pub fn remove_confirmed(&mut self, tx_ids: &[[u8; 32]]) {
        for tx_id in tx_ids {
            self.remove_transaction(tx_id);
        }
    }

    /// Remove expired transactions.
    pub fn remove_expired(&mut self, current_time: u64) {
        let expired: Vec<[u8; 32]> = self
            .txs
            .iter()
            .filter(|(_, entry)| current_time.saturating_sub(entry.added_at) > self.tx_timeout)
            .map(|(id, _)| *id)
            .collect();

        for tx_id in expired {
            self.remove_transaction(&tx_id);
        }
    }

    /// Revalidate all transactions against a new state.
    ///
    /// Removes transactions that are no longer valid.
    pub fn revalidate(&mut self, state: &ProtocolState, new_tip: &[u8; 32], current_time: u64) {
        if self.validated_at_tip == *new_tip {
            return; // Already validated at this tip
        }

        let mut invalid = Vec::new();

        for (tx_id, entry) in &self.txs {
            let mut test_state = state.clone();
            if apply_transaction(&mut test_state, &entry.tx, current_time).is_err() {
                invalid.push(*tx_id);
            }
        }

        for tx_id in invalid {
            self.remove_transaction(&tx_id);
        }

        self.validated_at_tip = *new_tip;
    }

    /// Get all transaction IDs in the mempool.
    pub fn tx_ids(&self) -> Vec<[u8; 32]> {
        self.txs.keys().copied().collect()
    }

    /// Get transactions by signer address.
    pub fn get_by_signer(&self, address: &[u8; 20]) -> Vec<&MempoolEntry> {
        self.by_signer
            .get(address)
            .map(|ids| ids.iter().filter_map(|id| self.txs.get(id)).collect())
            .unwrap_or_default()
    }

    /// Clear all transactions from the mempool.
    pub fn clear(&mut self) {
        self.txs.clear();
        self.by_signer.clear();
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::with_defaults()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::MAX_BLOCK_SIZE;
    use tunnels_core::crypto::{sign, KeyPair};
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::{mine_pow, Transaction};

    fn create_test_tx(kp: &KeyPair, difficulty: u64) -> SignedTransaction {
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
    fn test_mempool_new() {
        let mempool = Mempool::new(100, 8);
        assert!(mempool.is_empty());
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn test_mempool_add_and_remove() {
        let mut mempool = Mempool::new(100, DEFAULT_MIN_TX_DIFFICULTY);
        let mut state = ProtocolState::new();
        let tip = [0u8; 32];
        let kp = KeyPair::generate();
        let tx = create_test_tx(&kp, DEFAULT_MIN_TX_DIFFICULTY);
        let tx_id = tx.id();

        let result = mempool.add_transaction(tx, &mut state, &tip, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tx_id);
        assert_eq!(mempool.len(), 1);
        assert!(mempool.contains(&tx_id));

        let removed = mempool.remove_transaction(&tx_id);
        assert!(removed.is_some());
        assert!(mempool.is_empty());
    }

    #[test]
    fn test_mempool_duplicate_rejected() {
        let mut mempool = Mempool::new(100, DEFAULT_MIN_TX_DIFFICULTY);
        let mut state = ProtocolState::new();
        let tip = [0u8; 32];
        let kp = KeyPair::generate();
        let tx = create_test_tx(&kp, DEFAULT_MIN_TX_DIFFICULTY);

        mempool.add_transaction(tx.clone(), &mut state, &tip, 0).unwrap();
        let result = mempool.add_transaction(tx, &mut state, &tip, 0);

        assert!(matches!(
            result,
            Err(ChainError::TransactionAlreadyInPool { .. })
        ));
    }

    #[test]
    fn test_mempool_capacity() {
        let mut mempool = Mempool::new(2, DEFAULT_MIN_TX_DIFFICULTY);
        let mut state = ProtocolState::new();
        let tip = [0u8; 32];

        // Add two transactions
        for _ in 0..2 {
            let kp = KeyPair::generate();
            let tx = create_test_tx(&kp, DEFAULT_MIN_TX_DIFFICULTY);
            mempool.add_transaction(tx, &mut state, &tip, 0).unwrap();
        }

        // Third should fail
        let kp = KeyPair::generate();
        let tx = create_test_tx(&kp, DEFAULT_MIN_TX_DIFFICULTY);
        let result = mempool.add_transaction(tx, &mut state, &tip, 0);

        assert!(matches!(result, Err(ChainError::MempoolFull { .. })));
    }

    #[test]
    fn test_get_transactions_for_block() {
        let mut mempool = Mempool::new(100, DEFAULT_MIN_TX_DIFFICULTY);
        let mut state = ProtocolState::new();
        let tip = [0u8; 32];

        // Add some transactions
        for _ in 0..5 {
            let kp = KeyPair::generate();
            let tx = create_test_tx(&kp, DEFAULT_MIN_TX_DIFFICULTY);
            mempool.add_transaction(tx, &mut state, &tip, 0).unwrap();
        }

        let txs = mempool.get_transactions_for_block(MAX_BLOCK_SIZE);
        assert_eq!(txs.len(), 5);
    }
}
