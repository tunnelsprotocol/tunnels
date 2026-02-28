//! SHA-256 node hashing for the Merkle Patricia Trie.
//!
//! Each trie node is hashed using SHA-256 of its serialized form.
//! The hash serves as the node's address in the trie storage.

use tunnels_core::crypto::sha256;

use super::TrieNode;

/// Hash representing an empty trie.
pub const EMPTY_ROOT: [u8; 32] = [0u8; 32];

/// Compute the hash of a trie node.
///
/// The hash is SHA-256 of the bincode-serialized node.
pub fn hash_node(node: &TrieNode) -> [u8; 32] {
    if node.is_empty() {
        return EMPTY_ROOT;
    }
    let bytes = node.to_bytes();
    sha256(&bytes)
}

/// Check if a hash represents an empty trie.
pub fn is_empty_root(hash: &[u8; 32]) -> bool {
    *hash == EMPTY_ROOT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trie::Nibbles;

    #[test]
    fn test_empty_node_hash() {
        let node = TrieNode::empty();
        let hash = hash_node(&node);
        assert_eq!(hash, EMPTY_ROOT);
        assert!(is_empty_root(&hash));
    }

    #[test]
    fn test_leaf_node_hash() {
        let node = TrieNode::leaf(Nibbles::from_raw(vec![1, 2]), b"value".to_vec());
        let hash = hash_node(&node);
        assert_ne!(hash, EMPTY_ROOT);
        assert!(!is_empty_root(&hash));
    }

    #[test]
    fn test_hash_determinism() {
        let node = TrieNode::leaf(Nibbles::from_raw(vec![1, 2, 3]), b"test".to_vec());
        let hash1 = hash_node(&node);
        let hash2 = hash_node(&node);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_nodes_different_hashes() {
        let node1 = TrieNode::leaf(Nibbles::from_raw(vec![1, 2]), b"value1".to_vec());
        let node2 = TrieNode::leaf(Nibbles::from_raw(vec![1, 2]), b"value2".to_vec());
        let node3 = TrieNode::leaf(Nibbles::from_raw(vec![1, 3]), b"value1".to_vec());

        let hash1 = hash_node(&node1);
        let hash2 = hash_node(&node2);
        let hash3 = hash_node(&node3);

        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3);
    }

    #[test]
    fn test_branch_node_hash() {
        let mut children: [Option<[u8; 32]>; 16] = [None; 16];
        children[0] = Some([0x11; 32]);
        children[5] = Some([0x55; 32]);

        let node = TrieNode::branch_with(children, Some(b"branch".to_vec()));
        let hash = hash_node(&node);

        assert_ne!(hash, EMPTY_ROOT);
    }

    #[test]
    fn test_extension_node_hash() {
        let node = TrieNode::extension(Nibbles::from_raw(vec![1, 2, 3]), [0xAB; 32]);
        let hash = hash_node(&node);

        assert_ne!(hash, EMPTY_ROOT);
    }
}
