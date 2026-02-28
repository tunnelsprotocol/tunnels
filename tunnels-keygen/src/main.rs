//! Tunnels key generation tool.
//!
//! Generates Ed25519 keypairs for use with the Tunnels protocol.
//!
//! # Usage
//!
//! ```bash
//! # Generate a keypair (human-readable output)
//! tunnels-keygen
//!
//! # Generate a keypair (JSON output)
//! tunnels-keygen --json
//! ```

use clap::Parser;
use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use rand::rngs::OsRng;
use serde::Serialize;

/// Tunnels protocol key generation tool.
#[derive(Parser, Debug)]
#[command(name = "tunnels-keygen")]
#[command(about = "Generate Ed25519 keypairs for the Tunnels protocol")]
#[command(version)]
struct Cli {
    /// Output in JSON format for machine parsing.
    #[arg(long)]
    json: bool,

    /// Verify the generated keypair by signing and verifying a test message.
    #[arg(long)]
    verify: bool,
}

/// JSON output format.
#[derive(Serialize)]
struct KeypairJson {
    /// Public key in hex format (64 characters).
    public_key: String,
    /// Private key in hex format (64 characters for seed).
    private_key: String,
    /// Address derived from public key (40 characters).
    address: String,
}

/// Derive address from public key.
///
/// The address is the first 20 bytes of SHA-256(public_key).
fn derive_address(public_key: &VerifyingKey) -> [u8; 20] {
    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(public_key.as_bytes());
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

fn main() {
    let cli = Cli::parse();

    // Generate a new keypair using the OS random number generator
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    // Extract keys as bytes
    let private_key_bytes = signing_key.to_bytes();
    let public_key_bytes = verifying_key.to_bytes();
    let address = derive_address(&verifying_key);

    // Optionally verify the keypair works
    if cli.verify {
        let test_message = b"Tunnels protocol keypair verification";
        let signature: Signature = signing_key.sign(test_message);

        if verifying_key.verify(test_message, &signature).is_err() {
            eprintln!("ERROR: Generated keypair failed verification!");
            std::process::exit(1);
        }
    }

    if cli.json {
        let output = KeypairJson {
            public_key: hex::encode(public_key_bytes),
            private_key: hex::encode(private_key_bytes),
            address: hex::encode(address),
        };
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("=== Tunnels Protocol Keypair ===");
        println!();
        println!("Public Key:  {}", hex::encode(public_key_bytes));
        println!("Private Key: {}", hex::encode(private_key_bytes));
        println!("Address:     {}", hex::encode(address));
        println!();
        println!("IMPORTANT: Store your private key securely!");
        println!("           Never share it with anyone.");
        println!("           Anyone with this key can control your identity.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer, Verifier};

    #[test]
    fn test_keypair_generation_and_signing() {
        // Generate keypair
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        // Sign a message
        let message = b"Test message for Tunnels protocol";
        let signature = signing_key.sign(message);

        // Verify signature
        assert!(verifying_key.verify(message, &signature).is_ok());

        // Verify fails with wrong message
        let wrong_message = b"Wrong message";
        assert!(verifying_key.verify(wrong_message, &signature).is_err());
    }

    #[test]
    fn test_address_derivation() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let address = derive_address(&verifying_key);
        assert_eq!(address.len(), 20);

        // Same key should always produce same address
        let address2 = derive_address(&verifying_key);
        assert_eq!(address, address2);
    }

    #[test]
    fn test_keypair_hex_roundtrip() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        // Convert to hex and back
        let private_hex = hex::encode(signing_key.to_bytes());
        let private_bytes = hex::decode(&private_hex).unwrap();

        let restored_key = SigningKey::from_bytes(&private_bytes.try_into().unwrap());

        // Keys should be identical
        assert_eq!(signing_key.to_bytes(), restored_key.to_bytes());
        assert_eq!(
            signing_key.verifying_key().as_bytes(),
            restored_key.verifying_key().as_bytes()
        );
    }

    #[test]
    fn test_address_matches_tunnels_core() {
        use tunnels_core::crypto::{KeyPair, derive_address as core_derive_address};

        // Generate keypair using tunnels-core
        let keypair = KeyPair::generate();
        let core_address = core_derive_address(&keypair.public_key());

        // Derive address using our function
        let verifying_key = VerifyingKey::from_bytes(keypair.public_key().as_bytes()).unwrap();
        let our_address = derive_address(&verifying_key);

        // Should match
        assert_eq!(core_address, our_address);
    }
}
