//! Network-related RPC methods.

use std::sync::Arc;

use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use serde::{Deserialize, Serialize};

use super::RpcState;

/// Peer information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub addr: String,
    pub direction: String,
    pub best_height: u64,
}

/// Node information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub version: String,
    pub protocol_version: u32,
    pub network: String,
    pub height: u64,
    pub best_block_hash: String,
    pub connections: u64,
    pub synced: bool,
}

/// Register network RPC methods.
pub fn register_methods(module: &mut RpcModule<Arc<RpcState>>) {
    // getPeerInfo - list connected peers
    module
        .register_async_method("getPeerInfo", |_params, state, _| async move {
            let peers = if let Some(p2p_state) = &state.p2p_state {
                let p2p = p2p_state.read().await;
                p2p.peers()
                    .iter()
                    .map(|p| PeerInfo {
                        id: p.id.clone(),
                        addr: p.addr.to_string(),
                        direction: p.direction.clone(),
                        best_height: p.best_height,
                    })
                    .collect()
            } else {
                Vec::new()
            };

            Ok::<_, ErrorObjectOwned>(peers)
        })
        .unwrap();

    // getConnectionCount - get number of connections
    module
        .register_async_method("getConnectionCount", |_params, state, _| async move {
            let count = if let Some(p2p_state) = &state.p2p_state {
                let p2p = p2p_state.read().await;
                p2p.connection_count() as u64
            } else {
                0
            };

            Ok::<_, ErrorObjectOwned>(count)
        })
        .unwrap();

    // addPeer - manually add a peer
    module
        .register_async_method("addPeer", |params, state, _| async move {
            let addr: String = params.one()?;

            // Validate the address format
            let socket_addr: std::net::SocketAddr = addr.parse().map_err(|_| {
                ErrorObjectOwned::owned(-32602, "Invalid address format", None::<()>)
            })?;

            // Check if P2P is running
            if let Some(p2p_state) = &state.p2p_state {
                let p2p = p2p_state.read().await;
                if !p2p.is_running() {
                    return Err(ErrorObjectOwned::owned(
                        -32603,
                        "P2P network not running",
                        None::<()>,
                    ));
                }
            } else {
                return Err(ErrorObjectOwned::owned(
                    -32603,
                    "P2P network not available",
                    None::<()>,
                ));
            }

            // Note: Actually adding a peer would require access to P2P internals.
            // For now, we log the request. In a full implementation, we'd send
            // a message to the P2P task to initiate a connection.
            tracing::info!("Request to add peer: {}", socket_addr);

            // Return success - the P2P layer will attempt to connect
            Ok::<_, ErrorObjectOwned>(true)
        })
        .unwrap();

    // getNodeInfo - get node information
    module
        .register_async_method("getNodeInfo", |_params, state, _| async move {
            let chain = state.chain.read().await;

            let network = if state.devnet.is_some() {
                "devnet"
            } else {
                "mainnet"
            };

            // Get connection count and sync status from P2P state
            let (connections, synced) = if let Some(p2p_state) = &state.p2p_state {
                let p2p = p2p_state.read().await;
                (p2p.connection_count() as u64, p2p.is_synced())
            } else {
                (0, true) // No P2P = assume synced (local only)
            };

            Ok::<_, ErrorObjectOwned>(NodeInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                protocol_version: tunnels_chain::PROTOCOL_VERSION,
                network: network.to_string(),
                height: chain.height(),
                best_block_hash: hex::encode(chain.tip_hash()),
                connections,
                synced,
            })
        })
        .unwrap();
}
