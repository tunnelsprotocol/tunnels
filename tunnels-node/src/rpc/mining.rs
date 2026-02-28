//! Mining-related RPC methods.

use std::sync::Arc;

use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use serde::{Deserialize, Serialize};

use super::{MineRequest, RpcState};

/// Block template for external miners.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub height: u64,
    pub prev_block_hash: String,
    pub difficulty: u64,
    pub timestamp: u64,
    pub transactions: Vec<String>,
}

/// Register mining RPC methods.
pub fn register_methods(module: &mut RpcModule<Arc<RpcState>>) {
    // mine - mine N blocks (devnet/testing only)
    module
        .register_async_method("mine", |params, state, _| async move {
            // Parse optional count parameter (default to 1)
            let count: u64 = params.one().unwrap_or(1);

            // Check if mining channel is available
            let mine_tx = state.mine_tx.as_ref().ok_or_else(|| {
                ErrorObjectOwned::owned(-32601, "Mining not enabled", None::<()>)
            })?;

            // Send mine request
            let (result_tx, result_rx) = tokio::sync::oneshot::channel();
            mine_tx
                .send(MineRequest { count, result_tx })
                .await
                .map_err(|_| {
                    ErrorObjectOwned::owned(-32603, "Mining service unavailable", None::<()>)
                })?;

            // Wait for result
            let hashes = result_rx.await.map_err(|_| {
                ErrorObjectOwned::owned(-32603, "Mining failed", None::<()>)
            })?;

            Ok::<_, ErrorObjectOwned>(hashes.iter().map(hex::encode).collect::<Vec<_>>())
        })
        .unwrap();

    // getBlockTemplate - get a block template for mining
    module
        .register_async_method("getBlockTemplate", |_params, state, _| async move {
            let chain = state.chain.read().await;
            let mempool = state.mempool.read().await;

            let tip = chain.tip_block();
            let height = chain.height() + 1;
            let prev_block_hash = hex::encode(tip.block.header.hash());

            // Get difficulty for next block
            let difficulty = if let Some(devnet) = &state.devnet {
                devnet.block_difficulty
            } else {
                tunnels_chain::calculate_next_difficulty(&chain, height)
            };

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Get pending transactions
            let max_block_size = tunnels_chain::MAX_BLOCK_SIZE;
            let transactions: Vec<String> = mempool
                .get_transactions_for_block(max_block_size)
                .iter()
                .map(|tx| hex::encode(tx.id()))
                .collect();

            Ok::<_, ErrorObjectOwned>(BlockTemplate {
                height,
                prev_block_hash,
                difficulty,
                timestamp,
                transactions,
            })
        })
        .unwrap();

    // getMiningInfo - get mining status
    module
        .register_async_method("getMiningInfo", |_params, state, _| async move {
            let chain = state.chain.read().await;
            let height = chain.height();
            let difficulty = tunnels_chain::calculate_next_difficulty(&chain, height + 1);
            let is_devnet = state.devnet.is_some();

            #[derive(Clone, Serialize)]
            struct MiningInfo {
                height: u64,
                difficulty: u64,
                devnet: bool,
                mining_enabled: bool,
            }

            Ok::<_, ErrorObjectOwned>(MiningInfo {
                height,
                difficulty,
                devnet: is_devnet,
                mining_enabled: state.mine_tx.is_some(),
            })
        })
        .unwrap();
}
