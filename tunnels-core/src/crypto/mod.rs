//! Cryptographic primitives for the Tunnels protocol.
//!
//! This module provides:
//! - Ed25519 key pair generation, signing, and verification
//! - SHA-256 hashing
//! - Identity address derivation (first 20 bytes of SHA-256 of public key)
//! - Encrypted key file format (Argon2id + AES-256-GCM)
//!
//! All cryptographic choices follow Rule 9: conservative, interoperable,
//! with decades of scrutiny.

mod address;
mod hashing;
pub mod keyfile;
mod keys;
mod signing;

pub use address::derive_address;
pub use hashing::{sha256, sha256_concat, merkle_root};
pub use keyfile::{encrypt_key, decrypt_key, KeyFileError};
pub use keys::{KeyPair, PublicKey, SecretKey};
pub use signing::{sign, verify, Signature};
