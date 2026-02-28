//! Node orchestrator.
//!
//! Coordinates all node components: chain, mempool, P2P, RPC, and mining.

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use tunnels_chain::{create_genesis_block, produce_block, ChainState, Mempool, PROTOCOL_VERSION};
use tunnels_p2p::{P2pConfig, P2pNode};
use tunnels_storage::{BlockStore, RocksBackend};

use crate::config::NodeConfig;
use crate::rpc::{self, MineRequest, RpcState, SharedP2pState};
use crate::shutdown::{shutdown_channel, wait_for_shutdown_signal, ShutdownTx};

/// The main node structure.
pub struct Node {
    /// Node configuration.
    config: NodeConfig,

    /// Blockchain state.
    chain: Arc<RwLock<ChainState>>,

    /// Transaction mempool.
    mempool: Arc<RwLock<Mempool>>,

    /// Block storage (for persistence).
    block_store: Option<Arc<BlockStore<RocksBackend>>>,

    /// Shared P2P state for RPC queries.
    p2p_state: Arc<RwLock<SharedP2pState>>,

    /// Shutdown signal sender.
    shutdown_tx: ShutdownTx,
}

impl Node {
    /// Create a new node with the given configuration.
    pub async fn new(config: NodeConfig) -> anyhow::Result<Self> {
        // Initialize storage if persistence is enabled
        let is_devnet = config.is_devnet();
        let (chain, block_store) = if config.data_dir.as_os_str().is_empty() {
            // No persistence - use in-memory only
            tracing::info!("Running without persistence (no data directory)");
            let chain = if is_devnet {
                ChainState::devnet()
            } else {
                ChainState::new()
            };
            (chain, None)
        } else {
            // Ensure data directory exists
            std::fs::create_dir_all(&config.data_dir)?;
            tracing::info!("Data directory: {:?}", config.data_dir);

            // Open storage
            let db_path = config.data_dir.join("blocks.db");
            let backend = Arc::new(RocksBackend::open(&db_path)?);
            let block_store = Arc::new(BlockStore::new(backend));

            // Load existing chain or start fresh
            let chain = load_chain_from_storage(&block_store, is_devnet)?;

            (chain, Some(block_store))
        };

        // Verify genesis is correct (chain should have correct genesis at height 0)
        let genesis = create_genesis_block();
        assert_eq!(
            chain.genesis_hash(),
            genesis.header.hash(),
            "Chain should have correct genesis block"
        );

        tracing::info!(
            "Chain initialized at height {} (tip: {})",
            chain.height(),
            hex::encode(chain.tip_hash())
        );

        // Initialize mempool
        let mempool = if let Some(ref devnet) = config.devnet {
            // Use devnet mempool with relaxed constraints
            Mempool::devnet(5000, devnet.tx_difficulty)
        } else {
            Mempool::with_defaults()
        };

        let (shutdown_tx, _) = shutdown_channel();

        // Initialize shared P2P state
        let p2p_state = Arc::new(RwLock::new(SharedP2pState::new()));

        Ok(Self {
            config,
            chain: Arc::new(RwLock::new(chain)),
            mempool: Arc::new(RwLock::new(mempool)),
            block_store,
            p2p_state,
            shutdown_tx,
        })
    }

    /// Run the node.
    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Starting Tunnels node...");
        tracing::info!("  Protocol version: {}", PROTOCOL_VERSION);
        tracing::info!("  P2P address: {}", self.config.p2p_addr);
        tracing::info!("  RPC address: {}", self.config.rpc_addr);
        tracing::info!("  Mining enabled: {}", self.config.mining_enabled);
        tracing::info!("  Devnet mode: {}", self.config.is_devnet());

        // Set up mining channel if mining is enabled
        let (mine_tx, mine_rx) = if self.config.mining_enabled {
            let (tx, rx) = mpsc::channel::<MineRequest>(32);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // Create RPC state
        let rpc_state = Arc::new(
            RpcState::new(
                self.chain.clone(),
                self.mempool.clone(),
                self.config.devnet.clone(),
            )
            .with_p2p_state(self.p2p_state.clone())
            .with_mine_tx(mine_tx.clone().unwrap_or_else(|| {
                let (tx, _) = mpsc::channel(1);
                tx
            })),
        );

        // Start RPC server
        let rpc_handle =
            rpc::start_rpc_server(self.config.rpc_addr, rpc_state, self.shutdown_tx.clone())
                .await?;

        tracing::info!("RPC server listening on {}", self.config.rpc_addr);

        // Start P2P node
        let p2p_shutdown_handle = self.start_p2p().await;

        // Start mining task if enabled
        let mining_handle = if let Some(rx) = mine_rx {
            let chain = self.chain.clone();
            let mempool = self.mempool.clone();
            let block_store = self.block_store.clone();
            let shutdown_tx = self.shutdown_tx.clone();

            Some(tokio::spawn(async move {
                run_mining_task(chain, mempool, block_store, rx, shutdown_tx).await;
            }))
        } else {
            None
        };

        // Wait for shutdown signal
        wait_for_shutdown_signal().await;

        // Initiate shutdown
        tracing::info!("Shutting down node...");
        let _ = self.shutdown_tx.send(());

        // Stop P2P
        if let Some(handle) = p2p_shutdown_handle {
            let _ = handle.send(()).await;
            tracing::info!("P2P shutdown signal sent");
        }

        // Stop RPC server
        rpc_handle.stop()?;
        tracing::info!("RPC server stopped");

        // Wait for mining task to finish
        if let Some(handle) = mining_handle {
            let _ = handle.await;
            tracing::info!("Mining task stopped");
        }

        tracing::info!("Node shutdown complete");
        Ok(())
    }

    /// Start the P2P node in a background task.
    async fn start_p2p(&self) -> Option<mpsc::Sender<()>> {
        // Build P2P config
        let mut p2p_config = P2pConfig::new(self.config.p2p_addr);

        // Add seed nodes
        for seed in &self.config.seed_nodes {
            p2p_config.bootstrap_peers.push(*seed);
        }

        // Adjust for devnet if needed
        if self.config.is_devnet() {
            p2p_config.connect_timeout = std::time::Duration::from_secs(2);
            p2p_config.handshake_timeout = std::time::Duration::from_secs(2);
        }

        // Create P2P node
        let p2p_node = P2pNode::new(
            p2p_config,
            self.chain.clone(),
            self.mempool.clone(),
        );

        let shutdown_handle = p2p_node.shutdown_handle();
        let p2p_state = self.p2p_state.clone();

        // Update P2P state to indicate we're starting
        {
            let mut state = p2p_state.write().await;
            state.set_running(true);
        }

        // Spawn P2P task
        tokio::spawn(async move {
            match p2p_node.run().await {
                Ok(()) => {
                    tracing::info!("P2P node stopped gracefully");
                }
                Err(e) => {
                    tracing::error!("P2P node error: {}", e);
                }
            }

            // Update state to indicate P2P stopped
            let mut state = p2p_state.write().await;
            state.set_running(false);
        });

        tracing::info!("P2P node started on {}", self.config.p2p_addr);

        Some(shutdown_handle)
    }

    /// Get the chain state (for testing).
    #[allow(dead_code)]
    pub fn chain(&self) -> &Arc<RwLock<ChainState>> {
        &self.chain
    }

    /// Get the mempool (for testing).
    #[allow(dead_code)]
    pub fn mempool(&self) -> &Arc<RwLock<Mempool>> {
        &self.mempool
    }

    /// Get the P2P state (for testing).
    #[allow(dead_code)]
    pub fn p2p_state(&self) -> &Arc<RwLock<SharedP2pState>> {
        &self.p2p_state
    }

    /// Get the node configuration.
    #[allow(dead_code)]
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Start the P2P node and return the bound address.
    /// This is useful for tests that need to know the actual port when using port 0.
    /// Returns (bound_address, shutdown_handle, block_submit_sender).
    #[allow(dead_code)]
    pub async fn start_p2p_with_addr(&self) -> anyhow::Result<(std::net::SocketAddr, mpsc::Sender<()>, mpsc::Sender<tunnels_core::block::Block>)> {
        // Build P2P config
        let mut p2p_config = P2pConfig::new(self.config.p2p_addr);

        // Add seed nodes
        for seed in &self.config.seed_nodes {
            p2p_config.bootstrap_peers.push(*seed);
        }

        // Adjust for devnet if needed
        if self.config.is_devnet() {
            p2p_config.connect_timeout = std::time::Duration::from_secs(2);
            p2p_config.handshake_timeout = std::time::Duration::from_secs(2);
        }

        // Create P2P node
        let mut p2p_node = P2pNode::new(
            p2p_config,
            self.chain.clone(),
            self.mempool.clone(),
        );

        // Get the bound address receiver BEFORE running
        let addr_rx = p2p_node.bound_addr_receiver();
        // Get state updates receiver to forward to SharedP2pState
        let mut state_rx = p2p_node.state_updates_receiver();
        // Get block submit sender for broadcasting mined blocks
        let block_submit_tx = p2p_node.block_submit_sender();
        let shutdown_handle = p2p_node.shutdown_handle();
        let p2p_state = self.p2p_state.clone();
        let p2p_state_for_updates = self.p2p_state.clone();

        // Update P2P state to indicate we're starting
        {
            let mut state = p2p_state.write().await;
            state.set_running(true);
        }

        // Spawn task to forward P2P state updates to SharedP2pState
        tokio::spawn(async move {
            while let Some(update) = state_rx.recv().await {
                let mut state = p2p_state_for_updates.write().await;
                // Convert P2pStateUpdate to SharedP2pState updates
                state.set_connections(update.outbound_connections, update.inbound_connections);
                state.set_sync_state(update.syncing, update.synced);
                // Convert peer snapshots
                let peers: Vec<rpc::PeerSnapshot> = update.peers.into_iter().map(|p| {
                    rpc::PeerSnapshot {
                        id: p.id,
                        addr: p.addr,
                        direction: p.direction,
                        best_height: p.best_height,
                    }
                }).collect();
                state.set_peers(peers);
            }
        });

        // Spawn P2P task
        tokio::spawn(async move {
            match p2p_node.run().await {
                Ok(()) => {
                    tracing::info!("P2P node stopped gracefully");
                }
                Err(e) => {
                    tracing::error!("P2P node error: {}", e);
                }
            }

            // Update state to indicate P2P stopped
            let mut state = p2p_state.write().await;
            state.set_running(false);
        });

        // Wait for the bound address
        let bound_addr = addr_rx.await
            .map_err(|_| anyhow::anyhow!("Failed to receive bound P2P address"))?;

        tracing::info!("P2P node started on {}", bound_addr);

        Ok((bound_addr, shutdown_handle, block_submit_tx))
    }

    /// Start the RPC server and return the handle.
    /// The handle provides the local_addr() for the actual bound port.
    #[allow(dead_code)]
    pub async fn start_rpc(&self) -> anyhow::Result<rpc::RpcServerHandle> {
        // Set up mining channel
        let (mine_tx, _mine_rx) = mpsc::channel::<MineRequest>(32);

        // Create RPC state
        let rpc_state = Arc::new(
            RpcState::new(
                self.chain.clone(),
                self.mempool.clone(),
                self.config.devnet.clone(),
            )
            .with_p2p_state(self.p2p_state.clone())
            .with_mine_tx(mine_tx),
        );

        // Start RPC server
        let handle = rpc::start_rpc_server(
            self.config.rpc_addr,
            rpc_state,
            self.shutdown_tx.clone(),
        ).await?;

        tracing::info!("RPC server listening on {}", handle.local_addr());

        Ok(handle)
    }

    /// Get the block store (for testing persistence).
    #[allow(dead_code)]
    pub fn block_store(&self) -> Option<&Arc<BlockStore<RocksBackend>>> {
        self.block_store.as_ref()
    }
}

/// Load chain state from storage, replaying blocks if needed.
fn load_chain_from_storage(
    block_store: &BlockStore<RocksBackend>,
    is_devnet: bool,
) -> anyhow::Result<ChainState> {
    // Check if we have any blocks stored
    let latest_height = block_store.get_latest_height()?;

    match latest_height {
        None => {
            // No blocks stored - start fresh with genesis
            tracing::info!("No stored blocks found, starting from genesis");
            let chain = if is_devnet {
                ChainState::devnet()
            } else {
                ChainState::new()
            };

            // Store genesis block
            let genesis = create_genesis_block();
            let state_root = [0u8; 32]; // Genesis state root
            block_store.store_block(&genesis, state_root)?;

            Ok(chain)
        }
        Some(height) => {
            // Replay blocks from storage
            tracing::info!("Found {} blocks in storage, replaying...", height + 1);
            let mut chain = if is_devnet {
                ChainState::devnet()
            } else {
                ChainState::new()
            };

            // Skip genesis (height 0) since ChainState::new() already includes it
            for h in 1..=height {
                let block = block_store
                    .get_block_by_height(h)?
                    .ok_or_else(|| anyhow::anyhow!("Block at height {} not found", h))?;

                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                chain.add_block(block, current_time)?;
            }

            tracing::info!("Replayed {} blocks, tip at height {}", height, chain.height());
            Ok(chain)
        }
    }
}

/// Run the mining task.
async fn run_mining_task(
    chain: Arc<RwLock<ChainState>>,
    mempool: Arc<RwLock<Mempool>>,
    block_store: Option<Arc<BlockStore<RocksBackend>>>,
    mut rx: mpsc::Receiver<MineRequest>,
    shutdown_tx: ShutdownTx,
) {
    let mut shutdown_rx = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                tracing::info!("Mining task received shutdown signal");
                break;
            }
            Some(request) = rx.recv() => {
                let hashes = mine_blocks(&chain, &mempool, block_store.as_ref(), request.count).await;
                let _ = request.result_tx.send(hashes);
            }
        }
    }
}

/// Mine the specified number of blocks.
async fn mine_blocks(
    chain: &Arc<RwLock<ChainState>>,
    mempool: &Arc<RwLock<Mempool>>,
    block_store: Option<&Arc<BlockStore<RocksBackend>>>,
    count: u64,
) -> Vec<[u8; 32]> {
    let mut hashes = Vec::new();

    for _ in 0..count {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Produce block using the standard API
        let block = {
            let chain_read = chain.read().await;
            let mempool_read = mempool.read().await;

            match produce_block(&chain_read, &mempool_read, current_time) {
                Ok(block) => block,
                Err(e) => {
                    tracing::error!("Failed to produce block: {}", e);
                    break;
                }
            }
        };

        let block_hash = block.header.hash();
        let tx_ids: Vec<[u8; 32]> = block.transactions.iter().map(|tx| tx.id()).collect();

        // Add block to chain
        {
            let mut chain_write = chain.write().await;
            match chain_write.add_block(block.clone(), current_time) {
                Ok(_) => {
                    tracing::info!(
                        "Mined block {} at height {} with {} transactions",
                        hex::encode(block_hash),
                        block.header.height,
                        block.transactions.len()
                    );
                    hashes.push(block_hash);
                }
                Err(e) => {
                    tracing::error!("Failed to add mined block: {}", e);
                    break;
                }
            }
        }

        // Persist block to storage
        if let Some(store) = block_store {
            let state_root = [0u8; 32]; // TODO: Compute actual state root
            if let Err(e) = store.store_block(&block, state_root) {
                tracing::error!("Failed to persist block: {}", e);
                // Continue anyway - in-memory state is still valid
            }
        }

        // Remove confirmed transactions from mempool
        {
            let mut mempool_write = mempool.write().await;
            mempool_write.remove_confirmed(&tx_ids);
        }
    }

    hashes
}
