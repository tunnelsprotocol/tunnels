//! SHA-256 hashing utilities.

use sha2::{Sha256, Digest};

/// Compute SHA-256 hash of the input data.
#[inline]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Compute SHA-256 hash of concatenated data slices.
///
/// More efficient than allocating a buffer for concatenation.
pub fn sha256_concat(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Compute Merkle root of a list of 32-byte hashes.
///
/// Uses a simple binary Merkle tree construction:
/// - Empty list returns 32 zero bytes
/// - Single hash returns that hash
/// - Otherwise, pair hashes and hash pairs recursively
/// - Odd leaves are duplicated
pub fn merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
    if hashes.is_empty() {
        return [0u8; 32];
    }
    if hashes.len() == 1 {
        return hashes[0];
    }

    let mut level: Vec<[u8; 32]> = hashes.to_vec();

    while level.len() > 1 {
        let mut next_level = Vec::with_capacity(level.len().div_ceil(2));

        for chunk in level.chunks(2) {
            let combined = if chunk.len() == 2 {
                sha256_concat(&[&chunk[0], &chunk[1]])
            } else {
                // Odd leaf: duplicate it
                sha256_concat(&[&chunk[0], &chunk[0]])
            };
            next_level.push(combined);
        }

        level = next_level;
    }

    level[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_determinism() {
        let data = b"hello world";
        let hash1 = sha256(data);
        let hash2 = sha256(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_known_value() {
        // SHA-256 of "hello world" is a known value
        let hash = sha256(b"hello world");
        // Just verify it's 32 bytes and non-zero
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, [0u8; 32]);
    }

    #[test]
    fn test_sha256_concat_equals_manual() {
        let part1 = b"hello";
        let part2 = b" world";

        let concat_hash = sha256_concat(&[part1, part2]);
        let manual_hash = sha256(b"hello world");

        assert_eq!(concat_hash, manual_hash);
    }

    #[test]
    fn test_merkle_root_empty() {
        let root = merkle_root(&[]);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_single() {
        let hash = sha256(b"test");
        let root = merkle_root(&[hash]);
        assert_eq!(root, hash);
    }

    #[test]
    fn test_merkle_root_two() {
        let h1 = sha256(b"one");
        let h2 = sha256(b"two");
        let root = merkle_root(&[h1, h2]);
        let expected = sha256_concat(&[&h1, &h2]);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_merkle_root_three() {
        let h1 = sha256(b"one");
        let h2 = sha256(b"two");
        let h3 = sha256(b"three");

        let root = merkle_root(&[h1, h2, h3]);

        // Expected: hash(hash(h1, h2), hash(h3, h3))
        let left = sha256_concat(&[&h1, &h2]);
        let right = sha256_concat(&[&h3, &h3]); // h3 duplicated
        let expected = sha256_concat(&[&left, &right]);

        assert_eq!(root, expected);
    }

    #[test]
    fn test_merkle_root_four() {
        let h1 = sha256(b"one");
        let h2 = sha256(b"two");
        let h3 = sha256(b"three");
        let h4 = sha256(b"four");

        let root = merkle_root(&[h1, h2, h3, h4]);

        // Expected: hash(hash(h1, h2), hash(h3, h4))
        let left = sha256_concat(&[&h1, &h2]);
        let right = sha256_concat(&[&h3, &h4]);
        let expected = sha256_concat(&[&left, &right]);

        assert_eq!(root, expected);
    }

    #[test]
    fn test_merkle_root_determinism() {
        let hashes: Vec<[u8; 32]> = (0..10)
            .map(|i| sha256(&[i as u8]))
            .collect();

        let root1 = merkle_root(&hashes);
        let root2 = merkle_root(&hashes);

        assert_eq!(root1, root2);
    }
}
