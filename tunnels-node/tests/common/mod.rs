//! Shared test helpers for tunnels-node integration tests.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use tunnels_chain::{ChainState, Mempool, BlockBuilder};
use tunnels_core::crypto::{sign, sha256, KeyPair, PublicKey};
use tunnels_core::serialization::serialize;
use tunnels_core::transaction::{mine_pow, SignedTransaction, Transaction};
use tunnels_core::types::{Denomination, RecipientType};
use tunnels_core::u256::U256;

use tunnels_node::devnet::DevnetConfig;
use tunnels_node::rpc::{MineRequest, RpcState, SharedP2pState, start_rpc_server, RpcServerHandle};

/// Default PoW difficulty for tests.
pub const TEST_TX_DIFFICULTY: u64 = 8;

/// USD denomination.
pub const USD: Denomination = *b"USD\0\0\0\0\0";

/// EUR denomination.
pub const EUR: Denomination = *b"EUR\0\0\0\0\0";

/// Devnet minimum verification window (10 seconds).
pub const DEVNET_VERIFICATION_WINDOW: u64 = 10;

/// Make a JSON-RPC request.
pub async fn rpc_call(client: &Client, url: &str, method: &str, params: Value) -> Value {
    let body = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });

    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("RPC request failed");

    response.json().await.expect("Failed to parse JSON response")
}

/// Create and sign a transaction with PoW.
pub fn create_signed_tx(tx: Transaction, keypair: &KeyPair, difficulty: u64) -> SignedTransaction {
    let tx_bytes = serialize(&tx).unwrap();
    let signature = sign(keypair.signing_key(), &tx_bytes);
    let (pow_nonce, pow_hash) = mine_pow(&tx, &keypair.public_key(), &signature, difficulty);

    SignedTransaction {
        tx,
        signer: keypair.public_key(),
        signature,
        pow_nonce,
        pow_hash,
    }
}

/// Derive address from public key (first 20 bytes of SHA-256).
pub fn derive_address(pubkey: &PublicKey) -> [u8; 20] {
    let hash = sha256(pubkey.as_bytes());
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

/// Project parameters for test helpers.
#[derive(Clone)]
pub struct ProjectParams {
    pub rho: u32,
    pub phi: u32,
    pub gamma: u32,
    pub delta: u32,
    pub phi_recipient: Option<[u8; 20]>,
    pub gamma_recipient: Option<[u8; 20]>,
    pub gamma_recipient_type: RecipientType,
    pub predecessor: Option<[u8; 20]>,
    pub verification_window: u64,
    pub attestation_bond: u64,
    pub challenge_bond: u64,
}

impl Default for ProjectParams {
    fn default() -> Self {
        Self {
            rho: 1000,  // 10%
            phi: 500,   // 5%
            gamma: 1500, // 15%
            delta: 0,
            phi_recipient: None,
            gamma_recipient: None,
            gamma_recipient_type: RecipientType::Identity,
            predecessor: None,
            verification_window: DEVNET_VERIFICATION_WINDOW,
            attestation_bond: 100,
            challenge_bond: 0,
        }
    }
}

/// Test context for protocol tests.
pub struct TestContext {
    pub client: Client,
    pub url: String,
    pub chain: Arc<RwLock<ChainState>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub time_offset: Arc<AtomicU64>,
    pub rpc_handle: RpcServerHandle,
    #[allow(dead_code)]
    mine_rx: Option<tokio::sync::mpsc::Receiver<MineRequest>>,
}

impl TestContext {
    /// Create a new test context with devnet settings.
    pub async fn new_devnet() -> Self {
        let chain = Arc::new(RwLock::new(ChainState::devnet()));
        let mempool = Arc::new(RwLock::new(Mempool::devnet(5000, TEST_TX_DIFFICULTY)));

        let (mine_tx, mine_rx) = tokio::sync::mpsc::channel(32);

        let rpc_state = Arc::new(
            RpcState::new(
                chain.clone(),
                mempool.clone(),
                Some(DevnetConfig::default()),
            )
            .with_mine_tx(mine_tx)
            .with_p2p_state(Arc::new(RwLock::new(SharedP2pState::new()))),
        );

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        let rpc_handle = start_rpc_server(addr, rpc_state, shutdown_tx)
            .await
            .expect("Failed to start RPC server");

        let port = rpc_handle.local_addr().port();
        let url = format!("http://127.0.0.1:{}", port);

        Self {
            client: Client::new(),
            url,
            chain,
            mempool,
            time_offset: Arc::new(AtomicU64::new(1)),
            rpc_handle,
            mine_rx: Some(mine_rx),
        }
    }

    /// Get current simulated time.
    pub fn current_time(&self) -> u64 {
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        base + self.time_offset.load(Ordering::SeqCst)
    }

    /// Advance time by given seconds.
    pub fn advance_time(&self, seconds: u64) {
        self.time_offset.fetch_add(seconds, Ordering::SeqCst);
    }

    /// Mine a block with current mempool transactions.
    pub async fn mine_block(&self) -> [u8; 32] {
        let timestamp = self.current_time();
        self.advance_time(1);
        self.mine_block_at_time(timestamp).await
    }

    /// Mine a block at a specific timestamp.
    pub async fn mine_block_at_time(&self, timestamp: u64) -> [u8; 32] {
        let block = {
            let chain_read = self.chain.read().await;
            let mempool_read = self.mempool.read().await;
            tunnels_chain::produce_block(&chain_read, &mempool_read, timestamp).unwrap()
        };

        let block_hash = block.header.hash();
        let tx_ids: Vec<[u8; 32]> = block.transactions.iter().map(|tx| tx.id()).collect();

        {
            let mut chain_write = self.chain.write().await;
            chain_write.add_block(block, timestamp).unwrap();
        }

        {
            let mut mempool_write = self.mempool.write().await;
            mempool_write.remove_confirmed(&tx_ids);
        }

        block_hash
    }

    /// Mine a block with a specific transaction (bypassing mempool).
    pub async fn mine_block_with_tx(&self, signed_tx: SignedTransaction, timestamp: u64) -> [u8; 32] {
        let block = {
            let chain_read = self.chain.read().await;
            let mut builder = BlockBuilder::new(&chain_read, timestamp);
            builder.add_transaction(signed_tx).expect("Should add transaction");
            builder.build()
        };

        let block_hash = block.header.hash();

        {
            let mut chain_write = self.chain.write().await;
            chain_write.add_block(block, timestamp).unwrap();
        }

        block_hash
    }

    /// Submit a transaction via RPC.
    pub async fn submit_tx(&self, signed_tx: &SignedTransaction) -> Result<String, String> {
        let tx_hex = hex::encode(serialize(signed_tx).unwrap());
        let result = rpc_call(&self.client, &self.url, "submitTransaction", json!([tx_hex])).await;

        if let Some(error) = result.get("error") {
            Err(format!("{:?}", error))
        } else if let Some(res) = result.get("result") {
            Ok(res.as_str().unwrap_or("").to_string())
        } else {
            Err("No result".to_string())
        }
    }

    /// Submit a transaction directly to mempool (bypasses RPC).
    pub async fn submit_tx_direct(&self, signed_tx: SignedTransaction) -> Result<[u8; 32], String> {
        let mut mempool = self.mempool.write().await;
        let chain = self.chain.read().await;
        let tip = chain.tip_hash();
        let mut state = chain.state().clone();
        let current_time = self.current_time();

        mempool
            .add_transaction(signed_tx, &mut state, &tip, current_time)
            .map_err(|e| format!("{:?}", e))
    }

    /// Mine a block containing multiple transactions directly (bypasses mempool).
    /// This is much faster for bulk operations.
    /// Note: timestamp should be >= current_time() to avoid TimestampTooEarly errors.
    pub async fn mine_block_with_txs(&self, transactions: Vec<SignedTransaction>, timestamp: u64) -> [u8; 32] {
        let block = {
            let chain_read = self.chain.read().await;
            let mut builder = BlockBuilder::new(&chain_read, timestamp);
            let mut added = 0;
            let mut failed = 0;
            for tx in transactions {
                match builder.add_transaction(tx) {
                    Ok(_) => added += 1,
                    Err(e) => {
                        failed += 1;
                        eprintln!("Warning: Failed to add tx to block: {:?}", e);
                    }
                }
            }
            if failed > 0 {
                eprintln!("Block: {} added, {} failed", added, failed);
            }
            builder.build()
        };

        let block_hash = block.header.hash();

        {
            let mut chain_write = self.chain.write().await;
            chain_write.add_block(block, timestamp).unwrap();
        }

        // Update time offset to keep current_time() valid for next block
        // Use timestamp + 1 to ensure next block can have a later timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if timestamp >= now {
            self.time_offset.store(timestamp - now + 1, Ordering::SeqCst);
        }

        block_hash
    }

    /// Batch create identities - returns addresses.
    /// Creates identities in batches of `batch_size` per block.
    pub async fn batch_create_identities(&self, keypairs: &[KeyPair], batch_size: usize) -> Vec<[u8; 20]> {
        let mut addresses = Vec::with_capacity(keypairs.len());
        let mut timestamp = self.current_time();

        for chunk in keypairs.chunks(batch_size) {
            let transactions: Vec<SignedTransaction> = chunk
                .iter()
                .map(|kp| {
                    let tx = Transaction::CreateIdentity {
                        public_key: kp.public_key(),
                    };
                    create_signed_tx(tx, kp, TEST_TX_DIFFICULTY)
                })
                .collect();

            self.mine_block_with_txs(transactions, timestamp).await;
            timestamp += 1;

            for kp in chunk {
                addresses.push(derive_address(&kp.public_key()));
            }
        }

        addresses
    }

    /// Batch create projects - returns project IDs.
    pub async fn batch_create_projects(&self, owners: &[(KeyPair, [u8; 20])], batch_size: usize) -> Vec<[u8; 20]> {
        let mut project_ids = Vec::with_capacity(owners.len());
        let mut timestamp = self.current_time();

        for chunk in owners.chunks(batch_size) {
            let mut transactions = Vec::with_capacity(chunk.len());
            let mut pending_ids = Vec::with_capacity(chunk.len());

            for (kp, addr) in chunk {
                let tx = Transaction::RegisterProject {
                    creator: *addr,
                    rho: 1000,
                    phi: 500,
                    gamma: 1500,
                    delta: 0,
                    phi_recipient: *addr,
                    gamma_recipient: *addr,
                    gamma_recipient_type: RecipientType::Identity,
                    reserve_authority: *addr,
                    deposit_authority: *addr,
                    adjudicator: *addr,
                    predecessor: None,
                    verification_window: DEVNET_VERIFICATION_WINDOW,
                    attestation_bond: 100,
                    challenge_bond: 0,
                    evidence_hash: None,
                };
                let signed_tx = create_signed_tx(tx, kp, TEST_TX_DIFFICULTY);

                // Compute project ID from transaction hash
                let tx_hash = sha256(&serialize(&signed_tx).unwrap());
                let mut project_id = [0u8; 20];
                project_id.copy_from_slice(&tx_hash[..20]);
                pending_ids.push(project_id);

                transactions.push(signed_tx);
            }

            self.mine_block_with_txs(transactions, timestamp).await;
            timestamp += 1;
            project_ids.extend(pending_ids);
        }

        project_ids
    }

    /// Batch submit attestations - returns attestation IDs.
    pub async fn batch_submit_attestations(
        &self,
        attestations: &[(KeyPair, [u8; 20], [u8; 20], u64)], // (contributor_kp, contributor_addr, project_id, units)
        batch_size: usize,
    ) -> Vec<[u8; 20]> {
        let mut attestation_ids = Vec::with_capacity(attestations.len());
        let mut timestamp = self.current_time();

        for chunk in attestations.chunks(batch_size) {
            let mut transactions = Vec::with_capacity(chunk.len());
            let mut pending_ids = Vec::with_capacity(chunk.len());

            for (kp, addr, project_id, units) in chunk {
                let tx = Transaction::SubmitAttestation {
                    project_id: *project_id,
                    units: *units,
                    evidence_hash: [0xAB; 32],
                    bond_poster: *addr,
                    bond_amount: U256::from(100u64),
                    bond_denomination: USD,
                };
                let signed_tx = create_signed_tx(tx, kp, TEST_TX_DIFFICULTY);

                let tx_hash = sha256(&serialize(&signed_tx).unwrap());
                let mut att_id = [0u8; 20];
                att_id.copy_from_slice(&tx_hash[..20]);
                pending_ids.push(att_id);

                transactions.push(signed_tx);
            }

            self.mine_block_with_txs(transactions, timestamp).await;
            timestamp += 1;
            attestation_ids.extend(pending_ids);
        }

        attestation_ids
    }

    /// Batch finalize attestations.
    pub async fn batch_finalize_attestations(
        &self,
        attestation_ids: &[[u8; 20]],
        signer_kp: &KeyPair,
        batch_size: usize,
        timestamp: u64,
    ) {
        let mut ts = timestamp;

        for chunk in attestation_ids.chunks(batch_size) {
            let transactions: Vec<SignedTransaction> = chunk
                .iter()
                .map(|att_id| {
                    let tx = Transaction::FinalizeAttestation {
                        attestation_id: *att_id,
                    };
                    create_signed_tx(tx, signer_kp, TEST_TX_DIFFICULTY)
                })
                .collect();

            self.mine_block_with_txs(transactions, ts).await;
            ts += 1;
        }
    }

    /// Create an identity and return its address.
    pub async fn create_identity(&self, kp: &KeyPair) -> [u8; 20] {
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        let signed_tx = create_signed_tx(tx, kp, TEST_TX_DIFFICULTY);
        self.submit_tx(&signed_tx).await.expect("Failed to submit identity tx");
        self.mine_block().await;
        derive_address(&kp.public_key())
    }

    /// Register a project and return its ID.
    pub async fn create_project(&self, owner_kp: &KeyPair, params: ProjectParams) -> [u8; 20] {
        let owner_addr = derive_address(&owner_kp.public_key());

        let tx = Transaction::RegisterProject {
            creator: owner_addr,
            rho: params.rho,
            phi: params.phi,
            gamma: params.gamma,
            delta: params.delta,
            phi_recipient: params.phi_recipient.unwrap_or(owner_addr),
            gamma_recipient: params.gamma_recipient.unwrap_or(owner_addr),
            gamma_recipient_type: params.gamma_recipient_type,
            reserve_authority: owner_addr,
            deposit_authority: owner_addr,
            adjudicator: owner_addr,
            predecessor: params.predecessor,
            verification_window: params.verification_window,
            attestation_bond: params.attestation_bond,
            challenge_bond: params.challenge_bond,
            evidence_hash: None,
        };

        let signed_tx = create_signed_tx(tx.clone(), owner_kp, TEST_TX_DIFFICULTY);
        self.submit_tx(&signed_tx).await.expect("Failed to submit project tx");
        self.mine_block().await;

        // Derive project ID from transaction hash
        let tx_hash = sha256(&serialize(&signed_tx).unwrap());
        let mut project_id = [0u8; 20];
        project_id.copy_from_slice(&tx_hash[..20]);
        project_id
    }

    /// Submit an attestation and return its ID.
    pub async fn submit_attestation(
        &self,
        contributor_kp: &KeyPair,
        project_id: [u8; 20],
        units: u64,
        bond_amount: U256,
    ) -> [u8; 20] {
        self.submit_attestation_with_denom(contributor_kp, project_id, units, bond_amount, USD).await
    }

    /// Submit an attestation with a specific bond denomination and return its ID.
    pub async fn submit_attestation_with_denom(
        &self,
        contributor_kp: &KeyPair,
        project_id: [u8; 20],
        units: u64,
        bond_amount: U256,
        bond_denomination: Denomination,
    ) -> [u8; 20] {
        let contributor_addr = derive_address(&contributor_kp.public_key());

        let tx = Transaction::SubmitAttestation {
            project_id,
            units,
            evidence_hash: [0xAB; 32],
            bond_poster: contributor_addr,
            bond_amount,
            bond_denomination,
        };

        let signed_tx = create_signed_tx(tx, contributor_kp, TEST_TX_DIFFICULTY);
        self.submit_tx(&signed_tx).await.expect("Failed to submit attestation tx");
        self.mine_block().await;

        // Derive attestation ID from transaction hash
        let tx_hash = sha256(&serialize(&signed_tx).unwrap());
        let mut attestation_id = [0u8; 20];
        attestation_id.copy_from_slice(&tx_hash[..20]);
        attestation_id
    }

    /// Finalize an attestation (constructs block directly to bypass time checks).
    pub async fn finalize_attestation(&self, attestation_id: [u8; 20], signer_kp: &KeyPair, timestamp: u64) {
        let tx = Transaction::FinalizeAttestation { attestation_id };
        let signed_tx = create_signed_tx(tx, signer_kp, TEST_TX_DIFFICULTY);
        self.mine_block_with_tx(signed_tx, timestamp).await;
    }

    /// Deposit revenue to a project.
    pub async fn deposit_revenue(
        &self,
        depositor_kp: &KeyPair,
        project_id: [u8; 20],
        amount: U256,
        denom: Denomination,
    ) {
        let tx = Transaction::DepositRevenue {
            project_id,
            amount,
            denomination: denom,
        };
        let signed_tx = create_signed_tx(tx, depositor_kp, TEST_TX_DIFFICULTY);
        self.submit_tx(&signed_tx).await.expect("Failed to submit deposit tx");
        self.mine_block().await;
    }

    /// Claim revenue from a project.
    pub async fn claim_revenue(
        &self,
        contributor_kp: &KeyPair,
        project_id: [u8; 20],
        denom: Denomination,
    ) {
        let tx = Transaction::ClaimRevenue {
            project_id,
            denomination: denom,
        };
        let signed_tx = create_signed_tx(tx, contributor_kp, TEST_TX_DIFFICULTY);
        self.submit_tx(&signed_tx).await.expect("Failed to submit claim tx");
        self.mine_block().await;
    }

    /// Challenge an attestation.
    pub async fn challenge_attestation(
        &self,
        challenger_kp: &KeyPair,
        attestation_id: [u8; 20],
        bond_amount: U256,
    ) -> Result<[u8; 20], String> {
        let challenger_addr = derive_address(&challenger_kp.public_key());

        let tx = Transaction::ChallengeAttestation {
            attestation_id,
            evidence_hash: [0xCD; 32],
            bond_poster: if bond_amount > U256::zero() { Some(challenger_addr) } else { None },
            bond_amount,
        };

        let signed_tx = create_signed_tx(tx, challenger_kp, TEST_TX_DIFFICULTY);
        self.submit_tx(&signed_tx).await?;
        self.mine_block().await;

        // Derive challenge ID
        let tx_hash = sha256(&serialize(&signed_tx).unwrap());
        let mut challenge_id = [0u8; 20];
        challenge_id.copy_from_slice(&tx_hash[..20]);
        Ok(challenge_id)
    }

    /// Get claimable balance for an identity in a project.
    pub async fn get_balance(&self, pubkey_hex: &str, project_id_hex: &str) -> U256 {
        let result = rpc_call(&self.client, &self.url, "get_balance", json!([pubkey_hex, project_id_hex])).await;
        if let Some(balance_info) = result.get("result") {
            let claimable_hex = balance_info["claimable"].as_str().unwrap_or("0x0");
            let hex_str = claimable_hex.trim_start_matches("0x");
            U256::from_str_radix(hex_str, 16).unwrap_or(U256::zero())
        } else {
            U256::zero()
        }
    }

    /// Get attestation info via RPC.
    pub async fn get_attestation(&self, attestation_id: [u8; 20]) -> Option<Value> {
        let result = rpc_call(
            &self.client,
            &self.url,
            "get_attestation",
            json!([hex::encode(attestation_id)]),
        )
        .await;
        result.get("result").cloned()
    }

    /// Get project info via RPC.
    pub async fn get_project(&self, project_id: [u8; 20]) -> Option<Value> {
        let result = rpc_call(
            &self.client,
            &self.url,
            "get_project",
            json!([hex::encode(project_id)]),
        )
        .await;
        result.get("result").cloned()
    }

    /// Get identity info via RPC.
    pub async fn get_identity(&self, pubkey: &PublicKey) -> Option<Value> {
        let result = rpc_call(
            &self.client,
            &self.url,
            "get_identity",
            json!([hex::encode(pubkey.as_bytes())]),
        )
        .await;
        result.get("result").cloned()
    }

    /// Get chain height.
    pub async fn get_height(&self) -> u64 {
        let result = rpc_call(&self.client, &self.url, "getBlockCount", json!([])).await;
        result["result"].as_u64().unwrap_or(0)
    }

    /// Get state root of current tip.
    pub async fn get_state_root(&self) -> [u8; 32] {
        let chain = self.chain.read().await;
        chain.tip_block().block.header.state_root
    }

    /// Get accumulator state directly from chain.
    pub async fn get_accumulator_state(&self, project_id: [u8; 20], denom: Denomination) -> Option<tunnels_core::types::AccumulatorState> {
        let chain = self.chain.read().await;
        chain.state().accumulators.get(&(project_id, denom)).cloned()
    }

    /// Get contributor balance directly from chain.
    pub async fn get_contributor_balance(&self, project_id: [u8; 20], contributor: [u8; 20], denom: Denomination) -> Option<tunnels_core::types::ContributorBalance> {
        let chain = self.chain.read().await;
        chain.state().balances.get(&(project_id, contributor, denom)).cloned()
    }

    /// Get direct claimable balance from chain state.
    pub async fn get_claimable(&self, identity: [u8; 20], denom: Denomination) -> U256 {
        let chain = self.chain.read().await;
        chain.state().claimable.get(&(identity, denom)).cloned().unwrap_or(U256::zero())
    }

    /// Cleanup: stop RPC server.
    pub fn stop(self) {
        let _ = self.rpc_handle.stop();
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = self.rpc_handle.stop();
    }
}
