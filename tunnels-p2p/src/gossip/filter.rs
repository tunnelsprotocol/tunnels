//! Deduplication filter using LRU cache.

use lru::LruCache;
use std::num::NonZeroUsize;

/// Default capacity for seen items.
const DEFAULT_CAPACITY: usize = 10000;

/// LRU-based deduplication filter.
pub struct SeenFilter {
    /// LRU cache of seen hashes.
    cache: LruCache<[u8; 32], ()>,
}

impl SeenFilter {
    /// Create a new filter with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            cache: LruCache::new(capacity),
        }
    }

    /// Create a new filter with default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Check if an item has been seen before.
    ///
    /// Returns true if the item was NOT seen before (i.e., is new).
    /// Returns false if the item WAS seen before (i.e., is duplicate).
    pub fn check(&mut self, hash: &[u8; 32]) -> bool {
        if self.cache.contains(hash) {
            false // Already seen
        } else {
            self.cache.put(*hash, ());
            true // New item
        }
    }

    /// Check if an item is in the filter without modifying it.
    pub fn contains(&self, hash: &[u8; 32]) -> bool {
        self.cache.contains(hash)
    }

    /// Mark an item as seen.
    pub fn mark_seen(&mut self, hash: [u8; 32]) {
        self.cache.put(hash, ());
    }

    /// Get the number of items in the filter.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the filter is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clear the filter.
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for SeenFilter {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seen_filter() {
        let mut filter = SeenFilter::new(100);

        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];

        // First check should return true (new item)
        assert!(filter.check(&hash1));

        // Second check should return false (duplicate)
        assert!(!filter.check(&hash1));

        // Different hash should return true
        assert!(filter.check(&hash2));

        assert_eq!(filter.len(), 2);
    }

    #[test]
    fn test_lru_eviction() {
        let mut filter = SeenFilter::new(2);

        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];
        let hash3 = [3u8; 32];

        filter.check(&hash1);
        filter.check(&hash2);

        // Cache is full, adding hash3 should evict hash1
        filter.check(&hash3);

        // hash1 should no longer be in cache
        assert!(!filter.contains(&hash1));
        assert!(filter.contains(&hash2));
        assert!(filter.contains(&hash3));
    }

    #[test]
    fn test_mark_seen() {
        let mut filter = SeenFilter::new(100);
        let hash = [1u8; 32];

        filter.mark_seen(hash);
        assert!(filter.contains(&hash));
        assert!(!filter.check(&hash)); // Should be duplicate
    }
}
