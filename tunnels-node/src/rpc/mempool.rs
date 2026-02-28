//! Mempool-related RPC methods.

use std::sync::Arc;

use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use serde::{Deserialize, Serialize};

use tunnels_core::serialization::deserialize;
use tunnels_core::transaction::SignedTransaction;

use super::RpcState;

/// Mempool information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolInfo {
    pub size: usize,
    pub bytes: usize,
}

/// Register mempool RPC methods.
pub fn register_methods(module: &mut RpcModule<Arc<RpcState>>) {
    // submitTransaction - accepts a signed transaction and adds it to the mempool
    module
        .register_async_method("submitTransaction", |params, state, _| async move {
            let tx_hex: String = params.one()?;

            // Decode the transaction
            let tx_bytes = hex::decode(&tx_hex).map_err(|_| {
                ErrorObjectOwned::owned(-32602, "Invalid hex encoding", None::<()>)
            })?;

            let tx: SignedTransaction = deserialize(&tx_bytes).map_err(|e| {
                ErrorObjectOwned::owned(
                    -32602,
                    format!("Invalid transaction format: {}", e),
                    None::<()>,
                )
            })?;

            // Validate signature
            if tx.verify_signature().is_err() {
                return Err(ErrorObjectOwned::owned(
                    -32003,
                    "Invalid signature",
                    None::<()>,
                ));
            }

            // Add to mempool
            let mut mempool = state.mempool.write().await;
            let chain = state.chain.read().await;

            let tip = chain.tip_hash();
            let mut tip_state = chain.state().clone();
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            drop(chain); // Release read lock before adding

            let tx_id = mempool
                .add_transaction(tx, &mut tip_state, &tip, current_time)
                .map_err(|e| {
                    ErrorObjectOwned::owned(-32003, format!("Transaction rejected: {}", e), None::<()>)
                })?;

            // If auto-mine is enabled and there's a mine channel, trigger mining
            if state.devnet.as_ref().map(|d| d.auto_mine).unwrap_or(false) {
                if let Some(mine_tx) = &state.mine_tx {
                    let (result_tx, _result_rx) = tokio::sync::oneshot::channel();
                    let _ = mine_tx
                        .send(super::MineRequest {
                            count: 1,
                            result_tx,
                        })
                        .await;
                }
            }

            Ok::<_, ErrorObjectOwned>(hex::encode(tx_id))
        })
        .unwrap();

    // getMempoolInfo - returns mempool statistics
    module
        .register_async_method("getMempoolInfo", |_params, state, _| async move {
            let mempool = state.mempool.read().await;

            // Estimate total bytes (rough estimate based on tx count)
            let size = mempool.len();
            let bytes = size * 500; // Rough estimate per transaction

            Ok::<_, ErrorObjectOwned>(MempoolInfo { size, bytes })
        })
        .unwrap();

    // getRawMempool - returns all transaction IDs in the mempool
    module
        .register_async_method("getRawMempool", |_params, state, _| async move {
            let mempool = state.mempool.read().await;
            let tx_ids: Vec<String> = mempool.tx_ids().iter().map(hex::encode).collect();
            Ok::<_, ErrorObjectOwned>(tx_ids)
        })
        .unwrap();
}
