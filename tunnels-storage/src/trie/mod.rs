//! Merkle Patricia Trie implementation.
//!
//! This module provides a Merkle Patricia Trie (MPT) for state commitments.
//! The trie uses SHA-256 for hashing and bincode for serialization.
//!
//! ## Structure
//!
//! The trie consists of three node types:
//! - **Leaf**: Stores a value at a key
//! - **Extension**: Compresses paths with single child
//! - **Branch**: 16-way branching point with optional value
//!
//! ## Usage
//!
//! ```ignore
//! let backend = MemoryBackend::new();
//! let mut trie = MerkleTrie::new(Arc::new(backend));
//!
//! // Insert values
//! trie.insert(b"key1", b"value1").unwrap();
//! trie.insert(b"key2", b"value2").unwrap();
//!
//! // Get the state root
//! let root = trie.root_hash();
//!
//! // Generate a proof
//! let proof = trie.prove(b"key1").unwrap();
//! assert!(proof.verify(&root));
//! ```

mod hasher;
mod nibbles;
mod node;
mod proof;

pub use hasher::{hash_node, is_empty_root, EMPTY_ROOT};
pub use nibbles::Nibbles;
pub use node::TrieNode;
pub use proof::MerkleProof;

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::StorageError;
use crate::keys::trie_node_key;
use crate::kv::KvBackend;

/// A Merkle Patricia Trie backed by a key-value store.
pub struct MerkleTrie<B: KvBackend> {
    /// The underlying key-value backend.
    backend: Arc<B>,
    /// The current root hash.
    root: [u8; 32],
    /// Cache of dirty (modified) nodes not yet persisted.
    dirty_nodes: HashMap<[u8; 32], TrieNode>,
}

impl<B: KvBackend> MerkleTrie<B> {
    /// Create a new empty trie.
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            root: EMPTY_ROOT,
            dirty_nodes: HashMap::new(),
        }
    }

    /// Create a trie from an existing root.
    pub fn from_root(backend: Arc<B>, root: [u8; 32]) -> Self {
        Self {
            backend,
            root,
            dirty_nodes: HashMap::new(),
        }
    }

    /// Get the current root hash.
    pub fn root_hash(&self) -> [u8; 32] {
        self.root
    }

    /// Check if the trie is empty.
    pub fn is_empty(&self) -> bool {
        is_empty_root(&self.root)
    }

    /// Get a value by key.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if is_empty_root(&self.root) {
            return Ok(None);
        }

        let nibbles = Nibbles::from_bytes(key);
        self.get_at(&self.root, &nibbles, 0)
    }

    /// Insert a key-value pair into the trie.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        let nibbles = Nibbles::from_bytes(key);
        let new_root = self.insert_at(self.root, &nibbles, 0, value.to_vec())?;
        self.root = new_root;
        Ok(())
    }

    /// Delete a key from the trie.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, StorageError> {
        if is_empty_root(&self.root) {
            return Ok(false);
        }

        let nibbles = Nibbles::from_bytes(key);
        match self.delete_at(self.root, &nibbles, 0)? {
            Some(new_root) => {
                self.root = new_root;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Generate a Merkle proof for a key.
    pub fn prove(&self, key: &[u8]) -> Result<MerkleProof, StorageError> {
        let nibbles = Nibbles::from_bytes(key);
        let (nodes, value) = self.collect_proof_path(&self.root, &nibbles, 0)?;
        Ok(MerkleProof::new(key.to_vec(), value, nodes))
    }

    /// Commit all dirty nodes to the backend.
    pub fn commit(&mut self) -> Result<(), StorageError> {
        use crate::kv::WriteBatch;

        let mut batch = WriteBatch::new();
        for (hash, node) in self.dirty_nodes.drain() {
            if !node.is_empty() {
                let key = trie_node_key(&hash);
                let value = node.to_bytes();
                batch.put(key, value);
            }
        }

        if !batch.is_empty() {
            self.backend.write_batch(batch)?;
        }

        Ok(())
    }

    /// Clear the dirty node cache without committing.
    pub fn rollback(&mut self) {
        self.dirty_nodes.clear();
    }

    /// Get a node by hash.
    fn get_node(&self, hash: &[u8; 32]) -> Result<TrieNode, StorageError> {
        if is_empty_root(hash) {
            return Ok(TrieNode::Empty);
        }

        // Check dirty cache first
        if let Some(node) = self.dirty_nodes.get(hash) {
            return Ok(node.clone());
        }

        // Load from backend
        let key = trie_node_key(hash);
        match self.backend.get(&key)? {
            Some(bytes) => {
                TrieNode::from_bytes(&bytes).map_err(|e| StorageError::Trie(e.to_string()))
            }
            None => Err(StorageError::NotFound(format!("Trie node: {:?}", hash))),
        }
    }

    /// Store a node and return its hash.
    fn store_node(&mut self, node: TrieNode) -> [u8; 32] {
        let hash = hash_node(&node);
        if !node.is_empty() {
            self.dirty_nodes.insert(hash, node);
        }
        hash
    }

    /// Get a value at a specific position in the trie.
    fn get_at(
        &self,
        node_hash: &[u8; 32],
        key: &Nibbles,
        offset: usize,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        if is_empty_root(node_hash) {
            return Ok(None);
        }

        let node = self.get_node(node_hash)?;

        match node {
            TrieNode::Empty => Ok(None),
            TrieNode::Leaf { partial, value } => {
                let remaining = key.slice_from(offset);
                let partial_nibbles = Nibbles::from_raw(partial);
                if remaining == partial_nibbles {
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }
            TrieNode::Extension { partial, child } => {
                let remaining = key.slice_from(offset);
                let partial_nibbles = Nibbles::from_raw(partial.clone());
                if remaining.starts_with(&partial_nibbles) {
                    self.get_at(&child, key, offset + partial.len())
                } else {
                    Ok(None)
                }
            }
            TrieNode::Branch { children, value } => {
                if offset >= key.len() {
                    Ok(value)
                } else {
                    let nibble = key.get(offset).unwrap() as usize;
                    match children[nibble] {
                        Some(child_hash) => self.get_at(&child_hash, key, offset + 1),
                        None => Ok(None),
                    }
                }
            }
        }
    }

    /// Insert a value at a specific position, returning the new node hash.
    fn insert_at(
        &mut self,
        node_hash: [u8; 32],
        key: &Nibbles,
        offset: usize,
        value: Vec<u8>,
    ) -> Result<[u8; 32], StorageError> {
        if is_empty_root(&node_hash) {
            // Create a new leaf
            let remaining = key.slice_from(offset);
            let leaf = TrieNode::leaf(remaining, value);
            return Ok(self.store_node(leaf));
        }

        let node = self.get_node(&node_hash)?;
        let remaining = key.slice_from(offset);

        match node {
            TrieNode::Empty => {
                let leaf = TrieNode::leaf(remaining, value);
                Ok(self.store_node(leaf))
            }
            TrieNode::Leaf {
                partial: existing_partial,
                value: existing_value,
            } => {
                let existing_nibbles = Nibbles::from_raw(existing_partial.clone());
                let common_len = remaining.common_prefix_length(&existing_nibbles);

                if common_len == existing_nibbles.len() && common_len == remaining.len() {
                    // Same key - update value
                    let new_leaf = TrieNode::leaf(remaining, value);
                    return Ok(self.store_node(new_leaf));
                }

                // Need to split
                let mut branch = TrieNode::branch();

                if common_len == existing_nibbles.len() {
                    // Existing key is prefix of new key
                    branch = branch.set_value(Some(existing_value));
                    let new_nibble = remaining.get(common_len).unwrap();
                    let new_partial = remaining.slice_from(common_len + 1);
                    let new_leaf = TrieNode::leaf(new_partial, value);
                    let new_hash = self.store_node(new_leaf);
                    branch = branch.set_child(new_nibble, Some(new_hash));
                } else if common_len == remaining.len() {
                    // New key is prefix of existing key
                    branch = branch.set_value(Some(value));
                    let existing_nibble = existing_nibbles.get(common_len).unwrap();
                    let existing_partial = existing_nibbles.slice_from(common_len + 1);
                    let existing_leaf = TrieNode::leaf(existing_partial, existing_value);
                    let existing_hash = self.store_node(existing_leaf);
                    branch = branch.set_child(existing_nibble, Some(existing_hash));
                } else {
                    // Keys diverge
                    let existing_nibble = existing_nibbles.get(common_len).unwrap();
                    let existing_partial = existing_nibbles.slice_from(common_len + 1);
                    let existing_leaf = TrieNode::leaf(existing_partial, existing_value);
                    let existing_hash = self.store_node(existing_leaf);
                    branch = branch.set_child(existing_nibble, Some(existing_hash));

                    let new_nibble = remaining.get(common_len).unwrap();
                    let new_partial = remaining.slice_from(common_len + 1);
                    let new_leaf = TrieNode::leaf(new_partial, value);
                    let new_hash = self.store_node(new_leaf);
                    branch = branch.set_child(new_nibble, Some(new_hash));
                }

                // Add extension if needed
                if common_len > 0 {
                    let branch_hash = self.store_node(branch);
                    let prefix = remaining.slice(0, common_len);
                    let extension = TrieNode::extension(prefix, branch_hash);
                    Ok(self.store_node(extension))
                } else {
                    Ok(self.store_node(branch))
                }
            }
            TrieNode::Extension {
                partial: ext_partial,
                child: ext_child,
            } => {
                let ext_nibbles = Nibbles::from_raw(ext_partial.clone());
                let common_len = remaining.common_prefix_length(&ext_nibbles);

                if common_len == ext_nibbles.len() {
                    // Extension fully consumed, recurse into child
                    let new_child = self.insert_at(ext_child, key, offset + ext_partial.len(), value)?;
                    let new_ext = TrieNode::extension(ext_nibbles, new_child);
                    Ok(self.store_node(new_ext))
                } else {
                    // Need to split the extension
                    let mut branch = TrieNode::branch();

                    // Add existing extension's remainder
                    let ext_nibble = ext_nibbles.get(common_len).unwrap();
                    if common_len + 1 < ext_nibbles.len() {
                        let ext_rest = ext_nibbles.slice_from(common_len + 1);
                        let new_ext = TrieNode::extension(ext_rest, ext_child);
                        let ext_hash = self.store_node(new_ext);
                        branch = branch.set_child(ext_nibble, Some(ext_hash));
                    } else {
                        branch = branch.set_child(ext_nibble, Some(ext_child));
                    }

                    // Add new key
                    if common_len < remaining.len() {
                        let new_nibble = remaining.get(common_len).unwrap();
                        let new_partial = remaining.slice_from(common_len + 1);
                        let new_leaf = TrieNode::leaf(new_partial, value);
                        let new_hash = self.store_node(new_leaf);
                        branch = branch.set_child(new_nibble, Some(new_hash));
                    } else {
                        branch = branch.set_value(Some(value));
                    }

                    // Add common prefix extension if needed
                    if common_len > 0 {
                        let branch_hash = self.store_node(branch);
                        let prefix = remaining.slice(0, common_len);
                        let extension = TrieNode::extension(prefix, branch_hash);
                        Ok(self.store_node(extension))
                    } else {
                        Ok(self.store_node(branch))
                    }
                }
            }
            TrieNode::Branch { mut children, value: branch_value } => {
                if offset >= key.len() {
                    // Key terminates at branch
                    let new_branch = TrieNode::branch_with(children, Some(value));
                    Ok(self.store_node(new_branch))
                } else {
                    let nibble = remaining.get(0).unwrap() as usize;
                    let child_hash = children[nibble].unwrap_or(EMPTY_ROOT);
                    let new_child = self.insert_at(child_hash, key, offset + 1, value)?;
                    children[nibble] = Some(new_child);
                    let new_branch = TrieNode::branch_with(children, branch_value);
                    Ok(self.store_node(new_branch))
                }
            }
        }
    }

    /// Delete a value at a specific position, returning the new node hash or None if key not found.
    fn delete_at(
        &mut self,
        node_hash: [u8; 32],
        key: &Nibbles,
        offset: usize,
    ) -> Result<Option<[u8; 32]>, StorageError> {
        if is_empty_root(&node_hash) {
            return Ok(None);
        }

        let node = self.get_node(&node_hash)?;
        let remaining = key.slice_from(offset);

        match node {
            TrieNode::Empty => Ok(None),
            TrieNode::Leaf { partial, .. } => {
                let leaf_nibbles = Nibbles::from_raw(partial);
                if remaining == leaf_nibbles {
                    Ok(Some(EMPTY_ROOT))
                } else {
                    Ok(None)
                }
            }
            TrieNode::Extension { partial, child } => {
                let ext_nibbles = Nibbles::from_raw(partial.clone());
                if !remaining.starts_with(&ext_nibbles) {
                    return Ok(None);
                }

                let new_child = self.delete_at(child, key, offset + partial.len())?;
                match new_child {
                    None => Ok(None),
                    Some(new_child_hash) => {
                        if is_empty_root(&new_child_hash) {
                            Ok(Some(EMPTY_ROOT))
                        } else {
                            // Try to collapse if child is extension or leaf
                            let child_node = self.get_node(&new_child_hash)?;
                            match child_node {
                                TrieNode::Leaf { partial: child_partial, value } => {
                                    let combined = ext_nibbles.concat(&Nibbles::from_raw(child_partial));
                                    let new_leaf = TrieNode::leaf(combined, value);
                                    Ok(Some(self.store_node(new_leaf)))
                                }
                                TrieNode::Extension { partial: child_partial, child: grandchild } => {
                                    let combined = ext_nibbles.concat(&Nibbles::from_raw(child_partial));
                                    let new_ext = TrieNode::extension(combined, grandchild);
                                    Ok(Some(self.store_node(new_ext)))
                                }
                                _ => {
                                    let new_ext = TrieNode::extension(ext_nibbles, new_child_hash);
                                    Ok(Some(self.store_node(new_ext)))
                                }
                            }
                        }
                    }
                }
            }
            TrieNode::Branch { mut children, value } => {
                if offset >= key.len() {
                    // Delete value at branch
                    if value.is_none() {
                        return Ok(None);
                    }
                    let new_branch = TrieNode::branch_with(children, None);
                    return Ok(Some(self.maybe_collapse_branch(new_branch)?));
                }

                let nibble = remaining.get(0).unwrap() as usize;
                let child_hash = match children[nibble] {
                    Some(h) => h,
                    None => return Ok(None),
                };

                let new_child = self.delete_at(child_hash, key, offset + 1)?;
                match new_child {
                    None => Ok(None),
                    Some(new_child_hash) => {
                        if is_empty_root(&new_child_hash) {
                            children[nibble] = None;
                        } else {
                            children[nibble] = Some(new_child_hash);
                        }
                        let new_branch = TrieNode::branch_with(children, value);
                        Ok(Some(self.maybe_collapse_branch(new_branch)?))
                    }
                }
            }
        }
    }

    /// Collapse a branch node if it has only one child or only a value.
    fn maybe_collapse_branch(&mut self, branch: TrieNode) -> Result<[u8; 32], StorageError> {
        let (children, value) = match &branch {
            TrieNode::Branch { children, value } => (*children, value.clone()),
            _ => return Ok(self.store_node(branch)),
        };

        let child_count = children.iter().filter(|c| c.is_some()).count();

        if child_count == 0 {
            // No children, only value or empty
            match value {
                Some(v) => {
                    let leaf = TrieNode::leaf(Nibbles::new(), v);
                    Ok(self.store_node(leaf))
                }
                None => Ok(EMPTY_ROOT),
            }
        } else if child_count == 1 && value.is_none() {
            // Single child, no value - collapse into extension
            let nibble = children.iter().position(|c| c.is_some()).unwrap();
            let child_hash = children[nibble].unwrap();
            let child_node = self.get_node(&child_hash)?;

            match child_node {
                TrieNode::Leaf { partial, value: leaf_value } => {
                    let mut combined = Nibbles::new();
                    combined.push(nibble as u8);
                    combined.extend(&Nibbles::from_raw(partial));
                    let new_leaf = TrieNode::leaf(combined, leaf_value);
                    Ok(self.store_node(new_leaf))
                }
                TrieNode::Extension { partial, child } => {
                    let mut combined = Nibbles::new();
                    combined.push(nibble as u8);
                    combined.extend(&Nibbles::from_raw(partial));
                    let new_ext = TrieNode::extension(combined, child);
                    Ok(self.store_node(new_ext))
                }
                _ => {
                    let ext = TrieNode::extension(Nibbles::from_raw(vec![nibble as u8]), child_hash);
                    Ok(self.store_node(ext))
                }
            }
        } else {
            Ok(self.store_node(branch))
        }
    }

    /// Collect nodes along the path for a proof.
    fn collect_proof_path(
        &self,
        node_hash: &[u8; 32],
        key: &Nibbles,
        offset: usize,
    ) -> Result<(Vec<TrieNode>, Option<Vec<u8>>), StorageError> {
        if is_empty_root(node_hash) {
            return Ok((vec![], None));
        }

        let node = self.get_node(node_hash)?;
        let remaining = key.slice_from(offset);

        match node {
            TrieNode::Empty => Ok((vec![TrieNode::Empty], None)),
            TrieNode::Leaf { ref partial, ref value } => {
                let leaf_nibbles = Nibbles::from_raw(partial.clone());
                let val = value.clone();
                if remaining == leaf_nibbles {
                    Ok((vec![node], Some(val)))
                } else {
                    Ok((vec![node], None))
                }
            }
            TrieNode::Extension { ref partial, ref child } => {
                let ext_nibbles = Nibbles::from_raw(partial.clone());
                let child_ref = *child;
                let partial_len = partial.len();
                if !remaining.starts_with(&ext_nibbles) {
                    return Ok((vec![node], None));
                }
                let (mut child_path, value) = self.collect_proof_path(&child_ref, key, offset + partial_len)?;
                let mut path = vec![node];
                path.append(&mut child_path);
                Ok((path, value))
            }
            TrieNode::Branch { ref children, ref value } => {
                let branch_value = value.clone();
                let children_copy = *children;
                if offset >= key.len() {
                    return Ok((vec![node], branch_value));
                }
                let nibble = remaining.get(0).unwrap() as usize;
                match children_copy[nibble] {
                    Some(child_hash) => {
                        let (mut child_path, value) = self.collect_proof_path(&child_hash, key, offset + 1)?;
                        let mut path = vec![node];
                        path.append(&mut child_path);
                        Ok((path, value))
                    }
                    None => Ok((vec![node], None)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::MemoryBackend;

    fn create_trie() -> MerkleTrie<MemoryBackend> {
        let backend = Arc::new(MemoryBackend::new());
        MerkleTrie::new(backend)
    }

    #[test]
    fn test_empty_trie() {
        let trie = create_trie();
        assert!(trie.is_empty());
        assert_eq!(trie.root_hash(), EMPTY_ROOT);
        assert!(trie.get(b"key").unwrap().is_none());
    }

    #[test]
    fn test_single_insert() {
        let mut trie = create_trie();
        trie.insert(b"key", b"value").unwrap();

        assert!(!trie.is_empty());
        assert_eq!(trie.get(b"key").unwrap(), Some(b"value".to_vec()));
        assert!(trie.get(b"other").unwrap().is_none());
    }

    #[test]
    fn test_multiple_inserts() {
        let mut trie = create_trie();
        trie.insert(b"key1", b"value1").unwrap();
        trie.insert(b"key2", b"value2").unwrap();
        trie.insert(b"key3", b"value3").unwrap();

        assert_eq!(trie.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(trie.get(b"key2").unwrap(), Some(b"value2".to_vec()));
        assert_eq!(trie.get(b"key3").unwrap(), Some(b"value3".to_vec()));
    }

    #[test]
    fn test_update_value() {
        let mut trie = create_trie();
        trie.insert(b"key", b"value1").unwrap();
        trie.insert(b"key", b"value2").unwrap();

        assert_eq!(trie.get(b"key").unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_delete() {
        let mut trie = create_trie();
        trie.insert(b"key1", b"value1").unwrap();
        trie.insert(b"key2", b"value2").unwrap();

        assert!(trie.delete(b"key1").unwrap());
        assert!(trie.get(b"key1").unwrap().is_none());
        assert_eq!(trie.get(b"key2").unwrap(), Some(b"value2".to_vec()));

        assert!(!trie.delete(b"nonexistent").unwrap());
    }

    #[test]
    fn test_delete_all() {
        let mut trie = create_trie();
        trie.insert(b"key", b"value").unwrap();
        trie.delete(b"key").unwrap();

        assert!(trie.is_empty());
        assert_eq!(trie.root_hash(), EMPTY_ROOT);
    }

    #[test]
    fn test_root_changes() {
        let mut trie = create_trie();
        let root0 = trie.root_hash();

        trie.insert(b"key1", b"value1").unwrap();
        let root1 = trie.root_hash();
        assert_ne!(root0, root1);

        trie.insert(b"key2", b"value2").unwrap();
        let root2 = trie.root_hash();
        assert_ne!(root1, root2);

        trie.delete(b"key2").unwrap();
        let root3 = trie.root_hash();
        assert_eq!(root1, root3);
    }

    #[test]
    fn test_proof_inclusion() {
        let mut trie = create_trie();
        trie.insert(b"key", b"value").unwrap();

        let proof = trie.prove(b"key").unwrap();
        assert!(proof.is_inclusion());
        assert!(proof.verify(&trie.root_hash()));
    }

    #[test]
    fn test_proof_exclusion() {
        let mut trie = create_trie();
        trie.insert(b"key", b"value").unwrap();

        let proof = trie.prove(b"other").unwrap();
        assert!(proof.is_exclusion());
        assert!(proof.verify(&trie.root_hash()));
    }

    #[test]
    fn test_commit_and_reload() {
        let backend = Arc::new(MemoryBackend::new());

        let root = {
            let mut trie = MerkleTrie::new(Arc::clone(&backend));
            trie.insert(b"key1", b"value1").unwrap();
            trie.insert(b"key2", b"value2").unwrap();
            trie.commit().unwrap();
            trie.root_hash()
        };

        // Reload from the same backend
        let trie = MerkleTrie::from_root(backend, root);
        assert_eq!(trie.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(trie.get(b"key2").unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_rollback() {
        let backend = Arc::new(MemoryBackend::new());

        // Create and commit initial state
        let initial_root = {
            let mut trie = MerkleTrie::new(Arc::clone(&backend));
            trie.insert(b"key1", b"value1").unwrap();
            trie.commit().unwrap();
            trie.root_hash()
        };

        // Make changes and rollback
        {
            let mut trie = MerkleTrie::from_root(Arc::clone(&backend), initial_root);
            trie.insert(b"key2", b"value2").unwrap();
            trie.rollback(); // Don't commit
        }

        // Verify original state is unchanged
        let trie = MerkleTrie::from_root(backend, initial_root);
        assert_eq!(trie.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        assert!(trie.get(b"key2").unwrap().is_none());
    }

    #[test]
    fn test_shared_prefix() {
        let mut trie = create_trie();
        trie.insert(b"abc", b"1").unwrap();
        trie.insert(b"abd", b"2").unwrap();
        trie.insert(b"abe", b"3").unwrap();

        assert_eq!(trie.get(b"abc").unwrap(), Some(b"1".to_vec()));
        assert_eq!(trie.get(b"abd").unwrap(), Some(b"2".to_vec()));
        assert_eq!(trie.get(b"abe").unwrap(), Some(b"3".to_vec()));
    }

    #[test]
    fn test_prefix_key() {
        let mut trie = create_trie();
        trie.insert(b"abc", b"1").unwrap();
        trie.insert(b"abcdef", b"2").unwrap();

        assert_eq!(trie.get(b"abc").unwrap(), Some(b"1".to_vec()));
        assert_eq!(trie.get(b"abcdef").unwrap(), Some(b"2".to_vec()));

        // Delete the prefix key
        trie.delete(b"abc").unwrap();
        assert!(trie.get(b"abc").unwrap().is_none());
        assert_eq!(trie.get(b"abcdef").unwrap(), Some(b"2".to_vec()));
    }

    #[test]
    fn test_deterministic_root() {
        let mut trie1 = create_trie();
        trie1.insert(b"a", b"1").unwrap();
        trie1.insert(b"b", b"2").unwrap();
        trie1.insert(b"c", b"3").unwrap();

        let mut trie2 = create_trie();
        trie2.insert(b"c", b"3").unwrap();
        trie2.insert(b"a", b"1").unwrap();
        trie2.insert(b"b", b"2").unwrap();

        assert_eq!(trie1.root_hash(), trie2.root_hash());
    }
}
