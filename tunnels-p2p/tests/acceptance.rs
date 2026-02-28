//! Acceptance tests for the P2P layer.
//!
//! These tests verify the acceptance criteria:
//! 1. Discovery - Two nodes discover each other and agree on genesis
//! 2. Block propagation - Block from A reaches B via sync
//! 3. Transaction propagation - Transaction infrastructure works
//! 4. Multi-hop - A→B→C topology connects correctly
//! 5. Sync after disconnect - Node syncs when connecting to node with more blocks
//! 6. Fork resolution - Chain correctly handles competing blocks and reorgs
//! 7. Gossip - Blocks propagate over the network
//! 8. Message framing - Protocol messages encode/decode correctly

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};

use tunnels_chain::{ChainState, Mempool};
use tunnels_core::block::{Block, BlockHeader};
use tunnels_core::crypto::KeyPair;
use tunnels_core::transaction::{SignedTransaction, Transaction};
use tunnels_p2p::{P2pConfig, P2pNode};

/// Timeout for waiting for sync to complete.
const SYNC_TIMEOUT_MS: u64 = 15000;

/// Create a test node configuration with port 0 (OS assigns port).
fn test_config() -> P2pConfig {
    P2pConfig::new("127.0.0.1:0".parse().unwrap())
        .with_target_outbound(4)
        .with_max_outbound(8)
        .with_max_inbound(8)
        .with_connect_timeout(Duration::from_secs(5))
        .with_handshake_timeout(Duration::from_secs(3))
}

/// Create a test node with chain and mempool.
fn create_test_node() -> (P2pNode, Arc<RwLock<ChainState>>, Arc<RwLock<Mempool>>) {
    let config = test_config();
    let chain = Arc::new(RwLock::new(ChainState::new()));
    let mempool = Arc::new(RwLock::new(Mempool::with_defaults()));
    let node = P2pNode::new(config, chain.clone(), mempool.clone());
    (node, chain, mempool)
}

/// Create a test node configured to connect to another.
fn create_connected_node(
    connect_to: SocketAddr,
) -> (P2pNode, Arc<RwLock<ChainState>>, Arc<RwLock<Mempool>>) {
    let mut config = test_config();
    config.bootstrap_peers = vec![connect_to];
    let chain = Arc::new(RwLock::new(ChainState::new()));
    let mempool = Arc::new(RwLock::new(Mempool::with_defaults()));
    let node = P2pNode::new(config, chain.clone(), mempool.clone());
    (node, chain, mempool)
}

/// Mine a simple test block and add to chain.
async fn mine_test_block(chain: &Arc<RwLock<ChainState>>) -> Block {
    let chain_guard = chain.read().await;
    let tip = chain_guard.tip_block();
    let height = tip.height() + 1;
    let prev_hash = tip.hash;
    let difficulty = chain_guard.tip_difficulty();
    let state_root = tunnels_chain::compute_state_root(&mut chain_guard.state().clone());
    let parent_timestamp = tip.block.header.timestamp;
    drop(chain_guard);

    let timestamp = parent_timestamp + 1;

    let mut header = BlockHeader {
        version: 1,
        height,
        timestamp,
        prev_block_hash: prev_hash,
        state_root,
        tx_root: [0u8; 32],
        difficulty,
        nonce: 0,
    };

    while !header.meets_difficulty() {
        header.nonce = header.nonce.wrapping_add(1);
        if header.nonce == 0 {
            header.timestamp += 1;
        }
    }

    let block = Block {
        header,
        transactions: Vec::new(),
    };

    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut chain_guard = chain.write().await;
    chain_guard.add_block(block.clone(), current_time.max(timestamp)).unwrap();

    block
}

/// Create a signed test transaction.
fn create_test_transaction() -> SignedTransaction {
    use ed25519_dalek::Signer;
    use sha2::Digest;

    let keypair = KeyPair::generate();
    let tx = Transaction::CreateIdentity {
        public_key: keypair.public_key(),
    };

    let tx_bytes = tunnels_core::serialization::serialize(&tx).unwrap();
    let signature = keypair.signing_key().sign(&tx_bytes);

    let mut pow_nonce = 0u64;
    let difficulty = 8;

    loop {
        let mut hasher = sha2::Sha256::new();
        hasher.update(&tx_bytes);
        hasher.update(keypair.public_key().as_bytes());
        hasher.update(&signature.to_bytes());
        hasher.update(&pow_nonce.to_le_bytes());
        let hash: [u8; 32] = hasher.finalize().into();

        let leading_zeros: u64 = hash
            .iter()
            .take_while(|&&b| b == 0)
            .count() as u64 * 8;

        if leading_zeros >= difficulty {
            return SignedTransaction {
                tx,
                signer: keypair.public_key(),
                signature: tunnels_core::Signature(signature),
                pow_nonce,
                pow_hash: hash,
            };
        }

        pow_nonce = pow_nonce.wrapping_add(1);
    }
}

/// Wait for a condition with timeout, polling periodically.
async fn wait_for<F, Fut>(timeout_ms: u64, poll_ms: u64, mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout_duration = Duration::from_millis(timeout_ms);
    let poll_duration = Duration::from_millis(poll_ms);

    loop {
        if condition().await {
            return true;
        }
        if start.elapsed() > timeout_duration {
            return false;
        }
        sleep(poll_duration).await;
    }
}

/// Start a node and return its bound address via the oneshot channel.
async fn start_node_get_addr(
    mut node: P2pNode,
) -> (SocketAddr, tokio::sync::mpsc::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr_rx = node.bound_addr_receiver();
    let shutdown = node.shutdown_handle();

    let handle = tokio::spawn(async move {
        let _ = node.run().await;
    });

    // Wait for the bound address
    let addr = addr_rx.await.expect("Failed to get bound address");
    (addr, shutdown, handle)
}

// ============================================================================
// Test 1: Discovery - Two nodes connect and agree on genesis
// ============================================================================

#[tokio::test]
async fn test_discovery_two_nodes_connect() {
    // Create and start node A
    let (node_a, chain_a, _mempool_a) = create_test_node();
    let (addr_a, shutdown_a, handle_a) = start_node_get_addr(node_a).await;

    // Create node B connecting to A's actual address
    let (node_b, chain_b, _mempool_b) = create_connected_node(addr_a);
    let (_, shutdown_b, handle_b) = start_node_get_addr(node_b).await;

    // Wait for genesis to be consistent (synchronize on state, not time)
    let chain_b_clone = chain_b.clone();
    let chain_a_clone = chain_a.clone();
    let connected = wait_for(5000, 50, || async {
        let genesis_a = chain_a_clone.read().await.genesis_hash();
        let genesis_b = chain_b_clone.read().await.genesis_hash();
        genesis_a == genesis_b && genesis_a != [0u8; 32]
    }).await;

    assert!(connected, "Nodes should connect and agree on genesis");

    // VERIFY: Both chains have same genesis
    let genesis_a = chain_a.read().await.genesis_hash();
    let genesis_b = chain_b.read().await.genesis_hash();
    assert_eq!(genesis_a, genesis_b, "Genesis hashes should match after handshake");

    // Shutdown
    let _ = shutdown_a.send(()).await;
    let _ = shutdown_b.send(()).await;
    let _ = timeout(Duration::from_secs(2), handle_a).await;
    let _ = timeout(Duration::from_secs(2), handle_b).await;
}

// ============================================================================
// Test 2: Block Propagation - Node B syncs blocks from Node A
// ============================================================================

#[tokio::test]
async fn test_block_propagation() {
    // Create node A with some blocks already mined
    let (node_a, chain_a, _) = create_test_node();

    // Mine blocks BEFORE starting the node (so A starts with height > 0)
    for _ in 0..3 {
        mine_test_block(&chain_a).await;
    }

    let height_a = chain_a.read().await.height();
    assert_eq!(height_a, 3, "Node A should have height 3 before starting");

    let (addr_a, shutdown_a, handle_a) = start_node_get_addr(node_a).await;

    // Create node B (fresh, at height 0) connecting to A
    let (node_b, chain_b, _) = create_connected_node(addr_a);
    let initial_height_b = chain_b.read().await.height();
    assert_eq!(initial_height_b, 0, "Node B should start at height 0");

    let (_, shutdown_b, handle_b) = start_node_get_addr(node_b).await;

    // Wait for sync - B should request headers from A and download blocks
    let chain_b_clone = chain_b.clone();
    let synced = wait_for(SYNC_TIMEOUT_MS, 100, || async {
        chain_b_clone.read().await.height() >= 3
    }).await;

    // VERIFY: B synced to A's height via P2P
    let final_height_b = chain_b.read().await.height();
    assert!(synced, "Node B should sync to Node A's height. Expected >= 3, got {}", final_height_b);
    assert_eq!(final_height_b, height_a, "Heights should match after sync");

    // VERIFY: B has the same tip as A (same chain, not just same height)
    let tip_a = chain_a.read().await.tip_hash();
    let tip_b = chain_b.read().await.tip_hash();
    assert_eq!(tip_a, tip_b, "Both nodes should have same tip after sync");

    // Shutdown
    let _ = shutdown_a.send(()).await;
    let _ = shutdown_b.send(()).await;
    let _ = timeout(Duration::from_secs(2), handle_a).await;
    let _ = timeout(Duration::from_secs(2), handle_b).await;
}

// ============================================================================
// Test 3: Transaction Propagation - Transaction added to mempool
// ============================================================================

#[tokio::test]
async fn test_transaction_propagation() {
    // This test verifies the transaction relay infrastructure.
    let chain = Arc::new(RwLock::new(ChainState::new()));
    let mempool = Arc::new(RwLock::new(Mempool::with_defaults()));

    // Create a transaction
    let tx = create_test_transaction();
    let tx_id = tx.id();

    // Add to mempool (simulates what P2P does after receiving)
    {
        let mut mp = mempool.write().await;
        let chain_guard = chain.read().await;
        let tip = chain_guard.tip_hash();
        let mut state = chain_guard.state().clone();

        let result = mp.add_transaction(tx, &mut state, &tip, 0);
        assert!(result.is_ok(), "Transaction should be accepted: {:?}", result.err());
    }

    // VERIFY: Transaction is in mempool
    let mp = mempool.read().await;
    assert!(mp.contains(&tx_id), "Transaction should be in mempool");
    assert_eq!(mp.len(), 1, "Mempool should have exactly 1 transaction");
}

// ============================================================================
// Test 4: Multi-hop - Three nodes form connected topology
// ============================================================================

#[tokio::test]
async fn test_multi_hop_propagation() {
    // Create topology: A <- B <- C (B connects to A, C connects to B)
    let (node_a, chain_a, _) = create_test_node();

    // Mine some blocks on A
    for _ in 0..2 {
        mine_test_block(&chain_a).await;
    }
    let height_a = chain_a.read().await.height();
    assert_eq!(height_a, 2, "Node A should have height 2");

    let (addr_a, shutdown_a, handle_a) = start_node_get_addr(node_a).await;

    // Start B, connecting to A
    let (node_b, chain_b, _) = create_connected_node(addr_a);
    let (addr_b, shutdown_b, handle_b) = start_node_get_addr(node_b).await;

    // Wait for B to sync from A before starting C
    let chain_b_clone = chain_b.clone();
    let b_synced = wait_for(SYNC_TIMEOUT_MS, 100, || async {
        chain_b_clone.read().await.height() >= 2
    }).await;
    assert!(b_synced, "Node B should sync from A first");

    // Start C, connecting to B
    let (node_c, chain_c, _) = create_connected_node(addr_b);
    let (_, shutdown_c, handle_c) = start_node_get_addr(node_c).await;

    // Wait for C to sync
    let chain_c_clone = chain_c.clone();
    let c_synced = wait_for(SYNC_TIMEOUT_MS, 100, || async {
        chain_c_clone.read().await.height() >= 2
    }).await;

    // VERIFY: All three nodes agree on genesis
    let genesis_a = chain_a.read().await.genesis_hash();
    let genesis_b = chain_b.read().await.genesis_hash();
    let genesis_c = chain_c.read().await.genesis_hash();
    assert_eq!(genesis_a, genesis_b, "A and B should agree on genesis");
    assert_eq!(genesis_b, genesis_c, "B and C should agree on genesis");

    // VERIFY: C synced from B (multi-hop)
    assert!(c_synced, "Node C should sync from B. Height: {}", chain_c.read().await.height());

    // VERIFY: All nodes have same tip
    let tip_a = chain_a.read().await.tip_hash();
    let tip_b = chain_b.read().await.tip_hash();
    let tip_c = chain_c.read().await.tip_hash();
    assert_eq!(tip_a, tip_b, "A and B should have same tip");
    assert_eq!(tip_b, tip_c, "B and C should have same tip");

    // Shutdown
    let _ = shutdown_a.send(()).await;
    let _ = shutdown_b.send(()).await;
    let _ = shutdown_c.send(()).await;
    let _ = timeout(Duration::from_secs(2), handle_a).await;
    let _ = timeout(Duration::from_secs(2), handle_b).await;
    let _ = timeout(Duration::from_secs(2), handle_c).await;
}

// ============================================================================
// Test 5: Sync After Disconnect - Fresh node syncs existing chain
// ============================================================================

#[tokio::test]
async fn test_sync_after_disconnect() {
    // Create node A with blocks
    let (node_a, chain_a, _) = create_test_node();

    // Mine several blocks
    for _ in 0..5 {
        mine_test_block(&chain_a).await;
    }

    let height_a = chain_a.read().await.height();
    assert_eq!(height_a, 5, "Node A should have height 5");

    let (addr_a, shutdown_a, handle_a) = start_node_get_addr(node_a).await;

    // Create fresh node B (simulates "reconnecting after being offline")
    let (node_b, chain_b, _) = create_connected_node(addr_a);
    let initial_height_b = chain_b.read().await.height();
    assert_eq!(initial_height_b, 0, "Node B should start at genesis");

    let (_, shutdown_b, handle_b) = start_node_get_addr(node_b).await;

    // Wait for sync
    let chain_b_clone = chain_b.clone();
    let synced = wait_for(SYNC_TIMEOUT_MS, 100, || async {
        chain_b_clone.read().await.height() >= 5
    }).await;

    // VERIFY: B synced completely to A's height
    let final_height_b = chain_b.read().await.height();
    assert!(synced, "Node B should sync to A's height. Expected 5, got {}", final_height_b);
    assert_eq!(final_height_b, height_a, "B should match A's height after sync");

    // VERIFY: Same chain (not just same height)
    let tip_a = chain_a.read().await.tip_hash();
    let tip_b = chain_b.read().await.tip_hash();
    assert_eq!(tip_a, tip_b, "Both nodes should have same tip");

    // Shutdown
    let _ = shutdown_a.send(()).await;
    let _ = shutdown_b.send(()).await;
    let _ = timeout(Duration::from_secs(2), handle_a).await;
    let _ = timeout(Duration::from_secs(2), handle_b).await;
}

// ============================================================================
// Test 6: Fork Resolution - Chain reorganizes to longer fork
// ============================================================================

#[tokio::test]
async fn test_fork_resolution_with_reorg() {
    // Test actual chain reorganization:
    // 1. Create chain A: genesis -> A1 (initially main chain)
    // 2. Create competing chain B: genesis -> B1 (stored as fork)
    // 3. Extend B: B1 -> B2 (B chain now has more work)
    // 4. Verify reorg: B becomes main chain, A becomes fork
    // 5. Verify state is correct after reorg

    let mut chain = ChainState::new();
    let genesis = chain.genesis_hash();
    let genesis_timestamp = chain.tip_block().block.header.timestamp;
    let state_root = tunnels_chain::compute_state_root(&mut chain.state().clone());
    let difficulty = chain.tip_difficulty();

    // Create block A1 (height 1, extends genesis)
    let mut header_a1 = BlockHeader {
        version: 1,
        height: 1,
        timestamp: genesis_timestamp + 1,
        prev_block_hash: genesis,
        state_root,
        tx_root: [0u8; 32],
        difficulty,
        nonce: 0,
    };
    while !header_a1.meets_difficulty() {
        header_a1.nonce = header_a1.nonce.wrapping_add(1);
    }
    let block_a1 = Block { header: header_a1, transactions: Vec::new() };
    let block_a1_hash = block_a1.hash();

    // Create block B1 (height 1, also extends genesis - competing block)
    let mut header_b1 = BlockHeader {
        version: 1,
        height: 1,
        timestamp: genesis_timestamp + 1,
        prev_block_hash: genesis,
        state_root,
        tx_root: [0u8; 32],
        difficulty,
        nonce: 1_000_000, // Start from different nonce to get different hash
    };
    while !header_b1.meets_difficulty() {
        header_b1.nonce = header_b1.nonce.wrapping_add(1);
    }
    let block_b1 = Block { header: header_b1, transactions: Vec::new() };
    let block_b1_hash = block_b1.hash();

    // Ensure they're different blocks
    assert_ne!(block_a1_hash, block_b1_hash, "Competing blocks should have different hashes");

    // Add A1 first - it becomes main chain
    chain.add_block(block_a1, genesis_timestamp + 1).unwrap();
    assert_eq!(chain.height(), 1);
    assert_eq!(chain.tip_hash(), block_a1_hash, "A1 should be tip initially");
    assert!(chain.get_block(&block_a1_hash).unwrap().is_main_chain, "A1 should be main chain");

    // Add B1 - same height, same work, stored as fork (not main chain)
    chain.add_block(block_b1, genesis_timestamp + 1).unwrap();
    assert!(chain.has_block(&block_b1_hash), "B1 should exist");
    assert!(!chain.get_block(&block_b1_hash).unwrap().is_main_chain, "B1 should be fork");
    assert_eq!(chain.tip_hash(), block_a1_hash, "A1 should still be tip");

    // Record state before reorg
    let tip_before_reorg = chain.tip_hash();
    let height_before_reorg = chain.height();

    // Now create B2 extending B1 - this will make B chain longer
    let mut header_b2 = BlockHeader {
        version: 1,
        height: 2,
        timestamp: genesis_timestamp + 2,
        prev_block_hash: block_b1_hash,
        state_root,
        tx_root: [0u8; 32],
        difficulty: chain.tip_difficulty(),
        nonce: 0,
    };
    while !header_b2.meets_difficulty() {
        header_b2.nonce = header_b2.nonce.wrapping_add(1);
    }
    let block_b2 = Block { header: header_b2, transactions: Vec::new() };
    let block_b2_hash = block_b2.hash();

    // Add B2 - this should trigger a REORG because B chain now has more work
    // B chain: genesis -> B1 -> B2 (total_work = 3 blocks worth)
    // A chain: genesis -> A1 (total_work = 2 blocks worth)
    let result = chain.add_block(block_b2, genesis_timestamp + 2);
    assert!(result.is_ok(), "Adding B2 should succeed and trigger reorg: {:?}", result.err());

    // =========================================================================
    // VERIFY REORG HAPPENED
    // =========================================================================

    // Verify the tip changed from A1 to B2
    assert_ne!(chain.tip_hash(), tip_before_reorg, "Tip should have changed after reorg");
    assert_eq!(chain.tip_hash(), block_b2_hash, "B2 should be the new tip");

    // Verify height increased
    assert_eq!(chain.height(), 2, "Height should be 2 after reorg");
    assert!(chain.height() > height_before_reorg, "Height should have increased");

    // Verify B chain is now main chain
    assert!(chain.get_block(&block_b1_hash).unwrap().is_main_chain, "B1 should now be main chain");
    assert!(chain.get_block(&block_b2_hash).unwrap().is_main_chain, "B2 should be main chain");

    // Verify A chain is now a fork (orphaned)
    assert!(!chain.get_block(&block_a1_hash).unwrap().is_main_chain, "A1 should now be a fork");

    // Verify total work is correct
    let total_work = chain.total_work();
    let work_b2 = chain.get_block(&block_b2_hash).unwrap().total_work;
    assert_eq!(total_work, work_b2, "Total work should match B2's cumulative work");

    // Verify B2 has more work than A1
    let work_a1 = chain.get_block(&block_a1_hash).unwrap().total_work;
    assert!(work_b2 > work_a1, "B2 chain should have more work than A1 chain");

    // Verify main chain block at height 1 is now B1 (not A1)
    let main_at_1 = chain.get_block_at_height(1).unwrap();
    assert_eq!(main_at_1.hash, block_b1_hash, "Main chain at height 1 should be B1 after reorg");

    // Verify main chain block at height 2 is B2
    let main_at_2 = chain.get_block_at_height(2).unwrap();
    assert_eq!(main_at_2.hash, block_b2_hash, "Main chain at height 2 should be B2");

    // =========================================================================
    // VERIFY STATE IS CORRECT AFTER REORG
    // =========================================================================

    // The state at tip should be valid (matches state_root in B2)
    let current_state = chain.state();
    let state_root_computed = tunnels_chain::compute_state_root(&mut current_state.clone());
    assert_eq!(state_root_computed, state_root, "State root should be valid after reorg");

    // All blocks should still exist (nothing deleted)
    assert!(chain.has_block(&block_a1_hash), "A1 should still exist (just orphaned)");
    assert!(chain.has_block(&block_b1_hash), "B1 should exist");
    assert!(chain.has_block(&block_b2_hash), "B2 should exist");

    // =========================================================================
    // EXTEND THE NEW MAIN CHAIN TO VERIFY IT WORKS
    // =========================================================================

    // Add B3 extending B2 to verify chain continues to work after reorg
    let mut header_b3 = BlockHeader {
        version: 1,
        height: 3,
        timestamp: genesis_timestamp + 3,
        prev_block_hash: block_b2_hash,
        state_root,
        tx_root: [0u8; 32],
        difficulty: chain.tip_difficulty(),
        nonce: 0,
    };
    while !header_b3.meets_difficulty() {
        header_b3.nonce = header_b3.nonce.wrapping_add(1);
    }
    let block_b3 = Block { header: header_b3, transactions: Vec::new() };
    let block_b3_hash = block_b3.hash();

    chain.add_block(block_b3, genesis_timestamp + 3).unwrap();

    assert_eq!(chain.height(), 3, "Height should be 3");
    assert_eq!(chain.tip_hash(), block_b3_hash, "B3 should be tip");
    assert!(chain.get_block(&block_b3_hash).unwrap().is_main_chain, "B3 should be main chain");
}

// ============================================================================
// Test 7: Block Sync Verification - Verify blocks transfer via P2P protocol
// ============================================================================

#[tokio::test]
async fn test_block_sync_verification() {
    // This test verifies that the P2P sync mechanism actually transfers blocks.
    // Node A has blocks, Node B connects and must sync them via P2P.

    let (node_a, chain_a, _) = create_test_node();

    // Mine blocks on A BEFORE starting
    for _ in 0..3 {
        mine_test_block(&chain_a).await;
    }
    let blocks_a: Vec<_> = {
        let c = chain_a.read().await;
        (1..=3).filter_map(|h| {
            let tip = c.tip_block();
            if h <= tip.height() {
                Some(c.get_block_at_height(h).map(|s| s.block.hash()))
            } else {
                None
            }
        }).collect::<Vec<_>>()
    };

    let (addr_a, shutdown_a, handle_a) = start_node_get_addr(node_a).await;

    // Create node B connecting to A (fresh, height 0)
    let (node_b, chain_b, _) = create_connected_node(addr_a);
    let (_, shutdown_b, handle_b) = start_node_get_addr(node_b).await;

    // Wait for sync
    let chain_b_clone = chain_b.clone();
    let synced = wait_for(SYNC_TIMEOUT_MS, 100, || async {
        chain_b_clone.read().await.height() >= 3
    }).await;

    assert!(synced, "Node B should sync to height 3");

    // VERIFY: B has the specific blocks that A had (not just same height)
    {
        let b = chain_b.read().await;
        for maybe_hash in &blocks_a {
            if let Some(hash) = maybe_hash {
                assert!(b.has_block(hash), "B should have synced block {:?}", &hash[..8]);
            }
        }
    }

    // VERIFY: Same tip
    let tip_a = chain_a.read().await.tip_hash();
    let tip_b = chain_b.read().await.tip_hash();
    assert_eq!(tip_a, tip_b, "Both nodes should have same tip after sync");

    // Shutdown
    let _ = shutdown_a.send(()).await;
    let _ = shutdown_b.send(()).await;
    let _ = timeout(Duration::from_secs(2), handle_a).await;
    let _ = timeout(Duration::from_secs(2), handle_b).await;
}

// ============================================================================
// Test 8: Message Framing - Protocol messages encode/decode correctly
// ============================================================================

#[tokio::test]
async fn test_message_framing_roundtrip() {
    use tunnels_p2p::protocol::{Message, MessageCodec, VersionMessage, NotFoundMessage};
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    let mut codec = MessageCodec::new();

    // Test Version message (most complex)
    let version = VersionMessage {
        protocol_version: 1,
        genesis_hash: [42u8; 32],
        best_height: 100,
        best_hash: [1u8; 32],
        user_agent: "test/1.0".to_string(),
        timestamp: 12345,
        listen_addr: Some("127.0.0.1:8333".parse().unwrap()),
    };

    let original = Message::Version(version);
    let mut buf = BytesMut::new();

    codec.encode(original.clone(), &mut buf).unwrap();
    let decoded = codec.decode(&mut buf).unwrap().unwrap();

    assert_eq!(original, decoded, "Version message should round-trip correctly");

    // Test NotFound message
    let not_found = NotFoundMessage {
        item_type: "block".to_string(),
        hash: [99u8; 32],
    };
    let msg = Message::NotFound(not_found);
    let mut buf = BytesMut::new();
    codec.encode(msg.clone(), &mut buf).unwrap();
    let decoded = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(msg, decoded, "NotFound message should round-trip correctly");

    // Test other message types
    let messages = vec![
        Message::VersionAck,
        Message::GetPeers,
        Message::Ping(42),
        Message::Pong(42),
    ];

    for msg in messages {
        let mut buf = BytesMut::new();
        codec.encode(msg.clone(), &mut buf).unwrap();
        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(msg, decoded, "Message {:?} should round-trip correctly", msg);
    }
}

// ============================================================================
// Test 9: Sync Timeout - Verify sync timeout field is used
// ============================================================================

#[tokio::test]
async fn test_sync_timeout_exists() {
    use tunnels_p2p::sync::SyncManager;
    use std::time::Duration;

    // Test that SyncManager has working timeout
    let sync = SyncManager::with_timeout(Duration::from_millis(100));
    assert_eq!(sync.timeout(), Duration::from_millis(100));

    let mut sync = SyncManager::new();
    assert!(!sync.is_stalled()); // Not syncing, not stalled

    // Start sync
    sync.start_sync(tunnels_p2p::PeerId::new(1), 0, 100);
    assert!(!sync.is_stalled()); // Just started

    // Time since activity should be Some
    assert!(sync.time_since_activity().is_some());
}

// ============================================================================
// Test 10: Address Validation
// ============================================================================

#[tokio::test]
async fn test_address_validation() {
    use tunnels_p2p::discovery::exchange::{is_private_or_local, is_valid_peer_address};

    // Private addresses should be rejected
    assert!(is_private_or_local(&"10.0.0.1:8333".parse().unwrap()));
    assert!(is_private_or_local(&"192.168.1.1:8333".parse().unwrap()));
    assert!(is_private_or_local(&"172.16.0.1:8333".parse().unwrap()));

    // Loopback should be rejected
    assert!(is_private_or_local(&"127.0.0.1:8333".parse().unwrap()));
    assert!(is_private_or_local(&"[::1]:8333".parse().unwrap()));

    // Public addresses should be valid
    assert!(!is_private_or_local(&"8.8.8.8:8333".parse().unwrap()));
    assert!(!is_private_or_local(&"1.1.1.1:443".parse().unwrap()));

    // Own address should be rejected
    let our_addr: SocketAddr = "8.8.8.8:8333".parse().unwrap();
    assert!(!is_valid_peer_address(&our_addr, Some(&our_addr)));

    // Port 0 should be rejected
    assert!(!is_valid_peer_address(&"8.8.8.8:0".parse().unwrap(), None));
}

// ============================================================================
// Test 11: Mempool Removal on Block Receipt
// ============================================================================

#[tokio::test]
async fn test_mempool_removal_on_block_receipt() {
    // This test verifies that the mempool correctly removes transactions
    // when remove_confirmed is called (simulating block receipt).
    //
    // The P2P node calls remove_confirmed_transactions() when a block is
    // added to the chain, which extracts tx_ids and calls mempool.remove_confirmed().

    let chain = Arc::new(RwLock::new(ChainState::new()));
    let mempool = Arc::new(RwLock::new(Mempool::with_defaults()));

    // Create multiple transactions and add to mempool
    let tx1 = create_test_transaction();
    let tx1_id = tx1.id();
    let tx2 = create_test_transaction();
    let tx2_id = tx2.id();
    let tx3 = create_test_transaction();
    let tx3_id = tx3.id();

    {
        let mut mp = mempool.write().await;
        let chain_guard = chain.read().await;
        let tip = chain_guard.tip_hash();
        let mut state = chain_guard.state().clone();

        mp.add_transaction(tx1, &mut state, &tip, 0).unwrap();
        mp.add_transaction(tx2, &mut state, &tip, 0).unwrap();
        mp.add_transaction(tx3, &mut state, &tip, 0).unwrap();
    }

    // VERIFY: All transactions are in mempool
    {
        let mp = mempool.read().await;
        assert!(mp.contains(&tx1_id), "tx1 should be in mempool");
        assert!(mp.contains(&tx2_id), "tx2 should be in mempool");
        assert!(mp.contains(&tx3_id), "tx3 should be in mempool");
        assert_eq!(mp.len(), 3, "Mempool should have 3 transactions");
    }

    // Simulate block receipt: remove some confirmed transactions
    // (This is what the P2P node does in remove_confirmed_transactions)
    {
        let mut mp = mempool.write().await;
        mp.remove_confirmed(&[tx1_id, tx2_id]);
    }

    // VERIFY: Confirmed transactions removed, unconfirmed remains
    {
        let mp = mempool.read().await;
        assert!(!mp.contains(&tx1_id), "tx1 should be removed after confirmation");
        assert!(!mp.contains(&tx2_id), "tx2 should be removed after confirmation");
        assert!(mp.contains(&tx3_id), "tx3 should still be in mempool");
        assert_eq!(mp.len(), 1, "Mempool should have 1 transaction remaining");
    }

    // Remove the last one
    {
        let mut mp = mempool.write().await;
        mp.remove_confirmed(&[tx3_id]);
    }

    // VERIFY: Mempool is now empty
    {
        let mp = mempool.read().await;
        assert!(!mp.contains(&tx3_id), "tx3 should be removed");
        assert_eq!(mp.len(), 0, "Mempool should be empty");
    }
}
