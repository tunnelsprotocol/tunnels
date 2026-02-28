//! End-to-end smoke test for the Tunnels protocol.
//!
//! This binary starts a real tunnels-node in devnet mode and runs the full
//! protocol lifecycle against it via HTTP JSON-RPC. Nothing is stubbed.
//!
//! Usage: cargo run -p tunnels-smoke

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use colored::Colorize;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use tempfile::TempDir;

use tunnels_core::crypto::{sign, sha256, KeyPair};
use tunnels_core::serialization::serialize;
use tunnels_core::transaction::{mine_pow, SignedTransaction, Transaction};
use tunnels_core::types::{Denomination, RecipientType};
use tunnels_core::u256::U256;

/// Default RPC port for devnet.
const RPC_PORT: u16 = 19334; // Use non-standard port to avoid conflicts

/// Transaction PoW difficulty for devnet (mempool requires 8 bits).
const TX_DIFFICULTY: u64 = 8;

/// USD denomination.
const USD: Denomination = *b"USD\0\0\0\0\0";

/// Devnet verification window (10 seconds).
const VERIFICATION_WINDOW: u64 = 10;

/// Bond amount for attestations.
const BOND_AMOUNT: u64 = 100;

fn main() {
    println!("\n{}", "=".repeat(70).bright_blue());
    println!("{}", "  TUNNELS PROTOCOL END-TO-END SMOKE TEST".bright_blue().bold());
    println!("{}", "=".repeat(70).bright_blue());
    println!();

    let result = run_smoke_test();

    println!();
    match result {
        Ok(()) => {
            println!("{}", "=".repeat(70).bright_green());
            println!("{}", "  ALL TESTS PASSED".bright_green().bold());
            println!("{}", "=".repeat(70).bright_green());
            println!();
        }
        Err(e) => {
            println!("{}", "=".repeat(70).bright_red());
            println!("{}", format!("  TEST FAILED: {}", e).bright_red().bold());
            println!("{}", "=".repeat(70).bright_red());
            println!();
            std::process::exit(1);
        }
    }
}

fn run_smoke_test() -> Result<(), String> {
    // Create temp directory for node data
    let temp_dir = TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let data_dir = temp_dir.path().to_path_buf();

    step("Creating temporary data directory");
    println!("  Data dir: {}", data_dir.display());

    // Build the node binary first
    step("Building tunnels-node binary");
    let build_status = Command::new(cargo_path())
        .args(["build", "-p", "tunnels-node", "--release"])
        .current_dir(workspace_root())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to run cargo build: {}", e))?;

    if !build_status.success() {
        return Err("Failed to build tunnels-node".to_string());
    }
    success("Build completed");

    // Start the node
    step("Starting tunnels-node --devnet");
    let mut node = start_node(&data_dir)?;
    success("Node process started");

    // Wait for RPC to be ready
    step("Waiting for RPC server to be ready");
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let rpc_url = format!("http://127.0.0.1:{}", RPC_PORT);
    wait_for_rpc(&client, &rpc_url)?;
    success("RPC server is ready");

    // Run the actual test
    let test_result = run_lifecycle_test(&client, &rpc_url);

    // Cleanup: kill node process
    step("Stopping node");
    let _ = node.kill();
    let _ = node.wait();
    success("Node stopped");

    // Temp directory is automatically cleaned up when temp_dir goes out of scope
    step("Cleaning up temporary directory");
    drop(temp_dir);
    success("Cleanup complete");

    test_result
}

fn run_lifecycle_test(client: &Client, rpc_url: &str) -> Result<(), String> {
    // Step 1: Generate keypairs
    step("Generating Ed25519 keypairs for Alice and Bob");
    let alice_kp = KeyPair::generate();
    let bob_kp = KeyPair::generate();
    let alice_addr = derive_address(&alice_kp);
    let bob_addr = derive_address(&bob_kp);
    println!("  Alice: 0x{}", hex::encode(alice_addr));
    println!("  Bob:   0x{}", hex::encode(bob_addr));
    success("Keypairs generated");

    // Step 2: Create identity for Alice
    step("Creating identity for Alice");
    let tx = Transaction::CreateIdentity {
        public_key: alice_kp.public_key(),
    };
    let signed_tx = create_signed_tx(&tx, &alice_kp);
    submit_and_mine(client, rpc_url, &signed_tx, "create_identity (Alice)")?;

    // Verify Alice's identity exists
    let alice_identity = get_identity(client, rpc_url, &alice_kp)?;
    if alice_identity.is_none() {
        return Err("Alice's identity not found after creation".to_string());
    }
    success("Alice's identity created and verified");

    // Step 3: Create identity for Bob
    step("Creating identity for Bob");
    let tx = Transaction::CreateIdentity {
        public_key: bob_kp.public_key(),
    };
    let signed_tx = create_signed_tx(&tx, &bob_kp);
    submit_and_mine(client, rpc_url, &signed_tx, "create_identity (Bob)")?;

    // Verify Bob's identity exists
    let bob_identity = get_identity(client, rpc_url, &bob_kp)?;
    if bob_identity.is_none() {
        return Err("Bob's identity not found after creation".to_string());
    }
    success("Bob's identity created and verified");

    // Step 4: Alice registers a project
    step("Alice registering a project");
    let tx = Transaction::RegisterProject {
        creator: alice_addr,
        rho: 1000,              // 10% reserve
        phi: 500,               // 5% application fee
        gamma: 1500,            // 15% genesis allocation
        delta: 0,               // no predecessor fee
        phi_recipient: alice_addr,
        gamma_recipient: alice_addr,
        gamma_recipient_type: RecipientType::Identity,
        reserve_authority: alice_addr,
        deposit_authority: alice_addr,
        adjudicator: alice_addr,
        predecessor: None,
        verification_window: VERIFICATION_WINDOW,
        attestation_bond: BOND_AMOUNT,
        challenge_bond: 0,
        evidence_hash: None,
    };
    let signed_tx = create_signed_tx(&tx, &alice_kp);
    let project_id = derive_id_from_tx(&signed_tx);
    submit_and_mine(client, rpc_url, &signed_tx, "register_project")?;

    // Query Alice's projects to see what was actually created
    let pubkey_hex = hex::encode(alice_kp.public_key().as_bytes());
    let projects_result = rpc_call(client, rpc_url, "get_projects_by_identity", json!([pubkey_hex]))?;
    println!("  Alice's projects: {:?}", projects_result.get("result"));

    // Verify project exists
    let project = get_project(client, rpc_url, project_id)?;
    println!("  Expected project ID: 0x{}", hex::encode(project_id));
    println!("  Project lookup result: {:?}", project);

    if project.is_none() {
        // Check if a project was created but with different ID
        if let Some(projects) = projects_result.get("result").and_then(|r| r.as_array()) {
            if !projects.is_empty() {
                println!("  WARNING: Found {} project(s), but not with expected ID!", projects.len());
            }
        }
        return Err("Project not found after registration".to_string());
    }
    success("Project registered and verified");

    // Step 5: Bob submits an attestation
    step("Bob submitting attestation (100 units)");
    let evidence_hash = sha256(b"Bob's contribution evidence");
    let tx = Transaction::SubmitAttestation {
        project_id,
        units: 100,
        evidence_hash,
        bond_poster: bob_addr,
        bond_amount: U256::from(BOND_AMOUNT),
        bond_denomination: USD,
    };
    let signed_tx = create_signed_tx(&tx, &bob_kp);
    let attestation_id = derive_id_from_tx(&signed_tx);
    submit_and_mine(client, rpc_url, &signed_tx, "submit_attestation")?;

    // Verify attestation exists and is pending
    let attestation = get_attestation(client, rpc_url, attestation_id)?;
    if attestation.is_none() {
        return Err("Attestation not found after submission".to_string());
    }
    let status = attestation.as_ref().unwrap()["status"].as_str().unwrap_or("");
    if status != "pending" {
        return Err(format!("Expected attestation status 'pending', got '{}'", status));
    }
    println!("  Attestation ID: 0x{}", hex::encode(attestation_id));
    success("Attestation submitted and verified (status: pending)");

    // Step 6: Wait for verification window to expire
    step(&format!("Waiting {} seconds for verification window to expire", VERIFICATION_WINDOW + 2));
    std::thread::sleep(Duration::from_secs(VERIFICATION_WINDOW + 2));
    success("Verification window expired");

    // Step 7: Finalize attestation (any signer can do this)
    step("Finalizing attestation");
    let tx = Transaction::FinalizeAttestation { attestation_id };
    let signed_tx = create_signed_tx(&tx, &alice_kp); // Alice finalizes (anyone can)
    submit_and_mine(client, rpc_url, &signed_tx, "finalize_attestation")?;

    // Verify attestation is now finalized
    let attestation = get_attestation(client, rpc_url, attestation_id)?;
    if attestation.is_none() {
        return Err("Attestation not found after finalization".to_string());
    }
    let status = attestation.as_ref().unwrap()["status"].as_str().unwrap_or("");
    if status != "finalized" {
        return Err(format!("Expected attestation status 'finalized', got '{}'", status));
    }
    success("Attestation finalized and verified (status: finalized)");

    // Step 8: Deposit revenue (Alice is deposit authority)
    step("Depositing 10000 USD revenue to project");
    let deposit_amount = U256::from(10000u64);
    let tx = Transaction::DepositRevenue {
        project_id,
        amount: deposit_amount,
        denomination: USD,
    };
    let signed_tx = create_signed_tx(&tx, &alice_kp);
    submit_and_mine(client, rpc_url, &signed_tx, "deposit_revenue")?;
    success("Revenue deposited");

    // Step 9: Check Bob's claimable balance before claim
    // Note: claimable balance includes bond return (100) from finalized attestation
    step("Checking Bob's claimable balance before claim");
    let balance_before_claim = get_balance(client, rpc_url, &bob_kp, project_id)?;
    println!("  Bob's claimable balance before claim: {}", balance_before_claim);

    // The claimable balance should exactly equal the bond return
    let expected_bond_return = U256::from(BOND_AMOUNT);
    if balance_before_claim != expected_bond_return {
        return Err(format!(
            "Bob's claimable balance ({}) != expected bond return ({})",
            balance_before_claim, expected_bond_return
        ));
    }
    success(&format!("Bob has claimable balance: {} (bond return)", balance_before_claim));

    // Step 10: Bob claims revenue from labor pool
    step("Bob claiming revenue from labor pool");
    let tx = Transaction::ClaimRevenue {
        project_id,
        denomination: USD,
    };
    let signed_tx = create_signed_tx(&tx, &bob_kp);
    submit_and_mine(client, rpc_url, &signed_tx, "claim_revenue")?;
    success("Revenue claimed from labor pool");

    // Step 11: Verify Bob's claimable balance increased by exact labor pool amount
    // ClaimRevenue moves funds FROM labor pool TO claimable balance
    step("Verifying Bob's claimable balance after claim");
    let balance_after_claim = get_balance(client, rpc_url, &bob_kp, project_id)?;
    println!("  Bob's claimable balance after claim: {}", balance_after_claim);

    // Calculate exact expected labor pool revenue using cascading waterfall:
    //   D = 10000 (deposit)
    //   predecessor = D * 0 = 0
    //   R' = 10000 - 0 = 10000
    //   reserve = R' * 1000/10000 = 1000
    //   R'' = 10000 - 1000 = 9000
    //   fee = R'' * 500/10000 = 450
    //   R''' = 9000 - 450 = 8550
    //   genesis = R''' * 1500/10000 = 1282 (truncated from 1282.5)
    //   labor = R''' - genesis = 8550 - 1282 = 7268
    //
    // However, the accumulator uses 128.128 fixed-point arithmetic:
    //   reward_delta = floor(7268 * 2^128 / 100) per unit
    //   claim = floor(100 * reward_delta / 2^128) = 7267
    // The 1-unit difference is precision loss from floor(7268/100) = 72.68
    const EXPECTED_LABOR_POOL: u64 = 7267;
    let claimed_from_labor_pool = balance_after_claim - balance_before_claim;
    println!("  Claimed from labor pool: {}", claimed_from_labor_pool);

    if claimed_from_labor_pool != U256::from(EXPECTED_LABOR_POOL) {
        return Err(format!(
            "Labor pool claim ({}) != expected ({})",
            claimed_from_labor_pool, EXPECTED_LABOR_POOL
        ));
    }

    // Verify total claimable = bond return + labor pool
    let expected_total = U256::from(BOND_AMOUNT) + U256::from(EXPECTED_LABOR_POOL);
    if balance_after_claim != expected_total {
        return Err(format!(
            "Total claimable ({}) != expected ({})",
            balance_after_claim, expected_total
        ));
    }
    success(&format!(
        "Bob's claimable balance: {} (bond) + {} (labor) = {}",
        BOND_AMOUNT, EXPECTED_LABOR_POOL, balance_after_claim
    ));

    // Step 12: Final verification - query all state
    step("Final state verification");

    // Verify Alice identity
    let alice_id = get_identity(client, rpc_url, &alice_kp)?;
    if alice_id.is_none() {
        return Err("Alice identity missing in final check".to_string());
    }
    println!("  Alice identity: exists");

    // Verify Bob identity
    let bob_id = get_identity(client, rpc_url, &bob_kp)?;
    if bob_id.is_none() {
        return Err("Bob identity missing in final check".to_string());
    }
    println!("  Bob identity: exists");

    // Verify project
    let proj = get_project(client, rpc_url, project_id)?;
    if proj.is_none() {
        return Err("Project missing in final check".to_string());
    }
    println!("  Project: exists");

    // Verify attestation is finalized
    let att = get_attestation(client, rpc_url, attestation_id)?;
    if att.is_none() {
        return Err("Attestation missing in final check".to_string());
    }
    let final_status = att.as_ref().unwrap()["status"].as_str().unwrap_or("");
    if final_status != "finalized" {
        return Err(format!("Attestation not finalized in final check: {}", final_status));
    }
    println!("  Attestation: finalized");

    // Get chain info
    let chain_info = get_chain_info(client, rpc_url)?;
    println!("  Chain height: {}", chain_info["height"]);
    println!("  Tip hash: {}", chain_info["tip_hash"]);

    success("All state verified successfully");

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn step(msg: &str) {
    println!("\n{} {}", "[STEP]".bright_cyan().bold(), msg);
}

fn success(msg: &str) {
    println!("  {} {}", "OK".bright_green().bold(), msg);
}

fn cargo_path() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".cargo/bin/cargo").to_string_lossy().to_string())
            .unwrap_or_else(|| "cargo".to_string())
    })
}

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn start_node(data_dir: &std::path::Path) -> Result<Child, String> {
    let binary = workspace_root().join("target/release/tunnels-node");

    if !binary.exists() {
        return Err(format!("Binary not found: {}", binary.display()));
    }

    let child = Command::new(&binary)
        .args([
            "--devnet",
            "--mine",  // Enable mining for explicit mine RPC calls
            "--no-auto-mine",  // Disable auto-mine to avoid race conditions
            "--data-dir", &data_dir.to_string_lossy(),
            "--rpc-listen", &format!("127.0.0.1:{}", RPC_PORT),
            "--listen", "127.0.0.1:19333", // Non-standard P2P port
            "--log-level", "warn",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to start node: {}", e))?;

    Ok(child)
}

fn wait_for_rpc(client: &Client, rpc_url: &str) -> Result<(), String> {
    let start = Instant::now();
    let timeout = Duration::from_secs(30);

    loop {
        if start.elapsed() > timeout {
            return Err("Timeout waiting for RPC server".to_string());
        }

        match rpc_call(client, rpc_url, "getBlockCount", json!([])) {
            Ok(result) => {
                if result.get("result").is_some() {
                    return Ok(());
                }
            }
            Err(_) => {
                // Server not ready yet
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn rpc_call(client: &Client, url: &str, method: &str, params: Value) -> Result<Value, String> {
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
        .map_err(|e| format!("RPC request failed: {}", e))?;

    let json: Value = response
        .json()
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    Ok(json)
}

fn create_signed_tx(tx: &Transaction, keypair: &KeyPair) -> SignedTransaction {
    let tx_bytes = serialize(tx).expect("Serialization failed");
    let signature = sign(keypair.signing_key(), &tx_bytes);
    let (pow_nonce, pow_hash) = mine_pow(tx, &keypair.public_key(), &signature, TX_DIFFICULTY);

    SignedTransaction {
        tx: tx.clone(),
        signer: keypair.public_key(),
        signature,
        pow_nonce,
        pow_hash,
    }
}

fn derive_address(keypair: &KeyPair) -> [u8; 20] {
    let hash = sha256(keypair.public_key().as_bytes());
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

fn derive_id_from_tx(signed_tx: &SignedTransaction) -> [u8; 20] {
    let tx_hash = sha256(&serialize(signed_tx).expect("Serialization failed"));
    let mut id = [0u8; 20];
    id.copy_from_slice(&tx_hash[..20]);
    id
}

fn submit_and_mine(client: &Client, rpc_url: &str, signed_tx: &SignedTransaction, desc: &str) -> Result<(), String> {
    // Submit transaction
    let tx_hex = hex::encode(serialize(signed_tx).expect("Serialization failed"));
    let result = rpc_call(client, rpc_url, "submitTransaction", json!([tx_hex]))?;

    if let Some(error) = result.get("error") {
        return Err(format!("Failed to submit {}: {:?}", desc, error));
    }

    let tx_id = result.get("result")
        .and_then(|r| r.as_str())
        .unwrap_or("unknown");
    println!("    Submitted tx: {}", tx_id);

    // Ensure timestamp advances (median-time-past rule requires strictly greater)
    std::thread::sleep(Duration::from_millis(1100));

    // Explicitly mine a block (auto-mine is disabled)
    let mine_result = rpc_call(client, rpc_url, "mine", json!([1]))?;

    if let Some(error) = mine_result.get("error") {
        return Err(format!("Failed to mine block for {}: {:?}", desc, error));
    }

    let block_hashes = mine_result.get("result")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    if block_hashes.is_empty() {
        return Err(format!("Mine returned no blocks for {}", desc));
    }

    println!("    Mined block: {}", block_hashes[0]);

    Ok(())
}

fn get_identity(client: &Client, rpc_url: &str, keypair: &KeyPair) -> Result<Option<Value>, String> {
    let pubkey_hex = hex::encode(keypair.public_key().as_bytes());
    let result = rpc_call(client, rpc_url, "get_identity", json!([pubkey_hex]))?;
    Ok(result.get("result").cloned())
}

fn get_project(client: &Client, rpc_url: &str, project_id: [u8; 20]) -> Result<Option<Value>, String> {
    let project_hex = hex::encode(project_id);
    let result = rpc_call(client, rpc_url, "get_project", json!([project_hex]))?;
    Ok(result.get("result").cloned())
}

fn get_attestation(client: &Client, rpc_url: &str, attestation_id: [u8; 20]) -> Result<Option<Value>, String> {
    let att_hex = hex::encode(attestation_id);
    let result = rpc_call(client, rpc_url, "get_attestation", json!([att_hex]))?;
    Ok(result.get("result").cloned())
}

fn get_balance(client: &Client, rpc_url: &str, keypair: &KeyPair, project_id: [u8; 20]) -> Result<U256, String> {
    let pubkey_hex = hex::encode(keypair.public_key().as_bytes());
    let project_hex = hex::encode(project_id);
    let result = rpc_call(client, rpc_url, "get_balance", json!([pubkey_hex, project_hex]))?;

    if let Some(balance_info) = result.get("result") {
        let claimable_hex = balance_info["claimable"].as_str().unwrap_or("0x0");
        let hex_str = claimable_hex.trim_start_matches("0x");
        Ok(U256::from_str_radix(hex_str, 16).unwrap_or(U256::zero()))
    } else {
        Ok(U256::zero())
    }
}

fn get_chain_info(client: &Client, rpc_url: &str) -> Result<Value, String> {
    let height_result = rpc_call(client, rpc_url, "getBlockCount", json!([]))?;
    let tip_result = rpc_call(client, rpc_url, "getBestBlockHash", json!([]))?;

    Ok(json!({
        "height": height_result.get("result").and_then(|v| v.as_u64()).unwrap_or(0),
        "tip_hash": tip_result.get("result").and_then(|v| v.as_str()).unwrap_or("unknown")
    }))
}
