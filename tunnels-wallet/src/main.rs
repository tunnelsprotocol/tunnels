//! Tunnels Wallet - Command-line key management and transaction submission.
//!
//! # Usage
//!
//! ## Offline Commands (no node needed)
//!
//! ```bash
//! # Generate a new identity
//! tunnels-wallet generate
//!
//! # Import an encrypted backup
//! tunnels-wallet import backup.key
//!
//! # Export encrypted backup
//! tunnels-wallet export backup.key
//!
//! # Show identity information
//! tunnels-wallet show
//! ```
//!
//! ## Online Commands (requires --node)
//!
//! ```bash
//! # Create identity on-chain
//! tunnels-wallet create-identity --node http://localhost:9334
//!
//! # Submit attestation
//! tunnels-wallet submit-attestation --node http://localhost:9334 \
//!     --project <project_id> --units 100 --evidence <hash>
//! ```

use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tunnels_core::{
    crypto::{derive_address, sha256, sign},
    serialization::serialize,
    transaction::{mine_pow, Transaction},
    Denomination, KeyPair, PublicKey, RecipientType, SignedTransaction, U256,
};
use tunnels_sdk::{decrypt_key, encrypt_key, generate_keypair};

/// Default keystore directory under user's home.
const KEYSTORE_DIR: &str = ".tunnels/keystore";
const DEFAULT_KEY_NAME: &str = "default.key";

/// Tunnels protocol wallet.
#[derive(Parser)]
#[command(name = "tunnels-wallet")]
#[command(about = "Command-line wallet for the Tunnels protocol")]
#[command(version)]
struct Cli {
    /// Path to the keystore directory.
    #[arg(long, global = true)]
    keystore: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // === Offline Commands ===
    /// Generate a new keypair and store it encrypted.
    Generate {
        /// Name for the key file (default: default.key).
        #[arg(long)]
        name: Option<String>,
    },

    /// Import an encrypted backup file.
    Import {
        /// Path to the encrypted backup file.
        file: PathBuf,

        /// Name for the imported key (default: derived from filename).
        #[arg(long)]
        name: Option<String>,
    },

    /// Export the key as an encrypted backup.
    Export {
        /// Path for the backup file.
        file: PathBuf,

        /// Name of the key to export (default: default.key).
        #[arg(long)]
        name: Option<String>,
    },

    /// Display the identity address and public key.
    Show {
        /// Name of the key to show (default: default.key).
        #[arg(long)]
        name: Option<String>,
    },

    // === Online Commands ===
    /// Create an identity on-chain.
    CreateIdentity {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the key to use (default: default.key).
        #[arg(long)]
        name: Option<String>,
    },

    /// Create an agent identity.
    CreateAgent {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the parent key to use (default: default.key).
        #[arg(long)]
        name: Option<String>,

        /// Public key hex of the agent.
        #[arg(long, required = true)]
        agent_pubkey: String,
    },

    /// Submit an attestation.
    SubmitAttestation {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the contributor key (default: default.key).
        #[arg(long)]
        name: Option<String>,

        /// Project ID (hex).
        #[arg(long, required = true)]
        project: String,

        /// Number of units.
        #[arg(long, required = true)]
        units: u64,

        /// Evidence hash (hex).
        #[arg(long, required = true)]
        evidence: String,

        /// Bond amount.
        #[arg(long, required = true)]
        bond: u64,

        /// Bond denomination (e.g., USD).
        #[arg(long, default_value = "USD")]
        denom: String,
    },

    /// Register a new project.
    RegisterProject {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the creator key (default: default.key).
        #[arg(long)]
        name: Option<String>,

        /// Reserve rate in basis points (0-10000).
        #[arg(long, default_value = "1000")]
        rho: u32,

        /// Fee rate in basis points.
        #[arg(long, default_value = "500")]
        phi: u32,

        /// Genesis rate in basis points.
        #[arg(long, default_value = "1500")]
        gamma: u32,

        /// Predecessor rate in basis points.
        #[arg(long, default_value = "0")]
        delta: u32,

        /// Verification window in seconds.
        #[arg(long, default_value = "604800")]
        verification_window: u64,

        /// Minimum attestation bond.
        #[arg(long, default_value = "100")]
        attestation_bond: u64,

        /// Challenge bond.
        #[arg(long, default_value = "0")]
        challenge_bond: u64,
    },

    /// Update profile hash.
    UpdateProfile {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the key (default: default.key).
        #[arg(long)]
        name: Option<String>,

        /// New profile hash (hex).
        #[arg(long, required = true)]
        hash: String,
    },

    /// Claim revenue from labor pool.
    ClaimRevenue {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the key (default: default.key).
        #[arg(long)]
        name: Option<String>,

        /// Project ID (hex).
        #[arg(long, required = true)]
        project: String,

        /// Denomination.
        #[arg(long, default_value = "USD")]
        denom: String,
    },

    /// Claim from reserve pool.
    ClaimReserve {
        /// Node RPC URL.
        #[arg(long, required = true)]
        node: String,

        /// Name of the key (default: default.key).
        #[arg(long)]
        name: Option<String>,

        /// Project ID (hex).
        #[arg(long, required = true)]
        project: String,

        /// Amount to claim.
        #[arg(long, required = true)]
        amount: u64,

        /// Denomination.
        #[arg(long, default_value = "USD")]
        denom: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let keystore = get_keystore_dir(&cli)?;

    match cli.command {
        Commands::Generate { name } => cmd_generate(&keystore, name),
        Commands::Import { file, name } => cmd_import(&keystore, file, name),
        Commands::Export { file, name } => cmd_export(&keystore, file, name),
        Commands::Show { name } => cmd_show(&keystore, name),

        Commands::CreateIdentity { node, name } => cmd_create_identity(&keystore, &node, name),
        Commands::CreateAgent {
            node,
            name,
            agent_pubkey,
        } => cmd_create_agent(&keystore, &node, name, &agent_pubkey),
        Commands::SubmitAttestation {
            node,
            name,
            project,
            units,
            evidence,
            bond,
            denom,
        } => cmd_submit_attestation(&keystore, &node, name, &project, units, &evidence, bond, &denom),
        Commands::RegisterProject {
            node,
            name,
            rho,
            phi,
            gamma,
            delta,
            verification_window,
            attestation_bond,
            challenge_bond,
        } => cmd_register_project(
            &keystore,
            &node,
            name,
            rho,
            phi,
            gamma,
            delta,
            verification_window,
            attestation_bond,
            challenge_bond,
        ),
        Commands::UpdateProfile { node, name, hash } => {
            cmd_update_profile(&keystore, &node, name, &hash)
        }
        Commands::ClaimRevenue {
            node,
            name,
            project,
            denom,
        } => cmd_claim_revenue(&keystore, &node, name, &project, &denom),
        Commands::ClaimReserve {
            node,
            name,
            project,
            amount,
            denom,
        } => cmd_claim_reserve(&keystore, &node, name, &project, amount, &denom),
    }
}

fn get_keystore_dir(cli: &Cli) -> Result<PathBuf, String> {
    if let Some(ref path) = cli.keystore {
        return Ok(path.clone());
    }

    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join(KEYSTORE_DIR))
}

fn get_key_path(keystore: &Path, name: Option<String>) -> PathBuf {
    let filename = name.unwrap_or_else(|| DEFAULT_KEY_NAME.to_string());
    keystore.join(filename)
}

fn prompt_password(prompt: &str) -> Result<String, String> {
    rpassword::prompt_password(prompt).map_err(|e| format!("Failed to read password: {}", e))
}

fn prompt_password_confirm() -> Result<String, String> {
    let pass1 = prompt_password("Enter password: ")?;
    let pass2 = prompt_password("Confirm password: ")?;
    if pass1 != pass2 {
        return Err("Passwords do not match".to_string());
    }
    Ok(pass1)
}

fn load_keypair(keystore: &Path, name: Option<String>) -> Result<KeyPair, String> {
    let path = get_key_path(keystore, name);
    if !path.exists() {
        return Err(format!("Key file not found: {}", path.display()));
    }

    let encrypted = fs::read(&path).map_err(|e| format!("Failed to read key file: {}", e))?;
    let password = prompt_password("Enter password: ")?;
    decrypt_key(&encrypted, &password).map_err(|e| format!("Failed to decrypt key: {}", e))
}

fn hex_to_bytes<const N: usize>(hex: &str) -> Result<[u8; N], String> {
    let bytes =
        hex::decode(hex.trim_start_matches("0x")).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.len() != N {
        return Err(format!("Expected {} bytes, got {}", N, bytes.len()));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn parse_denomination(s: &str) -> Result<Denomination, String> {
    let bytes = s.as_bytes();
    if bytes.len() > 8 {
        return Err("Denomination must be 8 bytes or less".to_string());
    }
    let mut denom = [0u8; 8];
    denom[..bytes.len()].copy_from_slice(bytes);
    Ok(denom)
}

// ============================================================================
// Offline Commands
// ============================================================================

fn cmd_generate(keystore: &Path, name: Option<String>) -> Result<(), String> {
    fs::create_dir_all(keystore).map_err(|e| format!("Failed to create keystore: {}", e))?;

    let path = get_key_path(keystore, name);
    if path.exists() {
        return Err(format!("Key already exists: {}", path.display()));
    }

    let kp = generate_keypair();
    let password = prompt_password_confirm()?;
    let encrypted =
        encrypt_key(&kp, &password).map_err(|e| format!("Failed to encrypt key: {}", e))?;

    fs::write(&path, encrypted).map_err(|e| format!("Failed to write key file: {}", e))?;

    let address = derive_address(&kp.public_key());
    println!("Generated new identity:");
    println!("  Address:    {}", hex::encode(address));
    println!("  Public Key: {}", hex::encode(kp.public_key().as_bytes()));
    println!("  Saved to:   {}", path.display());

    Ok(())
}

fn cmd_import(keystore: &Path, file: PathBuf, name: Option<String>) -> Result<(), String> {
    fs::create_dir_all(keystore).map_err(|e| format!("Failed to create keystore: {}", e))?;

    if !file.exists() {
        return Err(format!("File not found: {}", file.display()));
    }

    let encrypted = fs::read(&file).map_err(|e| format!("Failed to read file: {}", e))?;

    // Verify it's a valid key file by decrypting it
    let password = prompt_password("Enter backup password: ")?;
    let kp = decrypt_key(&encrypted, &password)
        .map_err(|e| format!("Failed to decrypt backup: {}", e))?;

    // Determine destination name
    let dest_name = name.unwrap_or_else(|| {
        file.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_KEY_NAME.to_string())
    });

    let dest_path = keystore.join(&dest_name);
    if dest_path.exists() {
        return Err(format!("Key already exists: {}", dest_path.display()));
    }

    // Re-encrypt with potentially new password
    print!("Re-encrypt with a new password? [y/N] ");
    io::stdout().flush().ok();
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .map_err(|e| e.to_string())?;

    let final_encrypted = if response.trim().eq_ignore_ascii_case("y") {
        let new_password = prompt_password_confirm()?;
        encrypt_key(&kp, &new_password).map_err(|e| e.to_string())?
    } else {
        encrypted
    };

    fs::write(&dest_path, final_encrypted)
        .map_err(|e| format!("Failed to write key file: {}", e))?;

    let address = derive_address(&kp.public_key());
    println!("Imported identity:");
    println!("  Address:    {}", hex::encode(address));
    println!("  Public Key: {}", hex::encode(kp.public_key().as_bytes()));
    println!("  Saved to:   {}", dest_path.display());

    Ok(())
}

fn cmd_export(keystore: &Path, file: PathBuf, name: Option<String>) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;

    if file.exists() {
        return Err(format!("File already exists: {}", file.display()));
    }

    println!("Set password for the backup file:");
    let password = prompt_password_confirm()?;
    let encrypted =
        encrypt_key(&kp, &password).map_err(|e| format!("Failed to encrypt: {}", e))?;

    fs::write(&file, encrypted).map_err(|e| format!("Failed to write file: {}", e))?;

    println!("Exported to: {}", file.display());
    Ok(())
}

fn cmd_show(keystore: &Path, name: Option<String>) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());

    println!("Identity:");
    println!("  Address:    {}", hex::encode(address));
    println!("  Public Key: {}", hex::encode(kp.public_key().as_bytes()));

    Ok(())
}

// ============================================================================
// Online Commands
// ============================================================================

fn submit_transaction(node: &str, signed_tx: &SignedTransaction) -> Result<String, String> {
    let tx_bytes = serialize(signed_tx).map_err(|e| format!("Serialization failed: {}", e))?;
    let tx_hex = hex::encode(&tx_bytes);

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "submit_transaction",
        "params": { "signed_tx": tx_hex },
        "id": 1
    });

    let mut response = ureq::post(node)
        .send_json(&body)
        .map_err(|e| format!("RPC request failed: {}", e))?;

    let response_json: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if let Some(error) = response_json.get("error") {
        return Err(format!("RPC error: {}", error));
    }

    let result = response_json
        .get("result")
        .ok_or("Missing result in response")?;
    let tx_hash = result
        .get("tx_hash")
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or("Missing tx_hash in result")?;

    Ok(tx_hash.to_string())
}

fn get_tx_difficulty(node: &str) -> Result<u64, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "get_chain_info",
        "params": {},
        "id": 1
    });

    let mut response = ureq::post(node)
        .send_json(&body)
        .map_err(|e| format!("RPC request failed: {}", e))?;

    let response_json: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if let Some(error) = response_json.get("error") {
        return Err(format!("RPC error: {}", error));
    }

    let result = response_json
        .get("result")
        .ok_or("Missing result in response")?;
    let difficulty = result
        .get("tx_difficulty")
        .and_then(|v: &serde_json::Value| v.as_u64())
        .unwrap_or(8); // Default to 8 for devnet

    Ok(difficulty)
}

fn sign_and_submit(node: &str, kp: &KeyPair, tx: &Transaction) -> Result<String, String> {
    let difficulty = get_tx_difficulty(node)?;

    let tx_bytes = serialize(tx).map_err(|e| format!("Serialization failed: {}", e))?;
    let signature = sign(kp.signing_key(), &tx_bytes);
    let (pow_nonce, pow_hash) = mine_pow(tx, &kp.public_key(), &signature, difficulty);

    let signed_tx = SignedTransaction {
        tx: tx.clone(),
        signer: kp.public_key(),
        signature,
        pow_nonce,
        pow_hash,
    };

    submit_transaction(node, &signed_tx)
}

fn cmd_create_identity(keystore: &Path, node: &str, name: Option<String>) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());

    println!("Creating identity...");
    println!("  Address: {}", hex::encode(address));

    let tx = Transaction::CreateIdentity {
        public_key: kp.public_key(),
    };

    let tx_hash = sign_and_submit(node, &kp, &tx)?;
    println!("Transaction submitted: {}", tx_hash);
    println!("Identity created successfully!");

    Ok(())
}

fn cmd_create_agent(
    keystore: &Path,
    node: &str,
    name: Option<String>,
    agent_pubkey_hex: &str,
) -> Result<(), String> {
    let parent_kp = load_keypair(keystore, name)?;
    let parent_address = derive_address(&parent_kp.public_key());

    let agent_pubkey_bytes: [u8; 32] = hex_to_bytes(agent_pubkey_hex)?;
    let agent_pubkey =
        PublicKey::from_bytes(&agent_pubkey_bytes).map_err(|e| format!("Invalid public key: {}", e))?;

    println!("Creating agent...");
    println!("  Parent: {}", hex::encode(parent_address));
    println!("  Agent:  {}", hex::encode(derive_address(&agent_pubkey)));

    let tx = Transaction::CreateAgent {
        parent: parent_address,
        public_key: agent_pubkey,
    };

    let tx_hash = sign_and_submit(node, &parent_kp, &tx)?;
    println!("Transaction submitted: {}", tx_hash);
    println!("Agent created successfully!");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_submit_attestation(
    keystore: &Path,
    node: &str,
    name: Option<String>,
    project_hex: &str,
    units: u64,
    evidence_hex: &str,
    bond: u64,
    denom_str: &str,
) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());
    let project_id: [u8; 20] = hex_to_bytes(project_hex)?;
    let evidence_hash: [u8; 32] = hex_to_bytes(evidence_hex)?;
    let denom = parse_denomination(denom_str)?;

    println!("Submitting attestation...");
    println!("  Project:  {}", hex::encode(project_id));
    println!("  Units:    {}", units);
    println!("  Bond:     {} {}", bond, denom_str);

    let tx = Transaction::SubmitAttestation {
        project_id,
        units,
        evidence_hash,
        bond_poster: address,
        bond_amount: U256::from(bond),
        bond_denomination: denom,
    };

    let tx_hash = sign_and_submit(node, &kp, &tx)?;
    println!("Transaction submitted: {}", tx_hash);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_register_project(
    keystore: &Path,
    node: &str,
    name: Option<String>,
    rho: u32,
    phi: u32,
    gamma: u32,
    delta: u32,
    verification_window: u64,
    attestation_bond: u64,
    challenge_bond: u64,
) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());

    println!("Registering project...");
    println!("  Creator: {}", hex::encode(address));
    println!("  rho={}, phi={}, gamma={}, delta={}", rho, phi, gamma, delta);

    let tx = Transaction::RegisterProject {
        creator: address,
        rho,
        phi,
        gamma,
        delta,
        phi_recipient: address,
        gamma_recipient: address,
        gamma_recipient_type: RecipientType::Identity,
        reserve_authority: address,
        deposit_authority: address,
        adjudicator: address,
        predecessor: None,
        verification_window,
        attestation_bond,
        challenge_bond,
        evidence_hash: None,
    };

    // Derive project ID from transaction hash (before submission)
    let tx_bytes = serialize(&tx).map_err(|e| e.to_string())?;
    let project_id_full = sha256(&tx_bytes);
    let mut project_id = [0u8; 20];
    project_id.copy_from_slice(&project_id_full[..20]);

    let tx_hash = sign_and_submit(node, &kp, &tx)?;

    println!("Transaction submitted: {}", tx_hash);
    println!("Project ID: {}", hex::encode(project_id));

    Ok(())
}

fn cmd_update_profile(
    keystore: &Path,
    node: &str,
    name: Option<String>,
    hash_hex: &str,
) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());
    let hash: [u8; 32] = hex_to_bytes(hash_hex)?;

    println!("Updating profile...");
    println!("  Identity: {}", hex::encode(address));
    println!("  Hash:     {}", hex::encode(hash));

    let tx = Transaction::UpdateProfileHash {
        identity: address,
        hash,
    };

    let tx_hash = sign_and_submit(node, &kp, &tx)?;
    println!("Transaction submitted: {}", tx_hash);

    Ok(())
}

fn cmd_claim_revenue(
    keystore: &Path,
    node: &str,
    name: Option<String>,
    project_hex: &str,
    denom_str: &str,
) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());
    let project_id: [u8; 20] = hex_to_bytes(project_hex)?;
    let denom = parse_denomination(denom_str)?;

    println!("Claiming revenue...");
    println!("  Identity: {}", hex::encode(address));
    println!("  Project:  {}", hex::encode(project_id));

    let tx = Transaction::ClaimRevenue {
        project_id,
        denomination: denom,
    };

    let tx_hash = sign_and_submit(node, &kp, &tx)?;
    println!("Transaction submitted: {}", tx_hash);

    Ok(())
}

fn cmd_claim_reserve(
    keystore: &Path,
    node: &str,
    name: Option<String>,
    project_hex: &str,
    amount: u64,
    denom_str: &str,
) -> Result<(), String> {
    let kp = load_keypair(keystore, name)?;
    let address = derive_address(&kp.public_key());
    let project_id: [u8; 20] = hex_to_bytes(project_hex)?;
    let denom = parse_denomination(denom_str)?;

    println!("Claiming from reserve...");
    println!("  Project:  {}", hex::encode(project_id));
    println!("  Signer:   {}", hex::encode(address));
    println!("  Amount:   {} {}", amount, denom_str);

    let tx = Transaction::ClaimReserve {
        project_id,
        denomination: denom,
        amount: U256::from(amount),
    };

    let tx_hash = sign_and_submit(node, &kp, &tx)?;
    println!("Transaction submitted: {}", tx_hash);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_and_show() {
        let dir = tempdir().unwrap();
        let keystore = dir.path().to_path_buf();

        // Generate requires interactive password, so we test the keystore path logic
        assert_eq!(get_key_path(&keystore, None), keystore.join(DEFAULT_KEY_NAME));
        assert_eq!(
            get_key_path(&keystore, Some("test.key".to_string())),
            keystore.join("test.key")
        );
    }

    #[test]
    fn test_hex_to_bytes() {
        let result: [u8; 20] = hex_to_bytes("0102030405060708090a0b0c0d0e0f1011121314").unwrap();
        assert_eq!(
            result,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
        );
    }

    #[test]
    fn test_parse_denomination() {
        let denom = parse_denomination("USD").unwrap();
        assert_eq!(&denom[..3], b"USD");
        assert_eq!(&denom[3..], &[0u8; 5]);

        let denom = parse_denomination("ETHEREUM").unwrap();
        assert_eq!(denom, *b"ETHEREUM");
    }
}
