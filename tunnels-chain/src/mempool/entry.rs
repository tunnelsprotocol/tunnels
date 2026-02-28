//! Mempool entry structure.

use tunnels_core::crypto::derive_address;
use tunnels_core::serialization::serialized_size;
use tunnels_core::transaction::SignedTransaction;

/// An entry in the mempool.
#[derive(Clone, Debug)]
pub struct MempoolEntry {
    /// The signed transaction.
    pub tx: SignedTransaction,

    /// Transaction ID (cached).
    pub tx_id: [u8; 32],

    /// Timestamp when added to mempool.
    pub added_at: u64,

    /// Signer address (for conflict detection).
    pub signer_address: [u8; 20],

    /// Size in bytes (for block packing).
    pub size: usize,

    /// Transaction PoW difficulty met (leading zero bits).
    pub pow_difficulty: u64,
}

impl MempoolEntry {
    /// Create a new mempool entry from a transaction.
    pub fn new(tx: SignedTransaction, added_at: u64) -> Self {
        let tx_id = tx.id();
        let signer_address = derive_address(&tx.signer);
        let size = serialized_size(&tx).unwrap_or(0) as usize;
        let pow_difficulty = count_leading_zero_bits(&tx.pow_hash);

        Self {
            tx,
            tx_id,
            added_at,
            signer_address,
            size,
            pow_difficulty,
        }
    }

    /// Get the transaction ID.
    #[inline]
    pub fn id(&self) -> [u8; 32] {
        self.tx_id
    }
}

/// Count leading zero bits in a hash.
fn count_leading_zero_bits(hash: &[u8; 32]) -> u64 {
    let mut count = 0u64;
    for byte in hash.iter() {
        if *byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros() as u64;
            break;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_leading_zero_bits() {
        assert_eq!(count_leading_zero_bits(&[0u8; 32]), 256);
        assert_eq!(count_leading_zero_bits(&[0xFF; 32]), 0);

        let mut hash = [0u8; 32];
        hash[0] = 0x01;
        assert_eq!(count_leading_zero_bits(&hash), 7);
    }
}
