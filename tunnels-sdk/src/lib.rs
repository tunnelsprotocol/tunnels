//! Tunnels SDK - Key management and transaction signing.
//!
//! This crate provides a unified interface for key management and transaction
//! signing that works on both native Rust and WebAssembly targets.
//!
//! # Native Usage
//!
//! ```rust
//! use tunnels_sdk::{generate_keypair, encrypt_key, decrypt_key, get_address};
//!
//! // Generate a new keypair
//! let keypair = generate_keypair();
//!
//! // Get the address
//! let address = get_address(&keypair);
//!
//! // Encrypt for backup
//! let backup = encrypt_key(&keypair, "password").unwrap();
//! ```
//!
//! # WASM Usage
//!
//! The WASM build exposes JavaScript-friendly functions via wasm-bindgen.
//! **Important:** The private key is never exposed to JavaScript. Operations
//! like signing happen inside WASM memory.

#![deny(unsafe_code)]

use tunnels_core::{
    crypto::{derive_address, sign, verify},
    KeyPair, PublicKey, Signature,
    encrypt_key as core_encrypt, decrypt_key as core_decrypt, KeyFileError,
};


/// SDK error types.
#[derive(Debug, Clone)]
pub enum SdkError {
    /// Key file operation failed.
    KeyFile(KeyFileError),
    /// Transaction serialization failed.
    Serialization(String),
    /// Signature verification failed.
    VerificationFailed,
    /// Invalid key data.
    InvalidKey,
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SdkError::KeyFile(e) => write!(f, "key file error: {}", e),
            SdkError::Serialization(s) => write!(f, "serialization error: {}", s),
            SdkError::VerificationFailed => write!(f, "signature verification failed"),
            SdkError::InvalidKey => write!(f, "invalid key data"),
        }
    }
}

impl std::error::Error for SdkError {}

impl From<KeyFileError> for SdkError {
    fn from(e: KeyFileError) -> Self {
        SdkError::KeyFile(e)
    }
}

// ============================================================================
// Native API
// ============================================================================

/// Generate a new random keypair.
pub fn generate_keypair() -> KeyPair {
    KeyPair::generate()
}

/// Derive an address from a public key.
pub fn derive_address_from_pubkey(pubkey: &PublicKey) -> [u8; 20] {
    derive_address(pubkey)
}

/// Encrypt a keypair with a password for backup.
pub fn encrypt_key(keypair: &KeyPair, password: &str) -> Result<Vec<u8>, SdkError> {
    core_encrypt(keypair.secret_bytes(), password).map_err(SdkError::from)
}

/// Decrypt a keypair from an encrypted backup.
pub fn decrypt_key(encrypted: &[u8], password: &str) -> Result<KeyPair, SdkError> {
    let secret_bytes = core_decrypt(encrypted, password)?;
    KeyPair::from_bytes(&secret_bytes).map_err(|_| SdkError::InvalidKey)
}

/// Sign a serialized transaction.
pub fn sign_transaction_bytes(
    keypair: &KeyPair,
    tx_bytes: &[u8],
) -> Signature {
    sign(keypair.signing_key(), tx_bytes)
}

/// Verify a signature on transaction bytes.
pub fn verify_signature(pubkey: &PublicKey, tx_bytes: &[u8], signature: &Signature) -> bool {
    verify(pubkey, tx_bytes, signature).is_ok()
}

/// Get the address for a keypair.
pub fn get_address(keypair: &KeyPair) -> [u8; 20] {
    derive_address(&keypair.public_key())
}

// ============================================================================
// WASM API
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    // Thread-local storage for active keypairs (WASM is single-threaded)
    thread_local! {
        static KEYPAIRS: RefCell<HashMap<u32, KeyPair>> = RefCell::new(HashMap::new());
        static NEXT_ID: RefCell<u32> = const { RefCell::new(1) };
    }

    /// Opaque handle to a keypair stored in WASM memory.
    /// The private key never leaves WASM - only the handle ID is exposed to JS.
    #[wasm_bindgen]
    pub struct WasmKeyHandle {
        id: u32,
    }

    #[wasm_bindgen]
    impl WasmKeyHandle {
        /// Get the identity address (hex-encoded).
        /// This is the ONLY identity information exposed to JavaScript.
        #[wasm_bindgen(js_name = getAddress)]
        pub fn get_address(&self) -> String {
            KEYPAIRS.with(|kps| {
                let kps = kps.borrow();
                if let Some(kp) = kps.get(&self.id) {
                    hex_encode(&derive_address(&kp.public_key()))
                } else {
                    String::new()
                }
            })
        }

        /// Get the public key (hex-encoded).
        #[wasm_bindgen(js_name = getPublicKey)]
        pub fn get_public_key(&self) -> String {
            KEYPAIRS.with(|kps| {
                let kps = kps.borrow();
                if let Some(kp) = kps.get(&self.id) {
                    hex_encode(kp.public_key().as_bytes())
                } else {
                    String::new()
                }
            })
        }

        /// Sign transaction bytes and return the signature (hex-encoded).
        #[wasm_bindgen(js_name = signTransaction)]
        pub fn sign_transaction(&self, tx_bytes: &[u8]) -> Result<String, JsValue> {
            KEYPAIRS.with(|kps| {
                let kps = kps.borrow();
                let kp = kps.get(&self.id)
                    .ok_or_else(|| JsValue::from_str("invalid key handle"))?;
                let sig = sign(kp.signing_key(), tx_bytes);
                Ok(hex_encode(&sig.to_bytes()))
            })
        }

        /// Encrypt the key for backup. Returns encrypted bytes as base64.
        /// The private key stays in WASM memory - only the encrypted backup
        /// (which requires a password to decrypt) is returned.
        #[wasm_bindgen(js_name = encryptForBackup)]
        pub fn encrypt_for_backup(&self, password: &str) -> Result<Vec<u8>, JsValue> {
            KEYPAIRS.with(|kps| {
                let kps = kps.borrow();
                let kp = kps.get(&self.id)
                    .ok_or_else(|| JsValue::from_str("invalid key handle"))?;
                core_encrypt(kp.secret_bytes(), password)
                    .map_err(|e| JsValue::from_str(&e.to_string()))
            })
        }

        /// Release this key handle, removing the keypair from memory.
        #[wasm_bindgen]
        pub fn release(&self) {
            KEYPAIRS.with(|kps| {
                kps.borrow_mut().remove(&self.id);
            });
        }
    }

    impl Drop for WasmKeyHandle {
        fn drop(&mut self) {
            // Clean up when handle is dropped
            KEYPAIRS.with(|kps| {
                kps.borrow_mut().remove(&self.id);
            });
        }
    }

    /// Generate a new keypair. Returns a handle, not the actual key.
    #[wasm_bindgen(js_name = generateKey)]
    pub fn generate_key() -> WasmKeyHandle {
        let kp = KeyPair::generate();
        let id = NEXT_ID.with(|next| {
            let mut next = next.borrow_mut();
            let id = *next;
            *next = next.wrapping_add(1);
            id
        });
        KEYPAIRS.with(|kps| {
            kps.borrow_mut().insert(id, kp);
        });
        WasmKeyHandle { id }
    }

    /// Decrypt an encrypted backup and return a key handle.
    /// The decrypted private key stays in WASM memory.
    #[wasm_bindgen(js_name = decryptKey)]
    pub fn decrypt_key_wasm(encrypted: &[u8], password: &str) -> Result<WasmKeyHandle, JsValue> {
        let secret_bytes = core_decrypt(encrypted, password)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let kp = KeyPair::from_bytes(&secret_bytes)
            .map_err(|_| JsValue::from_str("invalid key data"))?;

        let id = NEXT_ID.with(|next| {
            let mut next = next.borrow_mut();
            let id = *next;
            *next = next.wrapping_add(1);
            id
        });
        KEYPAIRS.with(|kps| {
            kps.borrow_mut().insert(id, kp);
        });

        Ok(WasmKeyHandle { id })
    }

    /// Derive an address from a hex-encoded public key.
    #[wasm_bindgen(js_name = deriveAddress)]
    pub fn derive_address_wasm(pubkey_hex: &str) -> Result<String, JsValue> {
        let bytes = hex_decode(pubkey_hex)
            .map_err(|_| JsValue::from_str("invalid hex"))?;
        if bytes.len() != 32 {
            return Err(JsValue::from_str("public key must be 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let pubkey = PublicKey::from_bytes(&arr)
            .map_err(|_| JsValue::from_str("invalid public key"))?;
        Ok(hex_encode(&derive_address(&pubkey)))
    }

    /// Verify a signature. All parameters are hex-encoded.
    #[wasm_bindgen(js_name = verifySignature)]
    pub fn verify_signature_wasm(
        pubkey_hex: &str,
        message_hex: &str,
        signature_hex: &str,
    ) -> Result<bool, JsValue> {
        let pubkey_bytes = hex_decode(pubkey_hex)
            .map_err(|_| JsValue::from_str("invalid public key hex"))?;
        let message = hex_decode(message_hex)
            .map_err(|_| JsValue::from_str("invalid message hex"))?;
        let sig_bytes = hex_decode(signature_hex)
            .map_err(|_| JsValue::from_str("invalid signature hex"))?;

        if pubkey_bytes.len() != 32 {
            return Err(JsValue::from_str("public key must be 32 bytes"));
        }
        if sig_bytes.len() != 64 {
            return Err(JsValue::from_str("signature must be 64 bytes"));
        }

        let mut pubkey_arr = [0u8; 32];
        pubkey_arr.copy_from_slice(&pubkey_bytes);
        let pubkey = PublicKey::from_bytes(&pubkey_arr)
            .map_err(|_| JsValue::from_str("invalid public key"))?;

        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_arr);

        Ok(verify(&pubkey, &message, &signature).is_ok())
    }

    // Hex encoding/decoding utilities
    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
        if s.len() % 2 != 0 {
            return Err(());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
            .collect()
    }
}

// Re-export WASM types when targeting wasm32
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let kp = generate_keypair();
        assert_eq!(kp.public_key().as_bytes().len(), 32);
        assert_eq!(kp.secret_bytes().len(), 32);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let kp = generate_keypair();
        let password = "test_password";

        let encrypted = encrypt_key(&kp, password).unwrap();
        let recovered = decrypt_key(&encrypted, password).unwrap();

        assert_eq!(kp.public_key(), recovered.public_key());
        assert_eq!(kp.secret_bytes(), recovered.secret_bytes());
    }

    #[test]
    fn test_sign_verify() {
        let kp = generate_keypair();
        let message = b"test message";

        let signature = sign_transaction_bytes(&kp, message);
        assert!(verify_signature(&kp.public_key(), message, &signature));
    }

    #[test]
    fn test_wrong_message_fails_verify() {
        let kp = generate_keypair();
        let message = b"test message";
        let wrong_message = b"wrong message";

        let signature = sign_transaction_bytes(&kp, message);
        assert!(!verify_signature(&kp.public_key(), wrong_message, &signature));
    }

    #[test]
    fn test_address_derivation() {
        let kp = generate_keypair();
        let address = get_address(&kp);
        assert_eq!(address.len(), 20);

        let address2 = derive_address_from_pubkey(&kp.public_key());
        assert_eq!(address, address2);
    }

    #[test]
    fn test_interoperability_with_core() {
        use tunnels_core::crypto::derive_address as core_derive;

        let kp = generate_keypair();
        let sdk_address = get_address(&kp);
        let core_address = core_derive(&kp.public_key());

        assert_eq!(sdk_address, core_address);
    }
}
