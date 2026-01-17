//! `BrandedLruCache` — a least-recently-used cache with token-gated access.
//!
//! This implementation combines a `BrandedHashMap` and a `BrandedDoublyLinkedList`
//! to provide an O(1) LRU cache. It relies on the `GhostCell` paradigm for safety.
//!
//! Key features:
//! - O(1) access, insertion, and eviction.
//! - Token-gated access ensures thread safety and memory safety.
//! - Uses `Clone` on keys as they are stored in both the map and the list.

use crate::{GhostToken, collections::{BrandedHashMap, BrandedDoublyLinkedList}};
use core::hash::Hash;

/// A Least Recently Used (LRU) cache.
pub struct BrandedLruCache<'brand, K, V> {
    map: BrandedHashMap<'brand, K, usize>,
    list: BrandedDoublyLinkedList<'brand, (K, V)>,
    capacity: usize,
}

impl<'brand, K, V> BrandedLruCache<'brand, K, V>
where
    K: Clone + Hash + Eq,
{
    /// Creates a new LRU cache with the given capacity.
    ///
    /// # Panics
    /// Panics if capacity is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be non-zero");
        Self {
            map: BrandedHashMap::new(),
            list: BrandedDoublyLinkedList::new(),
            capacity,
        }
    }

    /// Returns the number of elements in the cache.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the capacity of the cache.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clears the cache.
    pub fn clear(&mut self, token: &mut GhostToken<'brand>) {
        self.map.clear();
        self.list.clear(token);
    }

    /// Returns a reference to the value of the key in the cache or `None` if it is not present.
    ///
    /// Moves the key to the head of the LRU list.
    pub fn get<'a>(&'a mut self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if let Some(&idx) = self.map.get(token, key) {
            self.list.move_to_front(token, idx);
            self.list.get(token, idx).map(|(_k, v)| v)
        } else {
            None
        }
    }

    /// Returns a mutable reference to the value of the key in the cache or `None` if it is not present.
    ///
    /// Moves the key to the head of the LRU list.
    pub fn get_mut<'a>(&'a mut self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a mut V> {
        if let Some(&idx) = self.map.get(token, key) {
            self.list.move_to_front(token, idx);
            self.list.get_mut(token, idx).map(|(_k, v)| v)
        } else {
            None
        }
    }

    /// Returns a reference to the value without updating the LRU list.
    pub fn peek<'a>(&'a self, token: &'a GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if let Some(&idx) = self.map.get(token, key) {
            self.list.get(token, idx).map(|(_k, v)| v)
        } else {
            None
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the key is already present, the value is updated and the key is moved to the head.
    /// If the key is not present, it is inserted at the head.
    /// If the cache is full, the least recently used item is evicted.
    ///
    /// Returns the old value if the key was already present.
    pub fn put(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V> {
        if let Some(&idx) = self.map.get(token, &key) {
            self.list.move_to_front(token, idx);
            let slot = self.list.get_mut(token, idx).unwrap();
            let old_val = std::mem::replace(&mut slot.1, value);
            Some(old_val)
        } else {
            if self.len() == self.capacity {
                 let (k, _v) = self.list.pop_back(token).unwrap();
                 self.map.remove(&k);
            }
            let idx = self.list.push_front(token, (key.clone(), value));
            self.map.insert(key, idx);
            None
        }
    }

    /// Removes a key from the cache, returning the value if it existed.
    pub fn pop(&mut self, token: &mut GhostToken<'brand>, key: &K) -> Option<V> {
        if let Some(idx) = self.map.remove(key) {
            // We need to remove the node from the list.
            // BrandedDoublyLinkedList doesn't have remove_at_index exposed easily except via free or similar.
            // But we can use move_to_front then pop_front.
            self.list.move_to_front(token, idx);
            let (_k, v) = self.list.pop_front(token).unwrap();
            Some(v)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_lru_basic() {
        GhostToken::new(|mut token| {
            let mut cache = BrandedLruCache::new(2);
            cache.put(&mut token, "a", 1);
            cache.put(&mut token, "b", 2);

            assert_eq!(cache.get(&mut token, &"a"), Some(&1));
            assert_eq!(cache.get(&mut token, &"b"), Some(&2));

            // "b" was accessed last, so it's most recent.
            // Wait, get("a") was called first, then get("b").
            // "b" is head.

            cache.put(&mut token, "c", 3); // evicts "a" because "a" was accessed before "b".
            // Wait order:
            // put a (a)
            // put b (b, a)
            // get a -> (a, b)
            // get b -> (b, a)
            // put c -> (c, b) . Evicts a.

            assert_eq!(cache.get(&mut token, &"a"), None);
            assert_eq!(cache.get(&mut token, &"b"), Some(&2));
            assert_eq!(cache.get(&mut token, &"c"), Some(&3));
        });
    }

    #[test]
    fn test_lru_update() {
        GhostToken::new(|mut token| {
            let mut cache = BrandedLruCache::new(2);
            cache.put(&mut token, "a", 1);
            cache.put(&mut token, "b", 2);

            // Update a, moves to front. Order: (a, b)
            cache.put(&mut token, "a", 10);

            // Add c, evicts b. Order: (c, a)
            cache.put(&mut token, "c", 3);

            assert_eq!(cache.get(&mut token, &"b"), None);
            assert_eq!(cache.get(&mut token, &"a"), Some(&10));
            assert_eq!(cache.get(&mut token, &"c"), Some(&3));
        });
    }

    #[test]
    fn test_lru_peek() {
        GhostToken::new(|mut token| {
            let mut cache = BrandedLruCache::new(2);
            cache.put(&mut token, "a", 1);
            cache.put(&mut token, "b", 2);
            // Order: (b, a)

            // Peek a. Order remains (b, a)
            assert_eq!(cache.peek(&token, &"a"), Some(&1));

            // Put c. Evicts a.
            cache.put(&mut token, "c", 3);

            assert_eq!(cache.get(&mut token, &"a"), None);
        });
    }
}
