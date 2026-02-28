//! Encrypted key file format.
//!
//! Provides a secure, portable format for storing Ed25519 private keys.
//! All tools in the Tunnels ecosystem use this format for key backup and recovery.
//!
//! # File Format
//!
//! | Field      | Size (bytes) | Description                        |
//! |------------|--------------|-------------------------------------|
//! | Magic      | 4            | "TNLS" (0x544e4c53)                |
//! | Version    | 1            | Format version (currently 1)       |
//! | Salt       | 32           | Random salt for Argon2id           |
//! | Nonce      | 12           | Random nonce for AES-256-GCM       |
//! | Ciphertext | 48           | Encrypted key (32) + GCM tag (16)  |
//!
//! Total: 97 bytes
//!
//! # Security
//!
//! - Argon2id with recommended parameters for key derivation
//! - AES-256-GCM for authenticated encryption
//! - Unique random salt and nonce per file

use aes_gcm::{aead::Aead, Aes256Gcm, Nonce, KeyInit};
use argon2::{Argon2, Params, Version};
use rand::RngCore;

/// Magic bytes identifying a Tunnels key file.
pub const KEYFILE_MAGIC: &[u8; 4] = b"TNLS";

/// Current key file format version.
pub const KEYFILE_VERSION: u8 = 1;

/// Salt size in bytes (for Argon2id).
pub const SALT_SIZE: usize = 32;

/// Nonce size in bytes (for AES-GCM).
pub const NONCE_SIZE: usize = 12;

/// Private key size in bytes.
pub const KEY_SIZE: usize = 32;

/// GCM authentication tag size in bytes.
pub const TAG_SIZE: usize = 16;

/// Total key file size in bytes.
pub const KEYFILE_SIZE: usize = 4 + 1 + SALT_SIZE + NONCE_SIZE + KEY_SIZE + TAG_SIZE;

/// Argon2id parameters (OWASP recommended for 2024+)
const ARGON2_M_COST: u32 = 19 * 1024; // 19 MiB memory
const ARGON2_T_COST: u32 = 2;         // 2 iterations
const ARGON2_P_COST: u32 = 1;         // 1 thread

/// Errors that can occur during key file operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyFileError {
    /// File is too short to be a valid key file.
    FileTooShort {
        /// Expected file size in bytes.
        expected: usize,
        /// Actual file size in bytes.
        actual: usize,
    },
    /// Magic bytes don't match.
    InvalidMagic,
    /// Unsupported file format version.
    UnsupportedVersion {
        /// The version number found in the file.
        version: u8,
    },
    /// Password-based key derivation failed.
    KeyDerivationFailed,
    /// Decryption failed (wrong password or corrupted data).
    DecryptionFailed,
    /// Encryption failed.
    EncryptionFailed,
}

impl std::fmt::Display for KeyFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyFileError::FileTooShort { expected, actual } => {
                write!(f, "key file too short: expected {} bytes, got {}", expected, actual)
            }
            KeyFileError::InvalidMagic => write!(f, "not a valid Tunnels key file"),
            KeyFileError::UnsupportedVersion { version } => {
                write!(f, "unsupported key file version: {}", version)
            }
            KeyFileError::KeyDerivationFailed => write!(f, "key derivation failed"),
            KeyFileError::DecryptionFailed => {
                write!(f, "decryption failed (wrong password or corrupted file)")
            }
            KeyFileError::EncryptionFailed => write!(f, "encryption failed"),
        }
    }
}

impl std::error::Error for KeyFileError {}

/// Encrypt a private key with a password.
///
/// Returns the encrypted key file bytes (97 bytes total).
///
/// # Arguments
///
/// * `secret_bytes` - The 32-byte Ed25519 private key seed
/// * `password` - Password for encryption
///
/// # Example
///
/// ```
/// use tunnels_core::crypto::keyfile::encrypt_key;
/// use tunnels_core::KeyPair;
///
/// let kp = KeyPair::generate();
/// let encrypted = encrypt_key(kp.secret_bytes(), "my_password").unwrap();
/// assert_eq!(encrypted.len(), 97);
/// ```
pub fn encrypt_key(secret_bytes: &[u8; 32], password: &str) -> Result<Vec<u8>, KeyFileError> {
    // Generate random salt and nonce
    let mut salt = [0u8; SALT_SIZE];
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    // Derive encryption key using Argon2id
    let derived_key = derive_key(password.as_bytes(), &salt)?;

    // Encrypt the private key with AES-256-GCM
    let cipher = Aes256Gcm::new_from_slice(&derived_key)
        .map_err(|_| KeyFileError::EncryptionFailed)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, secret_bytes.as_ref())
        .map_err(|_| KeyFileError::EncryptionFailed)?;

    // Assemble the key file
    let mut result = Vec::with_capacity(KEYFILE_SIZE);
    result.extend_from_slice(KEYFILE_MAGIC);
    result.push(KEYFILE_VERSION);
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    debug_assert_eq!(result.len(), KEYFILE_SIZE);
    Ok(result)
}

/// Decrypt a private key from an encrypted key file.
///
/// # Arguments
///
/// * `encrypted` - The encrypted key file bytes
/// * `password` - Password used for encryption
///
/// # Returns
///
/// The decrypted 32-byte Ed25519 private key seed.
///
/// # Example
///
/// ```
/// use tunnels_core::crypto::keyfile::{encrypt_key, decrypt_key};
/// use tunnels_core::KeyPair;
///
/// let kp = KeyPair::generate();
/// let encrypted = encrypt_key(kp.secret_bytes(), "my_password").unwrap();
/// let decrypted = decrypt_key(&encrypted, "my_password").unwrap();
/// assert_eq!(kp.secret_bytes(), &decrypted);
/// ```
pub fn decrypt_key(encrypted: &[u8], password: &str) -> Result<[u8; 32], KeyFileError> {
    // Validate file size
    if encrypted.len() != KEYFILE_SIZE {
        return Err(KeyFileError::FileTooShort {
            expected: KEYFILE_SIZE,
            actual: encrypted.len(),
        });
    }

    // Parse header
    let magic = &encrypted[0..4];
    if magic != KEYFILE_MAGIC {
        return Err(KeyFileError::InvalidMagic);
    }

    let version = encrypted[4];
    if version != KEYFILE_VERSION {
        return Err(KeyFileError::UnsupportedVersion { version });
    }

    // Extract fields
    let salt = &encrypted[5..5 + SALT_SIZE];
    let nonce_bytes = &encrypted[5 + SALT_SIZE..5 + SALT_SIZE + NONCE_SIZE];
    let ciphertext = &encrypted[5 + SALT_SIZE + NONCE_SIZE..];

    // Derive decryption key
    let derived_key = derive_key(password.as_bytes(), salt)?;

    // Decrypt
    let cipher = Aes256Gcm::new_from_slice(&derived_key)
        .map_err(|_| KeyFileError::DecryptionFailed)?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| KeyFileError::DecryptionFailed)?;

    // Convert to fixed-size array
    if plaintext.len() != KEY_SIZE {
        return Err(KeyFileError::DecryptionFailed);
    }
    let mut result = [0u8; KEY_SIZE];
    result.copy_from_slice(&plaintext);

    Ok(result)
}

/// Derive a 256-bit encryption key from password and salt using Argon2id.
fn derive_key(password: &[u8], salt: &[u8]) -> Result<[u8; 32], KeyFileError> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(32))
        .map_err(|_| KeyFileError::KeyDerivationFailed)?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password, salt, &mut key)
        .map_err(|_| KeyFileError::KeyDerivationFailed)?;

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyPair;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let kp = KeyPair::generate();
        let password = "test_password_123!";

        let encrypted = encrypt_key(kp.secret_bytes(), password).unwrap();
        assert_eq!(encrypted.len(), KEYFILE_SIZE);

        let decrypted = decrypt_key(&encrypted, password).unwrap();
        assert_eq!(kp.secret_bytes(), &decrypted);
    }

    #[test]
    fn test_encrypted_file_format() {
        let secret = [0x42u8; 32];
        let password = "password";

        let encrypted = encrypt_key(&secret, password).unwrap();

        // Check magic
        assert_eq!(&encrypted[0..4], KEYFILE_MAGIC);
        // Check version
        assert_eq!(encrypted[4], KEYFILE_VERSION);
        // Check size
        assert_eq!(encrypted.len(), KEYFILE_SIZE);
    }

    #[test]
    fn test_wrong_password_fails() {
        let secret = [0x42u8; 32];
        let encrypted = encrypt_key(&secret, "correct_password").unwrap();

        let result = decrypt_key(&encrypted, "wrong_password");
        assert!(matches!(result, Err(KeyFileError::DecryptionFailed)));
    }

    #[test]
    fn test_invalid_magic_fails() {
        let mut data = vec![0u8; KEYFILE_SIZE];
        data[0..4].copy_from_slice(b"NOPE");

        let result = decrypt_key(&data, "password");
        assert!(matches!(result, Err(KeyFileError::InvalidMagic)));
    }

    #[test]
    fn test_unsupported_version_fails() {
        let mut data = vec![0u8; KEYFILE_SIZE];
        data[0..4].copy_from_slice(KEYFILE_MAGIC);
        data[4] = 99; // Future version

        let result = decrypt_key(&data, "password");
        assert!(matches!(result, Err(KeyFileError::UnsupportedVersion { version: 99 })));
    }

    #[test]
    fn test_file_too_short_fails() {
        let data = vec![0u8; 10];
        let result = decrypt_key(&data, "password");
        assert!(matches!(result, Err(KeyFileError::FileTooShort { .. })));
    }

    #[test]
    fn test_unique_encryptions() {
        let secret = [0x42u8; 32];
        let password = "password";

        let encrypted1 = encrypt_key(&secret, password).unwrap();
        let encrypted2 = encrypt_key(&secret, password).unwrap();

        // Different salt and nonce means different ciphertext
        assert_ne!(encrypted1, encrypted2);

        // But both decrypt to the same key
        let decrypted1 = decrypt_key(&encrypted1, password).unwrap();
        let decrypted2 = decrypt_key(&encrypted2, password).unwrap();
        assert_eq!(decrypted1, decrypted2);
        assert_eq!(decrypted1, secret);
    }

    #[test]
    fn test_empty_password() {
        let secret = [0x42u8; 32];
        let encrypted = encrypt_key(&secret, "").unwrap();
        let decrypted = decrypt_key(&encrypted, "").unwrap();
        assert_eq!(secret, decrypted);
    }

    #[test]
    fn test_unicode_password() {
        let secret = [0x42u8; 32];
        let password = "p@ssw0rd";

        let encrypted = encrypt_key(&secret, password).unwrap();
        let decrypted = decrypt_key(&encrypted, password).unwrap();
        assert_eq!(secret, decrypted);
    }

    #[test]
    fn test_keypair_roundtrip() {
        // Generate a real keypair and verify it survives encryption
        let kp1 = KeyPair::generate();
        let password = "secure_password";

        let encrypted = encrypt_key(kp1.secret_bytes(), password).unwrap();
        let decrypted = decrypt_key(&encrypted, password).unwrap();

        let kp2 = KeyPair::from_bytes(&decrypted).unwrap();

        // Public keys should match
        assert_eq!(kp1.public_key(), kp2.public_key());
        // Secret keys should match
        assert_eq!(kp1.secret_bytes(), kp2.secret_bytes());
    }
}
