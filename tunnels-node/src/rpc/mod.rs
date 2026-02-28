//! JSON-RPC server.

pub mod chain;
pub mod mempool;
pub mod mining;
pub mod network;
pub mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::RpcModule;
use tokio::sync::RwLock;

use tunnels_chain::{ChainState, Mempool};
use tunnels_state::ProtocolState;

use crate::devnet::DevnetConfig;
use crate::shutdown::ShutdownTx;

/// Shared P2P state for RPC queries.
///
/// This struct tracks P2P network state that can be queried via RPC.
/// It's updated by the P2P task and read by RPC handlers.
#[derive(Debug, Clone)]
pub struct SharedP2pState {
    /// Whether the P2P node is running.
    running: bool,
    /// Number of outbound connections.
    outbound_connections: usize,
    /// Number of inbound connections.
    inbound_connections: usize,
    /// Whether the node is syncing.
    syncing: bool,
    /// Whether the node is synced.
    synced: bool,
    /// Connected peer info.
    peers: Vec<PeerSnapshot>,
}

/// Snapshot of peer information.
#[derive(Debug, Clone)]
pub struct PeerSnapshot {
    /// Peer ID (hex-encoded).
    pub id: String,
    /// Peer address.
    pub addr: SocketAddr,
    /// Connection direction.
    pub direction: String,
    /// Peer's best known height.
    pub best_height: u64,
}

impl SharedP2pState {
    /// Create a new empty P2P state.
    pub fn new() -> Self {
        Self {
            running: false,
            outbound_connections: 0,
            inbound_connections: 0,
            syncing: false,
            synced: false,
            peers: Vec::new(),
        }
    }

    /// Set whether P2P is running.
    pub fn set_running(&mut self, running: bool) {
        self.running = running;
        if !running {
            // Clear peer state when stopped
            self.outbound_connections = 0;
            self.inbound_connections = 0;
            self.peers.clear();
        }
    }

    /// Update connection counts.
    #[allow(dead_code)]
    pub fn set_connections(&mut self, outbound: usize, inbound: usize) {
        self.outbound_connections = outbound;
        self.inbound_connections = inbound;
    }

    /// Update sync state.
    #[allow(dead_code)]
    pub fn set_sync_state(&mut self, syncing: bool, synced: bool) {
        self.syncing = syncing;
        self.synced = synced;
    }

    /// Update peer list.
    #[allow(dead_code)]
    pub fn set_peers(&mut self, peers: Vec<PeerSnapshot>) {
        self.peers = peers;
    }

    /// Check if P2P is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get total connection count.
    pub fn connection_count(&self) -> usize {
        self.outbound_connections + self.inbound_connections
    }

    /// Get connection counts (outbound, inbound).
    #[allow(dead_code)]
    pub fn connection_counts(&self) -> (usize, usize) {
        (self.outbound_connections, self.inbound_connections)
    }

    /// Check if syncing.
    #[allow(dead_code)]
    pub fn is_syncing(&self) -> bool {
        self.syncing
    }

    /// Check if synced.
    pub fn is_synced(&self) -> bool {
        // Consider synced if running and not actively syncing
        self.running && !self.syncing
    }

    /// Get peer snapshots.
    pub fn peers(&self) -> &[PeerSnapshot] {
        &self.peers
    }
}

impl Default for SharedP2pState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared state for RPC handlers.
pub struct RpcState {
    /// Chain state.
    pub chain: Arc<RwLock<ChainState>>,

    /// Transaction mempool.
    pub mempool: Arc<RwLock<Mempool>>,

    /// Devnet configuration (if enabled).
    pub devnet: Option<DevnetConfig>,

    /// Channel to request mining.
    pub mine_tx: Option<tokio::sync::mpsc::Sender<MineRequest>>,

    /// Shared P2P state.
    pub p2p_state: Option<Arc<RwLock<SharedP2pState>>>,
}

/// Request to mine blocks.
#[derive(Debug)]
pub struct MineRequest {
    /// Number of blocks to mine.
    pub count: u64,

    /// Channel to send back the mined block hashes.
    pub result_tx: tokio::sync::oneshot::Sender<Vec<[u8; 32]>>,
}

impl RpcState {
    /// Create new RPC state.
    pub fn new(
        chain: Arc<RwLock<ChainState>>,
        mempool: Arc<RwLock<Mempool>>,
        devnet: Option<DevnetConfig>,
    ) -> Self {
        Self {
            chain,
            mempool,
            devnet,
            mine_tx: None,
            p2p_state: None,
        }
    }

    /// Set the mining request channel.
    pub fn with_mine_tx(mut self, tx: tokio::sync::mpsc::Sender<MineRequest>) -> Self {
        self.mine_tx = Some(tx);
        self
    }

    /// Set the shared P2P state.
    pub fn with_p2p_state(mut self, p2p_state: Arc<RwLock<SharedP2pState>>) -> Self {
        self.p2p_state = Some(p2p_state);
        self
    }

    /// Get protocol state from chain.
    #[allow(dead_code)]
    pub async fn get_protocol_state(&self) -> ProtocolState {
        let chain = self.chain.read().await;
        chain.state().clone()
    }
}

/// Build the complete RPC module with all methods.
pub fn build_rpc_module(state: Arc<RpcState>) -> RpcModule<Arc<RpcState>> {
    let mut module = RpcModule::new(state.clone());

    // Register chain methods
    chain::register_methods(&mut module);

    // Register mempool methods
    mempool::register_methods(&mut module);

    // Register mining methods
    mining::register_methods(&mut module);

    // Register network methods
    network::register_methods(&mut module);

    // Register protocol state methods
    state::register_methods(&mut module);

    module
}

/// RPC server handle with local address.
#[allow(dead_code)]
pub struct RpcServerHandle {
    /// The server handle for shutdown.
    handle: ServerHandle,
    /// The local address the server is bound to.
    local_addr: SocketAddr,
}

impl RpcServerHandle {
    /// Get the local address the server is bound to.
    #[allow(dead_code)]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop the server.
    pub fn stop(&self) -> Result<(), anyhow::Error> {
        self.handle.stop().map_err(|e| anyhow::anyhow!("Failed to stop server: {:?}", e))
    }
}

/// Start the JSON-RPC server.
pub async fn start_rpc_server(
    addr: SocketAddr,
    state: Arc<RpcState>,
    _shutdown_tx: ShutdownTx,
) -> anyhow::Result<RpcServerHandle> {
    let server = ServerBuilder::default().build(addr).await?;
    let local_addr = server.local_addr()?;

    let module = build_rpc_module(state);

    tracing::info!("Starting JSON-RPC server on {}", local_addr);

    let handle = server.start(module);

    Ok(RpcServerHandle { handle, local_addr })
}
