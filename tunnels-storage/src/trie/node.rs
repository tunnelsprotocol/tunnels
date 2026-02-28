//! Merkle Patricia Trie node types.
//!
//! The trie uses three node types:
//! - Leaf: stores a value at a key
//! - Extension: compresses paths with a single child
//! - Branch: 16-way branching point with optional value

use serde::{Deserialize, Serialize};

use super::Nibbles;

/// A node in the Merkle Patricia Trie.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum TrieNode {
    /// Empty node (represents absence of data).
    #[default]
    Empty,

    /// Leaf node containing a value.
    ///
    /// The partial path represents the remaining key nibbles from the
    /// parent's position to this leaf.
    Leaf {
        /// Remaining nibbles of the key.
        partial: Vec<u8>,
        /// The stored value.
        value: Vec<u8>,
    },

    /// Extension node with a compressed path to a single child.
    ///
    /// Extension nodes optimize paths with no branching by storing
    /// a sequence of nibbles that lead to the child.
    Extension {
        /// Shared nibbles leading to the child.
        partial: Vec<u8>,
        /// Hash of the child node.
        child: [u8; 32],
    },

    /// Branch node with 16 children and optional value.
    ///
    /// Each child slot corresponds to a nibble (0-15). The value
    /// slot holds data if a key terminates at this branch.
    Branch {
        /// Child node hashes (None if no child for that nibble).
        children: [Option<[u8; 32]>; 16],
        /// Optional value stored at this branch.
        value: Option<Vec<u8>>,
    },
}

impl TrieNode {
    /// Create an empty node.
    pub fn empty() -> Self {
        TrieNode::Empty
    }

    /// Create a leaf node.
    pub fn leaf(partial: Nibbles, value: Vec<u8>) -> Self {
        TrieNode::Leaf {
            partial: partial.as_slice().to_vec(),
            value,
        }
    }

    /// Create an extension node.
    pub fn extension(partial: Nibbles, child: [u8; 32]) -> Self {
        TrieNode::Extension {
            partial: partial.as_slice().to_vec(),
            child,
        }
    }

    /// Create a branch node with no children or value.
    pub fn branch() -> Self {
        TrieNode::Branch {
            children: [None; 16],
            value: None,
        }
    }

    /// Create a branch node with specific children and value.
    pub fn branch_with(children: [Option<[u8; 32]>; 16], value: Option<Vec<u8>>) -> Self {
        TrieNode::Branch { children, value }
    }

    /// Check if this is an empty node.
    pub fn is_empty(&self) -> bool {
        matches!(self, TrieNode::Empty)
    }

    /// Check if this is a leaf node.
    pub fn is_leaf(&self) -> bool {
        matches!(self, TrieNode::Leaf { .. })
    }

    /// Check if this is an extension node.
    pub fn is_extension(&self) -> bool {
        matches!(self, TrieNode::Extension { .. })
    }

    /// Check if this is a branch node.
    pub fn is_branch(&self) -> bool {
        matches!(self, TrieNode::Branch { .. })
    }

    /// Get the partial path (nibbles) for leaf or extension nodes.
    pub fn partial(&self) -> Option<Nibbles> {
        match self {
            TrieNode::Leaf { partial, .. } => Some(Nibbles::from_raw(partial.clone())),
            TrieNode::Extension { partial, .. } => Some(Nibbles::from_raw(partial.clone())),
            _ => None,
        }
    }

    /// Get the value for leaf or branch nodes.
    pub fn value(&self) -> Option<&[u8]> {
        match self {
            TrieNode::Leaf { value, .. } => Some(value),
            TrieNode::Branch { value: Some(v), .. } => Some(v),
            _ => None,
        }
    }

    /// Get the child hash for extension nodes.
    pub fn child(&self) -> Option<[u8; 32]> {
        match self {
            TrieNode::Extension { child, .. } => Some(*child),
            _ => None,
        }
    }

    /// Get a child hash at a specific nibble index for branch nodes.
    pub fn get_child(&self, nibble: u8) -> Option<[u8; 32]> {
        match self {
            TrieNode::Branch { children, .. } => children[nibble as usize],
            _ => None,
        }
    }

    /// Set a child hash at a specific nibble index for branch nodes.
    /// Returns the modified node.
    pub fn set_child(self, nibble: u8, child_hash: Option<[u8; 32]>) -> Self {
        match self {
            TrieNode::Branch { mut children, value } => {
                children[nibble as usize] = child_hash;
                TrieNode::Branch { children, value }
            }
            _ => self,
        }
    }

    /// Set the value for branch nodes. Returns the modified node.
    pub fn set_value(self, new_value: Option<Vec<u8>>) -> Self {
        match self {
            TrieNode::Branch { children, .. } => TrieNode::Branch {
                children,
                value: new_value,
            },
            _ => self,
        }
    }

    /// Count the number of children in a branch node.
    pub fn child_count(&self) -> usize {
        match self {
            TrieNode::Branch { children, .. } => children.iter().filter(|c| c.is_some()).count(),
            TrieNode::Extension { .. } => 1,
            TrieNode::Leaf { .. } => 0,
            TrieNode::Empty => 0,
        }
    }

    /// Get the single child nibble if branch has exactly one child.
    pub fn single_child_nibble(&self) -> Option<u8> {
        match self {
            TrieNode::Branch { children, .. } => {
                let mut found = None;
                for (i, child) in children.iter().enumerate() {
                    if child.is_some() {
                        if found.is_some() {
                            return None; // More than one child
                        }
                        found = Some(i as u8);
                    }
                }
                found
            }
            _ => None,
        }
    }

    /// Serialize the node to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        tunnels_core::serialization::serialize(self).expect("Node serialization should not fail")
    }

    /// Deserialize a node from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, tunnels_core::SerializationError> {
        tunnels_core::serialization::deserialize(bytes)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_node() {
        let node = TrieNode::empty();
        assert!(node.is_empty());
        assert!(!node.is_leaf());
        assert!(!node.is_extension());
        assert!(!node.is_branch());
    }

    #[test]
    fn test_leaf_node() {
        let partial = Nibbles::from_raw(vec![1, 2, 3]);
        let value = b"hello".to_vec();
        let node = TrieNode::leaf(partial.clone(), value.clone());

        assert!(node.is_leaf());
        assert_eq!(node.partial(), Some(partial));
        assert_eq!(node.value(), Some(value.as_slice()));
        assert_eq!(node.child_count(), 0);
    }

    #[test]
    fn test_extension_node() {
        let partial = Nibbles::from_raw(vec![4, 5, 6]);
        let child = [0xAB; 32];
        let node = TrieNode::extension(partial.clone(), child);

        assert!(node.is_extension());
        assert_eq!(node.partial(), Some(partial));
        assert_eq!(node.child(), Some(child));
        assert_eq!(node.child_count(), 1);
    }

    #[test]
    fn test_branch_node() {
        let mut node = TrieNode::branch();
        assert!(node.is_branch());
        assert_eq!(node.child_count(), 0);

        // Add children
        let child1 = [0x11; 32];
        let child2 = [0x22; 32];
        node = node.set_child(0, Some(child1));
        node = node.set_child(5, Some(child2));

        assert_eq!(node.child_count(), 2);
        assert_eq!(node.get_child(0), Some(child1));
        assert_eq!(node.get_child(5), Some(child2));
        assert_eq!(node.get_child(1), None);
    }

    #[test]
    fn test_branch_value() {
        let mut node = TrieNode::branch();
        assert!(node.value().is_none());

        node = node.set_value(Some(b"value".to_vec()));
        assert_eq!(node.value(), Some(b"value".as_slice()));
    }

    #[test]
    fn test_single_child_nibble() {
        let mut node = TrieNode::branch();
        assert_eq!(node.single_child_nibble(), None); // No children

        let child = [0x11; 32];
        node = node.set_child(7, Some(child));
        assert_eq!(node.single_child_nibble(), Some(7));

        // Add another child
        node = node.set_child(3, Some([0x22; 32]));
        assert_eq!(node.single_child_nibble(), None); // Multiple children
    }

    #[test]
    fn test_serialization() {
        let nodes = [
            TrieNode::empty(),
            TrieNode::leaf(Nibbles::from_raw(vec![1, 2]), b"value".to_vec()),
            TrieNode::extension(Nibbles::from_raw(vec![3, 4]), [0xAB; 32]),
            TrieNode::branch_with([None; 16], Some(b"branch_value".to_vec())),
        ];

        for node in nodes {
            let bytes = node.to_bytes();
            let recovered = TrieNode::from_bytes(&bytes).unwrap();
            assert_eq!(node, recovered);
        }
    }

    #[test]
    fn test_branch_with_children() {
        let mut children: [Option<[u8; 32]>; 16] = [None; 16];
        children[0] = Some([0x00; 32]);
        children[15] = Some([0xFF; 32]);

        let node = TrieNode::branch_with(children, Some(b"value".to_vec()));

        assert_eq!(node.child_count(), 2);
        assert_eq!(node.get_child(0), Some([0x00; 32]));
        assert_eq!(node.get_child(15), Some([0xFF; 32]));
        assert_eq!(node.value(), Some(b"value".as_slice()));
    }
}
