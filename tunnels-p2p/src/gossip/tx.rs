//! Transaction gossip.

use std::sync::Arc;

use tokio::sync::RwLock;
use tunnels_chain::{ChainState, Mempool};
use tunnels_core::transaction::SignedTransaction;

use crate::error::{P2pError, P2pResult};
use crate::gossip::SeenFilter;
use crate::protocol::{Message, NewTransactionMessage};

/// Transaction relay manager.
pub struct TxRelay {
    /// Filter for already-seen transactions.
    seen: SeenFilter,
}

impl TxRelay {
    /// Create a new transaction relay manager.
    pub fn new() -> Self {
        Self {
            seen: SeenFilter::with_default_capacity(),
        }
    }

    /// Create a new transaction relay with custom capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: SeenFilter::new(capacity),
        }
    }

    /// Check if a transaction should be relayed (not seen before).
    pub fn should_relay(&mut self, tx_id: &[u8; 32]) -> bool {
        self.seen.check(tx_id)
    }

    /// Mark a transaction as seen without checking.
    pub fn mark_seen(&mut self, tx_id: [u8; 32]) {
        self.seen.mark_seen(tx_id);
    }

    /// Check if we've seen a transaction.
    pub fn has_seen(&self, tx_id: &[u8; 32]) -> bool {
        self.seen.contains(tx_id)
    }

    /// Process a received transaction.
    ///
    /// Returns Ok(Some(tx)) if the transaction is new and valid.
    /// Returns Ok(None) if the transaction was already seen.
    /// Returns Err if validation fails.
    pub async fn process_transaction(
        &mut self,
        tx: SignedTransaction,
        mempool: &Arc<RwLock<Mempool>>,
        chain: &Arc<RwLock<ChainState>>,
        current_time: u64,
    ) -> P2pResult<Option<[u8; 32]>> {
        let tx_id = tx.id();

        // Check if already seen
        if !self.should_relay(&tx_id) {
            return Ok(None);
        }

        // Try to add to mempool
        let mut mempool_guard = mempool.write().await;
        let chain_guard = chain.read().await;
        let tip = chain_guard.tip_hash();
        let mut state = chain_guard.state().clone();

        match mempool_guard.add_transaction(tx, &mut state, &tip, current_time) {
            Ok(_) => {
                tracing::debug!(tx_id = ?&tx_id[..8], "Added transaction to mempool");
                Ok(Some(tx_id))
            }
            Err(e) => {
                tracing::debug!(tx_id = ?&tx_id[..8], error = %e, "Transaction rejected");
                Err(P2pError::TransactionValidationFailed(e.to_string()))
            }
        }
    }

    /// Create a NewTransaction message for broadcasting.
    pub fn create_message(tx: SignedTransaction) -> Message {
        Message::NewTransaction(NewTransactionMessage { transaction: tx })
    }

    /// Get the number of seen transactions.
    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }

    /// Clear the seen filter.
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

impl Default for TxRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_relay_dedup() {
        let mut relay = TxRelay::new();
        let tx_id = [1u8; 32];

        // First time should allow relay
        assert!(relay.should_relay(&tx_id));

        // Second time should not
        assert!(!relay.should_relay(&tx_id));
    }

    #[test]
    fn test_mark_seen() {
        let mut relay = TxRelay::new();
        let tx_id = [1u8; 32];

        relay.mark_seen(tx_id);
        assert!(relay.has_seen(&tx_id));
        assert!(!relay.should_relay(&tx_id));
    }
}
