//! Main P2P node orchestrator.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;

use tunnels_chain::{ChainState, Mempool};
use tunnels_core::block::Block;
use tunnels_core::transaction::SignedTransaction;

use crate::config::P2pConfig;
use crate::discovery::Discovery;
use crate::error::P2pResult;
use crate::gossip::GossipManager;
use crate::manager::{connect_to_peer, ConnectResult, PeerManager};
use crate::peer::{ConnectionDirection, PeerContext, PeerEvent, PeerId, PeerInfo};
use crate::protocol::Message;
use crate::sync::{respond_to_get_headers, SyncManager};

/// State update sent from P2P node to external observers.
#[derive(Debug, Clone)]
pub struct P2pStateUpdate {
    /// Number of outbound connections.
    pub outbound_connections: usize,
    /// Number of inbound connections.
    pub inbound_connections: usize,
    /// Whether the node is syncing.
    pub syncing: bool,
    /// Whether the node is synced.
    pub synced: bool,
    /// List of connected peer info.
    pub peers: Vec<PeerSnapshot>,
}

/// Snapshot of peer information for external reporting.
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

/// Discovery interval.
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);

/// Peer save interval.
const SAVE_INTERVAL: Duration = Duration::from_secs(300);

/// Sync stall check interval.
const STALL_CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// Main P2P node.
pub struct P2pNode {
    /// P2P configuration.
    config: Arc<P2pConfig>,
    /// Blockchain state.
    chain: Arc<RwLock<ChainState>>,
    /// Transaction mempool.
    mempool: Arc<RwLock<Mempool>>,
    /// Peer manager - THE authoritative source for peer state.
    peers: PeerManager,
    /// Sync manager.
    sync: SyncManager,
    /// Gossip manager.
    gossip: GossipManager,
    /// Discovery coordinator.
    discovery: Discovery,
    /// Genesis hash.
    genesis_hash: [u8; 32],
    /// Shutdown signal receiver.
    shutdown_rx: Option<mpsc::Receiver<()>>,
    /// Shutdown signal sender (for cloning).
    shutdown_tx: mpsc::Sender<()>,
    /// Channel to send the bound address when the node starts.
    bound_addr_tx: Option<tokio::sync::oneshot::Sender<std::net::SocketAddr>>,
    /// Channel to send state updates to external observers.
    state_tx: Option<mpsc::Sender<P2pStateUpdate>>,
    /// Channel to receive blocks to broadcast (from external miner).
    block_submit_rx: Option<mpsc::Receiver<Block>>,
    /// Peer contexts for tracking misbehavior.
    peer_contexts: HashMap<PeerId, PeerContext>,
    /// JoinHandles for peer tasks (for graceful shutdown).
    peer_tasks: HashMap<PeerId, JoinHandle<()>>,
}

impl P2pNode {
    /// Create a new P2P node.
    pub fn new(
        config: P2pConfig,
        chain: Arc<RwLock<ChainState>>,
        mempool: Arc<RwLock<Mempool>>,
    ) -> Self {
        let config = Arc::new(config);
        let genesis_hash = {
            // We'll get this from chain when we start
            [0u8; 32]
        };

        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Self {
            config: config.clone(),
            chain,
            mempool,
            peers: PeerManager::new(config.clone()),
            sync: SyncManager::new(),
            gossip: GossipManager::new(),
            discovery: Discovery::new(&config),
            genesis_hash,
            shutdown_rx: Some(shutdown_rx),
            shutdown_tx,
            bound_addr_tx: None,
            state_tx: None,
            block_submit_rx: None,
            peer_contexts: HashMap::new(),
            peer_tasks: HashMap::new(),
        }
    }

    /// Get a oneshot receiver that will receive the bound address when the node starts.
    /// This is useful for tests that need to know the actual port when using port 0.
    pub fn bound_addr_receiver(&mut self) -> tokio::sync::oneshot::Receiver<std::net::SocketAddr> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bound_addr_tx = Some(tx);
        rx
    }

    /// Get a receiver for P2P state updates.
    /// State updates are sent whenever connection state changes.
    pub fn state_updates_receiver(&mut self) -> mpsc::Receiver<P2pStateUpdate> {
        let (tx, rx) = mpsc::channel(16);
        self.state_tx = Some(tx);
        rx
    }

    /// Get a sender for submitting blocks to be broadcast to the network.
    /// Blocks sent through this channel will be broadcast to all connected peers.
    pub fn block_submit_sender(&mut self) -> mpsc::Sender<Block> {
        let (tx, rx) = mpsc::channel(16);
        self.block_submit_rx = Some(rx);
        tx
    }

    /// Get the shutdown sender for external shutdown signals.
    pub fn shutdown_handle(&self) -> mpsc::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Run the P2P node.
    pub async fn run(mut self) -> P2pResult<()> {
        // Get genesis hash
        {
            let chain = self.chain.read().await;
            self.genesis_hash = chain.genesis_hash();
        }

        // Initialize discovery
        self.discovery.initialize().await?;

        // Create event channel
        let (event_tx, mut event_rx) = mpsc::channel::<PeerEvent>(256);

        // Channel for outbound peer task handles
        let (task_tx, mut task_rx) = mpsc::channel::<(PeerId, JoinHandle<()>)>(64);

        // Start listener
        let listener = TcpListener::bind(self.config.bind_addr).await?;
        let local_addr = listener.local_addr()?;
        tracing::info!(addr = %local_addr, "P2P node listening");

        // Send bound address to receiver if one was set up
        if let Some(tx) = self.bound_addr_tx.take() {
            let _ = tx.send(local_addr);
        }

        // Take shutdown receiver
        let mut shutdown_rx = self.shutdown_rx.take().unwrap();

        // Take block submit receiver if configured
        let mut block_submit_rx = self.block_submit_rx.take();

        // Intervals
        let mut discovery_timer = interval(DISCOVERY_INTERVAL);
        let mut save_timer = interval(SAVE_INTERVAL);
        let mut stall_timer = interval(STALL_CHECK_INTERVAL);

        // Initial connection attempt
        self.try_connect_peers(event_tx.clone(), task_tx.clone()).await;

        loop {
            tokio::select! {
                // Handle shutdown
                _ = shutdown_rx.recv() => {
                    tracing::info!("P2P node shutting down");
                    self.shutdown_peers().await;
                    break;
                }

                // Handle block submissions from external miner
                Some(block) = async {
                    match block_submit_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.handle_block_submission(block).await;
                }

                // Accept inbound connections
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            if !self.peers.can_accept_inbound() {
                                tracing::debug!(addr = %addr, "Rejecting inbound: no slots");
                                continue;
                            }

                            let (height, hash) = self.get_chain_tip().await;
                            let peer_id = self.peers.next_peer_id();

                            if let Err(e) = stream.set_nodelay(true) {
                                tracing::warn!(error = %e, "Failed to set TCP_NODELAY");
                            }

                            let (cmd_tx, handle) = crate::peer::spawn_peer_connection(
                                peer_id,
                                addr,
                                ConnectionDirection::Inbound,
                                stream,
                                event_tx.clone(),
                                self.config.clone(),
                                self.genesis_hash,
                                height,
                                hash,
                            );

                            // Register with PeerManager immediately with preliminary info
                            // Full info will be updated on Connected event
                            let preliminary_info = PeerInfo::new(peer_id, addr, ConnectionDirection::Inbound);
                            self.peers.add_peer(preliminary_info, cmd_tx);
                            self.peer_tasks.insert(peer_id, handle);
                            tracing::debug!(peer = %peer_id, addr = %addr, "Accepted inbound");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Accept error");
                        }
                    }
                }

                // Handle peer events
                Some(event) = event_rx.recv() => {
                    self.handle_peer_event(event, event_tx.clone()).await;
                }

                // Handle outbound task handles
                Some((peer_id, handle)) = task_rx.recv() => {
                    self.peer_tasks.insert(peer_id, handle);
                }

                // Discovery timer
                _ = discovery_timer.tick() => {
                    self.try_connect_peers(event_tx.clone(), task_tx.clone()).await;
                }

                // Save timer
                _ = save_timer.tick() => {
                    if let Err(e) = self.discovery.save_if_dirty().await {
                        tracing::warn!(error = %e, "Failed to save peers");
                    }
                }

                // Stall check timer
                _ = stall_timer.tick() => {
                    self.check_sync_stall(event_tx.clone(), task_tx.clone()).await;
                }
            }
        }

        // Save peers on shutdown
        let _ = self.discovery.save_if_dirty().await;

        Ok(())
    }

    /// Get current chain tip.
    async fn get_chain_tip(&self) -> (u64, [u8; 32]) {
        let chain = self.chain.read().await;
        (chain.height(), chain.tip_hash())
    }

    /// Try to connect to more peers.
    async fn try_connect_peers(
        &mut self,
        event_tx: mpsc::Sender<PeerEvent>,
        task_tx: mpsc::Sender<(PeerId, JoinHandle<()>)>,
    ) {
        if !self.peers.needs_outbound() {
            return;
        }

        let connected: HashSet<_> = self.peers.peers().map(|p| p.addr).collect();
        let needed = self.config.target_outbound.saturating_sub(self.peers.connection_counts().0);
        let addrs = self.discovery.get_addresses_to_connect(&connected, needed);

        let (height, hash) = self.get_chain_tip().await;

        for addr in addrs {
            if !self.peers.should_connect(&addr) {
                continue;
            }

            let peer_id = self.peers.next_peer_id();
            self.peers.start_connecting(addr);

            let config = self.config.clone();
            let event_tx = event_tx.clone();
            let genesis_hash = self.genesis_hash;
            let task_tx = task_tx.clone();

            tokio::spawn(async move {
                let result = connect_to_peer(
                    peer_id,
                    addr,
                    config,
                    event_tx.clone(),
                    genesis_hash,
                    height,
                    hash,
                )
                .await;

                match result {
                    ConnectResult::Success(id, handle) => {
                        tracing::debug!(peer = %id, addr = %addr, "Outbound connection initiated");
                        // Send handle back to node for tracking
                        let _ = task_tx.send((id, handle)).await;
                    }
                    ConnectResult::Failed(addr, e) => {
                        tracing::debug!(addr = %addr, error = %e, "Outbound connection failed");
                    }
                }
            });
        }
    }

    /// Check for stalled sync and switch to a different peer if needed.
    async fn check_sync_stall(
        &mut self,
        event_tx: mpsc::Sender<PeerEvent>,
        task_tx: mpsc::Sender<(PeerId, JoinHandle<()>)>,
    ) {
        if !self.sync.is_stalled() {
            return;
        }

        let stalled_peer = match self.sync.sync_peer() {
            Some(peer) => peer,
            None => return,
        };

        tracing::warn!(
            peer = %stalled_peer,
            elapsed = ?self.sync.time_since_activity(),
            "Sync stalled, switching to different peer"
        );

        // Disconnect the stalled peer
        let _ = self.peers.disconnect_peer(&stalled_peer).await;

        // Reset sync state
        self.sync.reset();

        // Find a better peer to sync from
        let our_height = self.chain.read().await.height();
        if let Some(best_peer) = self.peers.best_peer() {
            if let Some(info) = self.peers.get_peer(&best_peer) {
                if info.best_height > our_height {
                    self.start_sync(best_peer, our_height, info.best_height).await;
                }
            }
        } else {
            // No good peer available, try to connect to more peers
            self.try_connect_peers(event_tx, task_tx).await;
        }
    }

    /// Handle a peer event.
    async fn handle_peer_event(&mut self, event: PeerEvent, _event_tx: mpsc::Sender<PeerEvent>) {
        match event {
            PeerEvent::Connected { peer_id, info, command_tx } => {
                self.handle_peer_connected(peer_id, info, command_tx).await;
            }
            PeerEvent::Message { peer_id, message } => {
                self.handle_peer_message(peer_id, *message).await;
            }
            PeerEvent::Disconnected { peer_id, reason } => {
                self.handle_peer_disconnected(peer_id, &reason).await;
            }
            PeerEvent::HandshakeFailed { peer_id, addr, error } => {
                tracing::debug!(peer = %peer_id, addr = %addr, error, "Handshake failed");
                self.peers.stop_connecting(&addr);
                self.peers.remove_peer(&peer_id);
                self.peer_contexts.remove(&peer_id);
            }
        }
    }

    /// Handle a new peer connection.
    async fn handle_peer_connected(
        &mut self,
        peer_id: PeerId,
        info: PeerInfo,
        command_tx: mpsc::UnboundedSender<crate::peer::PeerCommand>,
    ) {
        tracing::info!(
            peer = %peer_id,
            addr = %info.addr,
            height = info.best_height,
            "Peer connected"
        );

        self.discovery.mark_seen(info.addr);
        self.peers.stop_connecting(&info.addr);

        // Register or update peer in manager
        // For outbound connections, peer may not be registered yet
        // For inbound connections, update with handshake data
        if self.peers.get_peer(&peer_id).is_some() {
            // Update existing peer info
            if let Some(existing) = self.peers.get_peer_mut(&peer_id) {
                existing.best_height = info.best_height;
                existing.best_hash = info.best_hash;
                existing.protocol_version = info.protocol_version;
                existing.user_agent = info.user_agent.clone();
            }
        } else {
            // Register new peer (outbound connections arrive here)
            self.peers.add_peer(info.clone(), command_tx);
        }

        // Check if we need to sync
        let our_height = self.chain.read().await.height();
        if info.best_height > our_height && !self.sync.is_syncing() {
            self.start_sync(peer_id, our_height, info.best_height).await;
        }

        // Initialize peer context for misbehavior tracking
        self.peer_contexts.insert(peer_id, PeerContext::new());

        // Request peers
        let _ = self.peers.send_to_peer(&peer_id, Message::GetPeers).await;

        // Notify external observers of state change
        self.send_state_update();
    }

    /// Record that a peer sent invalid data and check if they should be banned.
    async fn record_peer_misbehavior(&mut self, peer_id: PeerId, reason: &str) {
        if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
            ctx.record_invalid();
            tracing::warn!(peer = %peer_id, reason, invalid_count = ctx.invalid_messages, "Peer misbehavior recorded");

            if ctx.should_ban() {
                tracing::warn!(peer = %peer_id, "Banning peer due to misbehavior");
                let _ = self.peers.disconnect_peer(&peer_id).await;
                // Could add to a ban list here for future connections
            }
        }
    }

    /// Start syncing from a peer.
    async fn start_sync(&mut self, peer_id: PeerId, our_height: u64, peer_height: u64) {
        self.sync.start_sync(peer_id, our_height, peer_height);

        let chain = self.chain.read().await;
        let request = self.sync.create_headers_request(&chain);
        drop(chain);

        let _ = self.peers.send_to_peer(&peer_id, request).await;
    }

    /// Handle a message from a peer.
    async fn handle_peer_message(&mut self, peer_id: PeerId, message: Message) {
        tracing::trace!(peer = %peer_id, msg = %message, "Received message");

        match message {
            Message::GetHeaders(request) => {
                let chain = self.chain.read().await;
                let response = respond_to_get_headers(&chain, &request);
                drop(chain);

                let _ = self.peers.send_to_peer(&peer_id, Message::Headers(response)).await;
            }

            Message::Headers(response) => {
                if self.sync.sync_peer() == Some(peer_id) {
                    self.handle_headers_response(peer_id, response).await;
                }
            }

            Message::GetBlock(request) => {
                let chain = self.chain.read().await;
                if let Some(stored) = chain.get_block(&request.hash) {
                    let response = Message::BlockData(crate::protocol::BlockDataMessage {
                        block: stored.block.clone(),
                    });
                    drop(chain);
                    let _ = self.peers.send_to_peer(&peer_id, response).await;
                } else {
                    drop(chain);
                    let response = Message::NotFound(crate::protocol::NotFoundMessage {
                        item_type: "block".to_string(),
                        hash: request.hash,
                    });
                    let _ = self.peers.send_to_peer(&peer_id, response).await;
                }
            }

            Message::BlockData(response) => {
                self.handle_block_data(peer_id, response.block).await;
            }

            Message::NewTransaction(msg) => {
                self.handle_new_transaction(peer_id, msg.transaction).await;
            }

            Message::NewBlock(msg) => {
                self.handle_new_block(peer_id, msg.block).await;
            }

            Message::GetPeers => {
                let response = self.discovery.exchange().create_peers_response();
                let _ = self.peers.send_to_peer(&peer_id, response).await;
            }

            Message::Peers(response) => {
                self.discovery.add_addresses(response.peers);
            }

            _ => {
                // Ping/Pong handled in connection
            }
        }
    }

    /// Handle headers response during sync.
    async fn handle_headers_response(&mut self, peer_id: PeerId, response: crate::protocol::HeadersMessage) {
        let result = {
            let chain = self.chain.read().await;
            self.sync.process_headers(&chain, &response)
        };

        match result {
            Ok(()) => {
                // If we got headers, start requesting blocks
                if !response.headers.is_empty() {
                    self.request_next_blocks(peer_id).await;
                }

                // If headers are empty and we have no pending blocks, we're done with headers
                // The sync manager handles state transitions
                if response.headers.is_empty() && !self.sync.has_pending_blocks() {
                    if self.sync.is_synced() {
                        tracing::info!("Sync complete");
                    }
                } else if !response.headers.is_empty() {
                    // More headers might be available, request them
                    let chain = self.chain.read().await;
                    let request = self.sync.create_headers_request(&chain);
                    drop(chain);

                    let _ = self.peers.send_to_peer(&peer_id, request).await;
                }
            }
            Err(e) => {
                tracing::warn!(peer = %peer_id, error = %e, "Headers validation failed");
                self.record_peer_misbehavior(peer_id, "invalid headers").await;
                self.sync.peer_disconnected(&peer_id);
            }
        }
    }

    /// Request the next batch of blocks from sync peer.
    async fn request_next_blocks(&mut self, peer_id: PeerId) {
        let requests = self.sync.next_block_requests(peer_id, 16);

        for request in requests {
            let _ = self.peers.send_to_peer(&peer_id, request).await;
        }
    }

    /// Handle received block data.
    async fn handle_block_data(&mut self, peer_id: PeerId, block: Block) {
        let hash = block.hash();
        let height = block.header.height;

        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut chain = self.chain.write().await;

        if chain.has_block(&hash) {
            drop(chain);
            self.sync.process_block(&hash, height);
            // Continue requesting blocks
            if self.sync.has_pending_blocks() {
                self.request_next_blocks(peer_id).await;
            }
            return;
        }

        match chain.add_block(block, current_time) {
            Ok(()) => {
                tracing::debug!(hash = ?&hash[..8], height, "Added synced block");
                drop(chain);

                self.sync.process_block(&hash, height);
                self.gossip.mark_block_seen(hash);

                // Process any orphans that might now be valid
                self.process_orphans().await;

                // Continue requesting more blocks if syncing
                if self.sync.has_pending_blocks() {
                    self.request_next_blocks(peer_id).await;
                } else if self.sync.is_synced() {
                    tracing::info!(height, "Sync complete");
                }
            }
            Err(e) => {
                tracing::warn!(hash = ?&hash[..8], error = %e, "Failed to add block");
            }
        }
    }

    /// Handle a new transaction announcement.
    async fn handle_new_transaction(&mut self, from_peer: PeerId, tx: SignedTransaction) {
        match self.gossip.process_transaction(tx, &self.mempool, &self.chain).await {
            Ok(Some((_tx_id, message))) => {
                // Relay to other peers via PeerManager
                self.peers.broadcast_except(message, &from_peer).await;
            }
            Ok(None) => {
                // Already seen
            }
            Err(e) => {
                tracing::debug!(peer = %from_peer, error = %e, "Transaction rejected");
                self.record_peer_misbehavior(from_peer, "invalid transaction").await;
            }
        }
    }

    /// Handle a new block announcement.
    async fn handle_new_block(&mut self, from_peer: PeerId, block: Block) {
        let hash = block.hash();
        let parent_hash = block.header.prev_block_hash;

        // Check if parent exists
        let parent_exists = {
            let chain = self.chain.read().await;
            chain.has_block(&parent_hash)
        };

        if !parent_exists {
            // This is an orphan block - queue it and trigger sync
            tracing::debug!(
                hash = ?&hash[..8],
                parent = ?&parent_hash[..8],
                "Received orphan block, queuing and triggering sync"
            );
            self.gossip.queue_orphan(block);

            // Trigger sync from this peer if not already syncing
            if !self.sync.is_syncing() {
                if let Some(peer_info) = self.peers.get_peer(&from_peer) {
                    let our_height = self.chain.read().await.height();
                    if peer_info.best_height > our_height {
                        self.start_sync(from_peer, our_height, peer_info.best_height).await;
                    }
                }
            }
            return;
        }

        match self.gossip.process_block(block, &self.chain).await {
            Ok(Some((hash, message))) => {
                // Update peer's best height
                let new_height = self.chain.read().await.height();
                self.peers.update_peer_tip(&from_peer, new_height, hash);

                // Relay to other peers via PeerManager
                self.peers.broadcast_except(message, &from_peer).await;

                // Process any orphans that might now be valid
                self.process_orphans().await;

                // Remove confirmed transactions from mempool
                self.remove_confirmed_transactions(&hash).await;
            }
            Ok(None) => {
                // Already seen
            }
            Err(e) => {
                tracing::debug!(peer = %from_peer, error = %e, "Block rejected");
                self.record_peer_misbehavior(from_peer, "invalid block").await;
            }
        }
    }

    /// Process queued orphan blocks that might now have their parents.
    async fn process_orphans(&mut self) {
        loop {
            let orphan = {
                let chain = self.chain.read().await;
                self.gossip.get_processable_orphan(&chain)
            };

            match orphan {
                Some(block) => {
                    let hash = block.hash();
                    match self.gossip.process_block(block, &self.chain).await {
                        Ok(Some((_, message))) => {
                            tracing::debug!(hash = ?&hash[..8], "Processed orphan block");
                            // Broadcast to all peers since we don't know who sent it
                            self.peers.broadcast(message).await;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::debug!(hash = ?&hash[..8], error = %e, "Orphan block rejected");
                        }
                    }
                }
                None => break,
            }
        }
    }

    /// Remove transactions from mempool that were confirmed in a block.
    async fn remove_confirmed_transactions(&self, block_hash: &[u8; 32]) {
        let chain = self.chain.read().await;
        if let Some(stored) = chain.get_block(block_hash) {
            let tx_ids: Vec<[u8; 32]> = stored.block.transactions.iter().map(|tx| tx.id()).collect();
            drop(chain);

            if !tx_ids.is_empty() {
                let mut mempool = self.mempool.write().await;
                mempool.remove_confirmed(&tx_ids);
            }
        }
    }

    /// Handle peer disconnection.
    async fn handle_peer_disconnected(&mut self, peer_id: PeerId, reason: &str) {
        tracing::info!(peer = %peer_id, reason, "Peer disconnected");

        self.peers.remove_peer(&peer_id);
        self.sync.peer_disconnected(&peer_id);
        self.peer_contexts.remove(&peer_id);
        // Remove and drop the task handle (task has already finished)
        self.peer_tasks.remove(&peer_id);

        // Notify external observers of state change
        self.send_state_update();
    }

    /// Handle a block submission from an external miner.
    /// Broadcasts the block to all connected peers.
    async fn handle_block_submission(&mut self, block: Block) {
        let hash = block.hash();
        let height = block.header.height;

        tracing::info!(
            hash = ?&hash[..8],
            height,
            "Broadcasting locally mined block"
        );

        // Mark the block as seen so we don't re-process it
        self.gossip.mark_block_seen(hash);

        // Broadcast to all peers
        let message = self.gossip.create_block_message(block);
        self.peers.broadcast(message).await;
    }

    /// Gracefully shutdown all peers.
    async fn shutdown_peers(&mut self) {
        tracing::info!(count = self.peer_tasks.len(), "Shutting down peer connections");

        // Send disconnect to all peers
        for peer_id in self.peers.peer_ids() {
            let _ = self.peers.disconnect_peer(&peer_id).await;
        }

        // Wait for all peer tasks to complete (with timeout)
        let handles: Vec<_> = self.peer_tasks.drain().map(|(_, h)| h).collect();
        for handle in handles {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }

    /// Broadcast a message to all peers.
    pub async fn broadcast(&self, message: Message) {
        self.peers.broadcast(message).await;
    }

    /// Submit a transaction to the network.
    pub async fn submit_transaction(&mut self, tx: SignedTransaction) -> P2pResult<[u8; 32]> {
        let tx_id = tx.id();

        // Add to our mempool first
        {
            let mut mempool = self.mempool.write().await;
            let chain = self.chain.read().await;
            let tip = chain.tip_hash();
            let mut state = chain.state().clone();
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            mempool.add_transaction(tx.clone(), &mut state, &tip, current_time)?;
        }

        // Broadcast via PeerManager
        let message = self.gossip.create_tx_message(tx);
        self.peers.broadcast(message).await;

        Ok(tx_id)
    }

    /// Submit a block to the network.
    pub async fn submit_block(&mut self, block: Block) -> P2pResult<[u8; 32]> {
        let hash = block.hash();

        // Add to our chain first
        {
            let mut chain = self.chain.write().await;
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            chain.add_block(block.clone(), current_time)?;
        }

        // Broadcast via PeerManager
        let message = self.gossip.create_block_message(block);
        self.peers.broadcast(message).await;

        Ok(hash)
    }

    /// Get connection info.
    pub fn connection_info(&self) -> (usize, usize) {
        self.peers.connection_counts()
    }

    /// Check if syncing.
    pub fn is_syncing(&self) -> bool {
        self.sync.is_syncing()
    }

    /// Check if synced.
    pub fn is_synced(&self) -> bool {
        self.sync.is_synced()
    }

    /// Get the number of connected peers.
    pub fn peer_count(&self) -> usize {
        let (outbound, inbound) = self.peers.connection_counts();
        outbound + inbound
    }

    /// Send a state update to external observers if a channel is configured.
    fn send_state_update(&self) {
        if let Some(ref tx) = self.state_tx {
            let (outbound, inbound) = self.peers.connection_counts();

            // Build peer snapshots
            let peers: Vec<PeerSnapshot> = self.peers.peers()
                .map(|p| PeerSnapshot {
                    id: format!("{:016x}", p.id.0),
                    addr: p.addr,
                    direction: match p.direction {
                        ConnectionDirection::Inbound => "inbound".to_string(),
                        ConnectionDirection::Outbound => "outbound".to_string(),
                    },
                    best_height: p.best_height,
                })
                .collect();

            let update = P2pStateUpdate {
                outbound_connections: outbound,
                inbound_connections: inbound,
                syncing: self.sync.is_syncing(),
                synced: self.sync.is_synced(),
                peers,
            };

            // Send non-blocking - if the receiver is full or dropped, we just skip
            let _ = tx.try_send(update);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_creation() {
        let config = P2pConfig::new("127.0.0.1:0".parse().unwrap());
        let chain = Arc::new(RwLock::new(ChainState::new()));
        let mempool = Arc::new(RwLock::new(Mempool::with_defaults()));

        let node = P2pNode::new(config, chain, mempool);
        assert_eq!(node.connection_info(), (0, 0));
        assert!(!node.is_syncing());
    }
}
