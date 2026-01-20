//! `BrandedIndexMap` — a high-performance ordered hash map with token-gated values.
//!
//! This implementation combines a **SwissTable-inspired** hash table for lookups with
//! **dense vectors** for storage, preserving insertion order and enabling fast iteration.
//!
//! Structure:
//! - **Hash Table**: Stores indices into the dense vectors. Uses control bytes for SIMD probing.
//! - **Dense Vectors**: `keys` (Vec<K>) and `values` (BrandedVec<V>) store the actual data.
//!
//! Benefits:
//! - **Order Preservation**: Iteration order matches insertion order.
//! - **Fast Iteration**: Iterating over dense vectors is cache-friendly.
//! - **Zero-Cost Access**: Values are token-gated using `GhostCell`.
//! - **Index Access**: O(1) access by index.

use crate::collections::{BrandedCollection, BrandedVec, ZeroCopyMapOps};
use crate::{GhostCell, GhostToken};
use core::hash::{BuildHasher, Hash, Hasher};
use std::collections::hash_map::RandomState;
// Control byte constants
const EMPTY: u8 = 0xFF;
const DELETED: u8 = 0xFE;
const GROUP_WIDTH: usize = 8;

/// Returns a mask where each byte is 0x80 if the corresponding byte in `x` is zero, else 0x00.
#[inline(always)]
fn has_zero_byte(x: u64) -> u64 {
    (x.wrapping_sub(0x0101010101010101)) & (!x) & 0x8080808080808080
}

/// Returns a mask where each byte is 0x80 if the corresponding byte in `x` matches `y`, else 0x00.
#[inline(always)]
fn match_byte(x: u64, y: u8) -> u64 {
    let pattern = (y as u64) * 0x0101010101010101;
    has_zero_byte(x ^ pattern)
}

/// High-performance ordered hash map.
pub struct BrandedIndexMap<'brand, K, V, S = RandomState> {
    /// Control bytes for the hash table part.
    ctrl: Box<[u8]>,
    /// Slots storing indices into the dense vectors.
    slots: Box<[usize]>,

    /// Dense storage for keys.
    keys: Vec<K>,
    /// Dense storage for values (wrapped in GhostCell).
    values: BrandedVec<'brand, V>,

    /// Number of items in the map (load factor tracking).
    /// Note: `keys.len()` tracks the actual number of elements.
    /// `items_count` tracks occupied + deleted slots in the hash table to trigger rehash.
    items_count: usize,

    /// Capacity of the hash table (power of 2).
    table_capacity: usize,

    hash_builder: S,
}

impl<'brand, K, V> BrandedIndexMap<'brand, K, V, RandomState> {
    /// Creates an empty map with default capacity.
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity_and_hasher(0, RandomState::new())
    }

    /// Creates an empty map with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<'brand, K, V, S> BrandedIndexMap<'brand, K, V, S> {
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let table_capacity = if capacity == 0 {
            0
        } else {
            // Target load factor ~0.875
            (capacity * 8 / 7).next_power_of_two().max(8)
        };

        if table_capacity == 0 {
            return Self {
                ctrl: Box::new([]),
                slots: Box::new([]),
                keys: Vec::new(),
                values: BrandedVec::new(),
                items_count: 0,
                table_capacity: 0,
                hash_builder,
            };
        }

        let ctrl_len = table_capacity + GROUP_WIDTH;
        let ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
        // Initialize slots to 0. We rely on ctrl bytes to know if a slot is occupied.
        let slots = vec![0; table_capacity].into_boxed_slice();

        Self {
            ctrl,
            slots,
            keys: Vec::with_capacity(capacity),
            values: BrandedVec::with_capacity(capacity),
            items_count: 0,
            table_capacity,
            hash_builder,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.keys.capacity()
    }

    /// Returns the key-value pair at the given index.
    pub fn get_index<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        index: usize,
    ) -> Option<(&'a K, &'a V)> {
        if index < self.keys.len() {
            let key = &self.keys[index];
            let val = self.values.get(token, index)?;
            Some((key, val))
        } else {
            None
        }
    }

    /// Returns the key-value pair at the given index (mutable).
    pub fn get_index_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        index: usize,
    ) -> Option<(&'a K, &'a mut V)> {
        if index < self.keys.len() {
            let key = &self.keys[index];
            let val = self.values.get_mut(token, index)?;
            Some((key, val))
        } else {
            None
        }
    }

    /// Clears the map.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
        self.items_count = 0;

        // Reset ctrl bytes to EMPTY
        self.ctrl.fill(EMPTY);
    }

    /// Iterator over keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.keys.iter()
    }

    /// Iterator over values.
    pub fn values<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a V> {
        self.values.iter(token)
    }

    /// Iterator over key-value pairs.
    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.keys.iter().zip(self.values.iter(token))
    }

    /// Iterator over key-value pairs (mutable).
    pub fn iter_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
    ) -> impl Iterator<Item = (&'a K, &'a mut V)> {
        self.keys
            .iter()
            .zip(self.values.as_mut_slice(token).iter_mut())
    }
}

impl<'brand, K, V, S> BrandedIndexMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    #[inline]
    fn hash(&self, key: &K) -> (usize, u8) {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let h1 = (hash as usize) & (self.table_capacity - 1);
        let h2 = (hash >> 57) as u8;
        (h1, h2 & 0x7F)
    }

    /// Finds the slot in the hash table.
    /// Returns (slot_index, true) if found, (slot_index, false) if not found.
    #[inline]
    fn find_slot(&self, key: &K, h1: usize, h2: u8) -> (usize, bool) {
        if self.table_capacity == 0 {
            return (0, false);
        }

        let mut idx = h1;
        let mut step = GROUP_WIDTH;
        let mask = self.table_capacity - 1;
        let mut first_deleted = None;
        let mut probes = 0;

        loop {
            let group_word = unsafe {
                let ptr = self.ctrl.as_ptr().add(idx);
                std::ptr::read_unaligned(ptr as *const u64)
            };

            let match_mask = match_byte(group_word, h2);
            if match_mask != 0 {
                let mut m = match_mask;
                while m != 0 {
                    let trailing = m.trailing_zeros() / 8;
                    let slot_idx = (idx + trailing as usize) & mask;

                    // Check actual key equality
                    unsafe {
                        let dense_idx = *self.slots.get_unchecked(slot_idx);
                        if let Some(k) = self.keys.get(dense_idx) {
                            if *k == *key {
                                return (slot_idx, true);
                            }
                        }
                    }

                    m &= m - 1;
                }
            }

            let empty_mask = match_byte(group_word, EMPTY);
            if empty_mask != 0 {
                let trailing = empty_mask.trailing_zeros() / 8;
                let empty_idx = (idx + trailing as usize) & mask;
                return match first_deleted {
                    Some(d) => (d, false),
                    None => (empty_idx, false),
                };
            }

            if first_deleted.is_none() {
                let deleted_mask = match_byte(group_word, DELETED);
                if deleted_mask != 0 {
                    let trailing = deleted_mask.trailing_zeros() / 8;
                    first_deleted = Some((idx + trailing as usize) & mask);
                }
            }

            idx = (idx + step) & mask;
            step += GROUP_WIDTH;
            probes += 1;

            if probes > self.table_capacity {
                return match first_deleted {
                    Some(d) => (d, false),
                    None => (0, false),
                };
            }
        }
    }

    /// Inserts a key-value pair into the map.
    /// If the map did not have this key present, None is returned.
    /// If the map did have this key present, the value is updated, and the old value is returned.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.table_capacity == 0 || self.items_count >= self.table_capacity * 7 / 8 {
            let new_cap = (self.table_capacity * 2).max(8);
            self.grow(new_cap);
        }

        let (h1, h2) = self.hash(&key);
        let (slot_idx, found) = self.find_slot(&key, h1, h2);

        if found {
            // Update existing value
            unsafe {
                let dense_idx = *self.slots.get_unchecked(slot_idx);
                let cell = self.values.get_unchecked_mut_exclusive(dense_idx);
                let old = std::mem::replace(cell, value);
                Some(old)
            }
        } else {
            // Add new entry
            let dense_idx = self.keys.len();
            self.keys.push(key);
            self.values.push(value);

            unsafe {
                let ctrl_byte = *self.ctrl.get_unchecked(slot_idx);
                let was_deleted = ctrl_byte == DELETED;

                *self.slots.get_unchecked_mut(slot_idx) = dense_idx;
                self.ctrl[slot_idx] = h2;
                if slot_idx < GROUP_WIDTH {
                    self.ctrl[self.table_capacity + slot_idx] = h2;
                }

                if !was_deleted {
                    self.items_count += 1;
                }
            }
            None
        }
    }

    /// Removes a key from the map, returning the value.
    /// Performs a swap_remove on the dense vectors to perform in O(1).
    pub fn swap_remove(&mut self, key: &K) -> Option<V> {
        if self.table_capacity == 0 {
            return None;
        }

        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let dense_idx = *self.slots.get_unchecked(slot_idx);

                // Mark slot as deleted
                self.ctrl[slot_idx] = DELETED;
                if slot_idx < GROUP_WIDTH {
                    self.ctrl[self.table_capacity + slot_idx] = DELETED;
                }
                // items_count stays same (DELETED is still "occupied" for probing)
                // But if we want to be strict, items_count tracks load. DELETED adds to load.

                // Remove from dense vectors
                // swap_remove moves the last element to dense_idx.
                // We need to update the hash table for the moved element.

                let last_idx = self.keys.len() - 1;
                if dense_idx == last_idx {
                    // Simple case: removing the last element
                    self.keys.pop();
                    return self.values.pop();
                }

                // Find the slot for the last element (which will move)
                // We must do this BEFORE swap_remove because find_slot relies on keys[idx] being valid.
                let last_key = self.keys.get_unchecked(last_idx);
                let (mh1, mh2) = self.hash(last_key);
                let (moved_slot_idx, moved_found) = self.find_slot(last_key, mh1, mh2);

                if !moved_found {
                    panic!("BrandedIndexMap inconsistency during swap_remove");
                }

                // Update the slot to point to the new location (dense_idx)
                // We can do this before swap_remove because we already have the index.
                *self.slots.get_unchecked_mut(moved_slot_idx) = dense_idx;

                // Now perform the swap
                self.keys.swap_remove(dense_idx);
                let val = self.values.swap_remove(dense_idx);

                Some(val)
            }
        } else {
            None
        }
    }

    fn grow(&mut self, new_cap: usize) {
        let _old_ctrl = std::mem::take(&mut self.ctrl);
        let _old_slots = std::mem::take(&mut self.slots);
        let _old_table_cap = self.table_capacity;

        self.table_capacity = new_cap;
        if new_cap > 0 {
            let ctrl_len = new_cap + GROUP_WIDTH;
            self.ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
            self.slots = vec![0; new_cap].into_boxed_slice();
        } else {
            self.items_count = 0;
            return;
        }

        self.items_count = 0;

        // Rehash all existing items
        // Since we have dense vectors, we can just iterate them!
        // This is much faster than iterating the hash table.

        for (i, key) in self.keys.iter().enumerate() {
            let (h1, h2) = self.hash(key);
            // We know it's a new table, so find_slot will return an empty slot.
            // Also we don't need to check for duplicates.

            // Simplified insert logic
            let mask = self.table_capacity - 1;
            let mut idx = h1;
            let mut step = GROUP_WIDTH;

            loop {
                let group_word = unsafe {
                    let ptr = self.ctrl.as_ptr().add(idx);
                    std::ptr::read_unaligned(ptr as *const u64)
                };

                let empty_mask = match_byte(group_word, EMPTY);
                if empty_mask != 0 {
                    let trailing = empty_mask.trailing_zeros() / 8;
                    let slot_idx = (idx + trailing as usize) & mask;

                    unsafe {
                        *self.slots.get_unchecked_mut(slot_idx) = i;
                        self.ctrl[slot_idx] = h2;
                        if slot_idx < GROUP_WIDTH {
                            self.ctrl[self.table_capacity + slot_idx] = h2;
                        }
                    }
                    self.items_count += 1;
                    break;
                }

                idx = (idx + step) & mask;
                step += GROUP_WIDTH;
            }
        }
    }

    /// Gets a shared reference to the value associated with the key.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if self.table_capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let dense_idx = *self.slots.get_unchecked(slot_idx);
                self.values.get(token, dense_idx)
            }
        } else {
            None
        }
    }

    /// Gets a mutable reference to the value associated with the key.
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a mut V> {
        if self.table_capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let dense_idx = *self.slots.get_unchecked(slot_idx);
                self.values.get_mut(token, dense_idx)
            }
        } else {
            None
        }
    }
}

impl<'brand, K, V, S> BrandedCollection<'brand> for BrandedIndexMap<'brand, K, V, S> {
    fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn len(&self) -> usize {
        self.keys.len()
    }
}

impl<'brand, K, V, S> ZeroCopyMapOps<'brand, K, V> for BrandedIndexMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<(&'a K, &'a V)>
    where
        F: Fn(&K, &V) -> bool,
    {
        self.iter(token).find(|(k, v)| f(k, v))
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        self.iter(token).any(|(k, v)| f(k, v))
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        self.iter(token).all(|(k, v)| f(k, v))
    }
}

impl<'brand, K, V, S> Default for BrandedIndexMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    fn default() -> Self {
        Self::with_capacity_and_hasher(0, S::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_index_map_basic() {
        GhostToken::new(|mut token| {
            let mut map = BrandedIndexMap::new();
            map.insert("a", 1);
            map.insert("b", 2);

            assert_eq!(map.len(), 2);
            assert_eq!(*map.get(&token, &"a").unwrap(), 1);
            assert_eq!(*map.get(&token, &"b").unwrap(), 2);

            // Test update
            map.insert("a", 10);
            assert_eq!(*map.get(&token, &"a").unwrap(), 10);
            assert_eq!(map.len(), 2);
        });
    }

    #[test]
    fn test_index_map_order() {
        GhostToken::new(|token| {
            let mut map = BrandedIndexMap::new();
            map.insert(1, "one");
            map.insert(2, "two");
            map.insert(3, "three");

            let keys: Vec<_> = map.keys().copied().collect();
            assert_eq!(keys, vec![1, 2, 3]);

            let values: Vec<_> = map.values(&token).copied().collect();
            assert_eq!(values, vec!["one", "two", "three"]);
        });
    }

    #[test]
    fn test_swap_remove() {
        GhostToken::new(|token| {
            let mut map = BrandedIndexMap::new();
            map.insert("a", 1);
            map.insert("b", 2);
            map.insert("c", 3);

            // Remove "b" (middle)
            // Swap remove moves "c" to "b"'s place.
            // Keys: ["a", "b", "c"] -> ["a", "c"]

            assert_eq!(map.swap_remove(&"b"), Some(2));
            assert_eq!(map.len(), 2);
            assert!(!map.get(&token, &"b").is_some());
            assert_eq!(*map.get(&token, &"c").unwrap(), 3);

            // Verify order changed
            let keys: Vec<_> = map.keys().copied().collect();
            assert_eq!(keys, vec!["a", "c"]);

            // Verify index lookup
            assert_eq!(map.get_index(&token, 1), Some((&"c", &3)));
        });
    }
}
