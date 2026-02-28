//! # Tunnels Core
//!
//! Core types, cryptography, and serialization for the Tunnels protocol.
//!
//! This crate provides the foundation for all other Tunnels crates:
//! - Cryptographic primitives (Ed25519 signatures, SHA-256 hashing)
//! - Protocol data types (Identity, Project, Attestation, etc.)
//! - Transaction types (all 13 protocol transaction variants)
//! - Block structure and Merkle tree computation
//! - 256-bit arithmetic for fixed-point accumulator math
//! - Deterministic binary serialization

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod crypto;
pub mod error;
pub mod serialization;
pub mod types;
pub mod transaction;
pub mod block;
pub mod u256;

// Re-export commonly used types at crate root
pub use crypto::{KeyPair, PublicKey, Signature, SecretKey, KeyFileError, encrypt_key, decrypt_key};
pub use error::{CoreError, CryptoError, SerializationError, DenominationError};
pub use types::{
    Identity, IdentityType,
    Project, RecipientType,
    Attestation, AttestationStatus,
    Challenge, ChallengeStatus, ChallengeRuling,
    AccumulatorState, ContributorBalance, TransactionDifficultyState,
    Denomination,
};
pub use transaction::{Transaction, SignedTransaction};
pub use block::{Block, BlockHeader};
pub use u256::U256;
