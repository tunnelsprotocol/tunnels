//! Write buffer for tracking dirty state entries.
//!
//! The write buffer tracks which state entries have been modified during
//! block execution, enabling efficient batch commits to the trie.

use std::collections::HashSet;

use crate::keys::StateKey;

/// A buffer tracking dirty (modified) state entries.
#[derive(Clone, Debug, Default)]
pub struct WriteBuffer {
    /// Keys that have been modified.
    dirty: HashSet<StateKey>,
}

impl WriteBuffer {
    /// Create a new empty write buffer.
    pub fn new() -> Self {
        Self {
            dirty: HashSet::new(),
        }
    }

    /// Mark a key as dirty.
    pub fn mark_dirty(&mut self, key: StateKey) {
        self.dirty.insert(key);
    }

    /// Check if a key is dirty.
    pub fn is_dirty(&self, key: &StateKey) -> bool {
        self.dirty.contains(key)
    }

    /// Get all dirty keys.
    pub fn dirty_keys(&self) -> impl Iterator<Item = &StateKey> {
        self.dirty.iter()
    }

    /// Get the number of dirty entries.
    pub fn len(&self) -> usize {
        self.dirty.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.dirty.is_empty()
    }

    /// Clear all dirty entries.
    pub fn clear(&mut self) {
        self.dirty.clear();
    }

    /// Drain all dirty keys from the buffer.
    pub fn drain(&mut self) -> impl Iterator<Item = StateKey> + '_ {
        self.dirty.drain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer_empty() {
        let buffer = WriteBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_mark_dirty() {
        let mut buffer = WriteBuffer::new();
        let key = StateKey::Identity([1u8; 20]);

        assert!(!buffer.is_dirty(&key));
        buffer.mark_dirty(key.clone());
        assert!(buffer.is_dirty(&key));
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_mark_same_key_twice() {
        let mut buffer = WriteBuffer::new();
        let key = StateKey::Identity([1u8; 20]);

        buffer.mark_dirty(key.clone());
        buffer.mark_dirty(key.clone());
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut buffer = WriteBuffer::new();
        buffer.mark_dirty(StateKey::Identity([1u8; 20]));
        buffer.mark_dirty(StateKey::Project([2u8; 20]));

        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_drain() {
        let mut buffer = WriteBuffer::new();
        buffer.mark_dirty(StateKey::Identity([1u8; 20]));
        buffer.mark_dirty(StateKey::Project([2u8; 20]));

        let keys: Vec<_> = buffer.drain().collect();
        assert_eq!(keys.len(), 2);
        assert!(buffer.is_empty());
    }
}
