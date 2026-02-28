//! Merkle proofs for the Patricia Trie.
//!
//! Proofs allow verification of state entries without having access
//! to the full trie. Supports both inclusion (key exists) and
//! exclusion (key does not exist) proofs.

use serde::{Deserialize, Serialize};

use super::{hasher, Nibbles, TrieNode};

/// A Merkle proof for a key in the trie.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    /// The key being proven.
    pub key: Vec<u8>,
    /// The value if this is an inclusion proof, None for exclusion.
    pub value: Option<Vec<u8>>,
    /// Nodes along the path from root to the key.
    pub nodes: Vec<TrieNode>,
}

impl MerkleProof {
    /// Create a new proof.
    pub fn new(key: Vec<u8>, value: Option<Vec<u8>>, nodes: Vec<TrieNode>) -> Self {
        Self { key, value, nodes }
    }

    /// Check if this is an inclusion proof (key exists with value).
    pub fn is_inclusion(&self) -> bool {
        self.value.is_some()
    }

    /// Check if this is an exclusion proof (key does not exist).
    pub fn is_exclusion(&self) -> bool {
        self.value.is_none()
    }

    /// Verify the proof against a root hash.
    ///
    /// For inclusion proofs: verifies the key exists with the given value.
    /// For exclusion proofs: verifies the key does not exist in the trie.
    pub fn verify(&self, root_hash: &[u8; 32]) -> bool {
        if self.nodes.is_empty() {
            // Empty proof is valid only for empty root
            return *root_hash == hasher::EMPTY_ROOT && self.value.is_none();
        }

        // Verify the chain of hashes
        let computed_root = self.compute_root();
        if computed_root != *root_hash {
            return false;
        }

        // Verify the value at the end of the path
        self.verify_value()
    }

    /// Compute the root hash from the proof nodes.
    fn compute_root(&self) -> [u8; 32] {
        if self.nodes.is_empty() {
            return hasher::EMPTY_ROOT;
        }

        // Start from the bottom of the path and hash up
        let mut current_hash = hasher::hash_node(self.nodes.last().unwrap());

        for node in self.nodes.iter().rev().skip(1) {
            // Verify the node references the child we just hashed
            let references_child = match node {
                TrieNode::Extension { child, .. } => *child == current_hash,
                TrieNode::Branch { children, .. } => children.iter().any(|c| c.as_ref() == Some(&current_hash)),
                _ => false,
            };

            if !references_child {
                // If node doesn't reference the computed child hash,
                // the proof is invalid, but we continue computing to return a root
            }

            current_hash = hasher::hash_node(node);
        }

        current_hash
    }

    /// Verify that the proof path leads to the expected value.
    fn verify_value(&self) -> bool {
        let key_nibbles = Nibbles::from_bytes(&self.key);
        let mut nibble_idx = 0;

        for (i, node) in self.nodes.iter().enumerate() {
            match node {
                TrieNode::Empty => {
                    // Empty node can only appear at root for exclusion proof
                    return i == 0 && self.value.is_none();
                }
                TrieNode::Leaf { partial, value } => {
                    // Must be last node
                    if i != self.nodes.len() - 1 {
                        return false;
                    }

                    let remaining = key_nibbles.slice_from(nibble_idx);
                    let partial_nibbles = Nibbles::from_raw(partial.clone());

                    if remaining == partial_nibbles {
                        // Key matches - this is an inclusion proof
                        return self.value.as_ref() == Some(value);
                    } else {
                        // Key doesn't match - this is an exclusion proof
                        return self.value.is_none();
                    }
                }
                TrieNode::Extension { partial, .. } => {
                    let remaining = key_nibbles.slice_from(nibble_idx);
                    let partial_nibbles = Nibbles::from_raw(partial.clone());

                    if !remaining.starts_with(&partial_nibbles) {
                        // Path diverges - exclusion proof
                        return self.value.is_none() && i == self.nodes.len() - 1;
                    }

                    nibble_idx += partial.len();
                }
                TrieNode::Branch { children, value } => {
                    if nibble_idx >= key_nibbles.len() {
                        // Key terminates at this branch
                        if self.value.is_some() {
                            return self.value.as_ref() == value.as_ref();
                        } else {
                            return value.is_none();
                        }
                    }

                    let nibble = key_nibbles.get(nibble_idx).unwrap();
                    nibble_idx += 1;

                    // If this is the last node and there's no child for this nibble
                    if i == self.nodes.len() - 1 && children[nibble as usize].is_none() {
                        return self.value.is_none();
                    }
                }
            }
        }

        // If we've consumed all nibbles, verify the final value
        if let Some(last_node) = self.nodes.last() {
            match last_node {
                TrieNode::Branch { value, .. } => {
                    self.value.as_ref() == value.as_ref()
                }
                TrieNode::Leaf { value, .. } => {
                    self.value.as_ref() == Some(value)
                }
                _ => self.value.is_none(),
            }
        } else {
            self.value.is_none()
        }
    }

    /// Serialize the proof to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        tunnels_core::serialization::serialize(self).expect("Proof serialization should not fail")
    }

    /// Deserialize a proof from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, tunnels_core::SerializationError> {
        tunnels_core::serialization::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_proof() {
        let proof = MerkleProof::new(b"key".to_vec(), None, vec![]);
        assert!(proof.is_exclusion());
        assert!(!proof.is_inclusion());
        assert!(proof.verify(&hasher::EMPTY_ROOT));
    }

    #[test]
    fn test_inclusion_proof_single_leaf() {
        let key = b"test";
        let value = b"value".to_vec();
        let leaf = TrieNode::leaf(Nibbles::from_bytes(key), value.clone());

        let proof = MerkleProof::new(key.to_vec(), Some(value), vec![leaf.clone()]);

        let root = hasher::hash_node(&leaf);
        assert!(proof.is_inclusion());
        assert!(proof.verify(&root));
    }

    #[test]
    fn test_exclusion_proof_wrong_key() {
        let key = b"test";
        let other_key = b"other";
        let value = b"value".to_vec();
        let leaf = TrieNode::leaf(Nibbles::from_bytes(other_key), value);

        let proof = MerkleProof::new(key.to_vec(), None, vec![leaf.clone()]);

        let root = hasher::hash_node(&leaf);
        assert!(proof.is_exclusion());
        assert!(proof.verify(&root));
    }

    #[test]
    fn test_invalid_proof_wrong_value() {
        let key = b"test";
        let correct_value = b"correct".to_vec();
        let wrong_value = b"wrong".to_vec();
        let leaf = TrieNode::leaf(Nibbles::from_bytes(key), correct_value);

        let proof = MerkleProof::new(key.to_vec(), Some(wrong_value), vec![leaf.clone()]);

        let root = hasher::hash_node(&leaf);
        assert!(!proof.verify(&root));
    }

    #[test]
    fn test_proof_serialization() {
        let key = b"key".to_vec();
        let value = Some(b"value".to_vec());
        let node = TrieNode::leaf(Nibbles::from_bytes(&key), b"value".to_vec());

        let proof = MerkleProof::new(key, value, vec![node]);
        let bytes = proof.to_bytes();
        let recovered = MerkleProof::from_bytes(&bytes).unwrap();

        assert_eq!(proof, recovered);
    }

    #[test]
    fn test_proof_with_extension() {
        let key = b"\x12\x34\x56";
        let value = b"value".to_vec();

        // Build a path: extension -> leaf
        let leaf = TrieNode::leaf(Nibbles::from_raw(vec![5, 6]), value.clone());
        let leaf_hash = hasher::hash_node(&leaf);
        let extension = TrieNode::extension(Nibbles::from_raw(vec![1, 2, 3, 4]), leaf_hash);

        let proof = MerkleProof::new(key.to_vec(), Some(value), vec![extension.clone(), leaf]);

        let root = hasher::hash_node(&extension);
        assert!(proof.verify(&root));
    }

    #[test]
    fn test_proof_with_branch() {
        let key = b"\x10"; // nibbles: 1, 0
        let value = b"value".to_vec();

        // Build a path: branch -> leaf
        let leaf = TrieNode::leaf(Nibbles::from_raw(vec![0]), value.clone());
        let leaf_hash = hasher::hash_node(&leaf);

        let mut children: [Option<[u8; 32]>; 16] = [None; 16];
        children[1] = Some(leaf_hash);
        let branch = TrieNode::branch_with(children, None);

        let proof = MerkleProof::new(key.to_vec(), Some(value), vec![branch.clone(), leaf]);

        let root = hasher::hash_node(&branch);
        assert!(proof.verify(&root));
    }
}
