//! Chain-related RPC methods.

use std::sync::Arc;

use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use serde::{Deserialize, Serialize};

use tunnels_core::block::{Block, BlockHeader};
use tunnels_core::transaction::SignedTransaction;

use super::RpcState;

/// Block information returned by RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub hash: String,
    pub height: u64,
    pub version: u32,
    pub prev_block_hash: String,
    pub tx_root: String,
    pub state_root: String,
    pub timestamp: u64,
    pub difficulty: u64,
    pub nonce: u64,
    pub tx_count: usize,
    pub transactions: Vec<String>,
}

impl From<&Block> for BlockInfo {
    fn from(block: &Block) -> Self {
        Self {
            hash: hex::encode(block.header.hash()),
            height: block.header.height,
            version: block.header.version,
            prev_block_hash: hex::encode(block.header.prev_block_hash),
            tx_root: hex::encode(block.header.tx_root),
            state_root: hex::encode(block.header.state_root),
            timestamp: block.header.timestamp,
            difficulty: block.header.difficulty,
            nonce: block.header.nonce,
            tx_count: block.transactions.len(),
            transactions: block.transactions.iter().map(|tx| hex::encode(tx.id())).collect(),
        }
    }
}

/// Block header information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct BlockHeaderInfo {
    pub hash: String,
    pub height: u64,
    pub version: u32,
    pub prev_block_hash: String,
    pub tx_root: String,
    pub state_root: String,
    pub timestamp: u64,
    pub difficulty: u64,
    pub nonce: u64,
}

impl From<&BlockHeader> for BlockHeaderInfo {
    fn from(header: &BlockHeader) -> Self {
        Self {
            hash: hex::encode(header.hash()),
            height: header.height,
            version: header.version,
            prev_block_hash: hex::encode(header.prev_block_hash),
            tx_root: hex::encode(header.tx_root),
            state_root: hex::encode(header.state_root),
            timestamp: header.timestamp,
            difficulty: header.difficulty,
            nonce: header.nonce,
        }
    }
}

/// Transaction information returned by RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub id: String,
    pub signer: String,
    pub signature: String,
    pub pow_nonce: u64,
    pub pow_hash: String,
    pub tx_type: String,
}

impl From<&SignedTransaction> for TransactionInfo {
    fn from(tx: &SignedTransaction) -> Self {
        Self {
            id: hex::encode(tx.id()),
            signer: hex::encode(tx.signer.as_bytes()),
            signature: hex::encode(tx.signature.to_bytes()),
            pow_nonce: tx.pow_nonce,
            pow_hash: hex::encode(tx.pow_hash),
            tx_type: tx.tx.name().to_string(),
        }
    }
}

/// Register chain RPC methods.
pub fn register_methods(module: &mut RpcModule<Arc<RpcState>>) {
    module
        .register_async_method("getBlockCount", |_params, state, _| async move {
            let chain = state.chain.read().await;
            Ok::<_, ErrorObjectOwned>(chain.height())
        })
        .unwrap();

    module
        .register_async_method("getBlockHash", |params, state, _| async move {
            let height: u64 = params.one()?;
            let chain = state.chain.read().await;

            chain
                .get_block_at_height(height)
                .map(|b| hex::encode(b.block.header.hash()))
                .ok_or_else(|| {
                    ErrorObjectOwned::owned(
                        -32001,
                        format!("Block not found at height {}", height),
                        None::<()>,
                    )
                })
        })
        .unwrap();

    module
        .register_async_method("getBlock", |params, state, _| async move {
            let hash_hex: String = params.one()?;
            let hash = parse_hash(&hash_hex)?;
            let chain = state.chain.read().await;

            chain
                .get_block(&hash)
                .map(|stored| BlockInfo::from(&stored.block))
                .ok_or_else(|| {
                    ErrorObjectOwned::owned(-32001, "Block not found", None::<()>)
                })
        })
        .unwrap();

    module
        .register_async_method("getBestBlockHash", |_params, state, _| async move {
            let chain = state.chain.read().await;
            Ok::<_, ErrorObjectOwned>(hex::encode(chain.tip_hash()))
        })
        .unwrap();

    module
        .register_async_method("getTransaction", |params, state, _| async move {
            let hash_hex: String = params.one()?;
            let hash = parse_hash(&hash_hex)?;
            let chain = state.chain.read().await;

            // Search through blocks for the transaction
            for height in (0..=chain.height()).rev() {
                if let Some(stored) = chain.get_block_at_height(height) {
                    for tx in &stored.block.transactions {
                        if tx.id() == hash {
                            return Ok(TransactionInfo::from(tx));
                        }
                    }
                }
            }

            Err(ErrorObjectOwned::owned(
                -32001,
                "Transaction not found",
                None::<()>,
            ))
        })
        .unwrap();
}

/// Parse a hex string into a 32-byte hash.
fn parse_hash(hex_str: &str) -> Result<[u8; 32], ErrorObjectOwned> {
    let bytes = hex::decode(hex_str).map_err(|_| {
        ErrorObjectOwned::owned(-32602, "Invalid hex string", None::<()>)
    })?;

    if bytes.len() != 32 {
        return Err(ErrorObjectOwned::owned(
            -32602,
            "Hash must be 32 bytes",
            None::<()>,
        ));
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}
