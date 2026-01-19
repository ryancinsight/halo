//! `BrandedLruCache` — a least-recently-used cache.
//!
//! This implementation uses a `BrandedHashMap` for O(1) lookups and a
//! `BrandedDoublyLinkedList` for O(1) maintenance of the LRU order.

use crate::collections::hash::BrandedHashMap;
use crate::collections::other::BrandedDoublyLinkedList;
use crate::GhostToken;
use core::fmt;
use core::hash::Hash;

/// A Least Recently Used (LRU) cache.
///
/// Keys are stored in both the hash map (for lookup) and the linked list (for ordering).
/// Therefore, keys must implement `Clone`.
pub struct BrandedLruCache<'brand, K, V> {
    map: BrandedHashMap<'brand, K, usize>,
    list: BrandedDoublyLinkedList<'brand, (K, V)>,
    capacity: usize,
}

impl<'brand, K, V> BrandedLruCache<'brand, K, V> {
    /// Returns the number of elements in the cache.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Returns `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Returns the capacity of the cache.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<'brand, K, V> BrandedLruCache<'brand, K, V>
where
    K: Hash + Eq + Clone,
{
    /// Creates a new LRU cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be non-zero");
        Self {
            map: BrandedHashMap::with_capacity(capacity),
            list: BrandedDoublyLinkedList::new(),
            capacity,
        }
    }

    /// Gets a reference to the value associated with `key`.
    ///
    /// This operation updates the LRU order, moving the accessed element to the front.
    /// Because it modifies the list structure, it requires `&mut self`.
    pub fn get<'a>(&'a mut self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if let Some(&index) = self.map.get(token, key) {
            self.list.move_to_front(token, index);
            // safe unwrap because map index comes from list
            let (_, v) = self.list.get(token, index).unwrap();
            Some(v)
        } else {
            None
        }
    }

    /// A version of `get` that allows mutating the value.
    pub fn get_mut<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
        key: &K,
    ) -> Option<&'a mut V> {
        if let Some(&index) = self.map.get(token, key) {
            self.list.move_to_front(token, index);
            let (_, v) = self.list.get_mut(token, index).unwrap();
            Some(v)
        } else {
            None
        }
    }

    /// Returns a reference to the value without updating the LRU order.
    pub fn peek<'a>(&'a self, token: &'a GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if let Some(&index) = self.map.get(token, key) {
            let (_, v) = self.list.get(token, index).unwrap();
            Some(v)
        } else {
            None
        }
    }

    /// Puts a key-value pair into the cache.
    pub fn put(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V> {
        if let Some(&index) = self.map.get(token, &key) {
            // Update existing
            self.list.move_to_front(token, index);
            let (_, v_ref) = self.list.get_mut(token, index).expect("Corrupted cache");
            let old_v = std::mem::replace(v_ref, value);
            Some(old_v)
        } else {
            // Insert new
            if self.len() == self.capacity {
                // Evict LRU
                if let Some((k, _)) = self.list.pop_back(token) {
                    self.map.remove(&k);
                }
            }
            let index = self.list.push_front(token, (key.clone(), value));
            self.map.insert(key, index);
            None
        }
    }
}

impl<'brand, K, V> Default for BrandedLruCache<'brand, K, V>
where
    K: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new(10) // Default capacity
    }
}

impl<'brand, K, V> fmt::Debug for BrandedLruCache<'brand, K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrandedLruCache")
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_lru_cache_basic() {
        GhostToken::new(|mut token| {
            let mut cache = BrandedLruCache::new(2);

            cache.put(&mut token, "a", 1);
            cache.put(&mut token, "b", 2);

            assert_eq!(cache.get(&mut token, &"a"), Some(&1)); // "a" is now MRU
            assert_eq!(cache.get(&mut token, &"b"), Some(&2)); // "b" is now MRU, "a" is LRU

            cache.put(&mut token, "c", 3); // Evicts "a"

            assert_eq!(cache.get(&mut token, &"a"), None);
            assert_eq!(cache.get(&mut token, &"b"), Some(&2));
            assert_eq!(cache.get(&mut token, &"c"), Some(&3));
        });
    }

    #[test]
    fn test_lru_cache_peek() {
        GhostToken::new(|mut token| {
            let mut cache = BrandedLruCache::new(2);
            cache.put(&mut token, "a", 1);
            cache.put(&mut token, "b", 2);
            // Order: b, a

            assert_eq!(cache.peek(&token, &"a"), Some(&1));
            // Order should still be b, a. "a" was not moved.

            cache.put(&mut token, "c", 3); // Should evict "a" if order was preserved
                                           // If peek moved "a", it would be a, b -> evict b.
                                           // If peek didn't move, it is b, a -> evict a.

            assert_eq!(cache.get(&mut token, &"a"), None);
            assert_eq!(cache.get(&mut token, &"b"), Some(&2));
            assert_eq!(cache.get(&mut token, &"c"), Some(&3));
        });
    }
}
