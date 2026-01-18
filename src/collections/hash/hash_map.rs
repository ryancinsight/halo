//! `BrandedHashMap` — a high-performance hash map with token-gated values.
//!
//! This implementation uses a **SwissTable-inspired** layout (Structure of Arrays)
//! to maximize cache locality and SIMD usage.
//!
//! Key features:
//! - **Control Bytes**: Metadata is stored in a dense byte array (1 byte per slot).
//!   - `0..=127`: Occupied (stores lower 7 bits of hash).
//!   - `255 (0xFF)`: Empty.
//!   - `254 (0xFE)`: Deleted (Tombstone).
//! - **Structure of Arrays (SoA)**: `keys` and `values` are stored in separate arrays.
//!   - This improves cache usage when searching (only `ctrl` is accessed).
//!   - Values are only accessed when a key matches.
//! - **SWAR (SIMD Within A Register)**: Uses `u64` operations to check 8 slots in parallel.
//! - **GhostToken Gating**: Values are wrapped in `GhostCell`, ensuring zero-cost safety.

use core::hash::{Hash, Hasher, BuildHasher};
use core::mem::MaybeUninit;
use std::collections::hash_map::RandomState;
use crate::{GhostCell, GhostToken};
use std::alloc::{self, Layout};
use std::ptr::NonNull;
use std::marker::PhantomData;

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

/// Helper to allocate a slice of MaybeUninit without initializing it.
fn alloc_slice<T>(len: usize) -> Box<[MaybeUninit<T>]> {
    if len == 0 || core::mem::size_of::<T>() == 0 {
        // Handle ZSTs and empty allocations
        let ptr = NonNull::<MaybeUninit<T>>::dangling().as_ptr();
        unsafe {
            let slice = std::slice::from_raw_parts_mut(ptr, len);
            Box::from_raw(slice)
        }
    } else {
        let layout = Layout::array::<MaybeUninit<T>>(len).unwrap();
        unsafe {
            let ptr = alloc::alloc(layout) as *mut MaybeUninit<T>;
            if ptr.is_null() {
                alloc::handle_alloc_error(layout);
            }
            let slice = std::slice::from_raw_parts_mut(ptr, len);
            Box::from_raw(slice)
        }
    }
}

/// High-performance hash map with SwissTable-like layout.
pub struct BrandedHashMap<'brand, K, V, S = RandomState> {
    /// Control bytes: 0xFF=Empty, 0xFE=Deleted, 0..127=H2
    /// Size is capacity + GROUP_WIDTH for mirror bytes optimization.
    ctrl: Box<[u8]>,
    /// Keys array (size = capacity)
    keys: Box<[MaybeUninit<K>]>,
    /// Values array (GhostCells) (size = capacity)
    values: Box<[MaybeUninit<GhostCell<'brand, V>>]>,
    /// Number of occupied slots (including DELETED for probe chain continuity)
    items_count: usize,
    /// Number of actual elements
    len: usize,
    /// Total capacity (power of 2)
    capacity: usize,
    /// Hash builder
    hash_builder: S,
}

impl<'brand, K, V> BrandedHashMap<'brand, K, V, RandomState>
where
    K: Eq + Hash,
{
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

impl<'brand, K, V, S> BrandedHashMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Creates an empty map with capacity and hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let capacity = if capacity == 0 {
            0
        } else {
            capacity.next_power_of_two().max(8)
        };

        if capacity == 0 {
            return Self {
                ctrl: Box::new([]),
                keys: Box::new([]),
                values: Box::new([]),
                items_count: 0,
                len: 0,
                capacity: 0,
                hash_builder,
            };
        }

        // Initialize ctrl with EMPTY, including padding bytes
        let ctrl_len = capacity + GROUP_WIDTH;
        let ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();

        let keys = alloc_slice(capacity);
        let values = alloc_slice(capacity);

        Self {
            ctrl,
            keys,
            values,
            items_count: 0,
            len: 0,
            capacity,
            hash_builder,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    fn hash(&self, key: &K) -> (usize, u8) {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        // Bottom bits for H1 (index) since capacity is power of 2
        let h1 = (hash as usize) & (self.capacity - 1);
        // Top 7 bits for H2 (tag) to ensure independence from H1
        // (hash >> 57) for 64-bit hash
        let h2 = (hash >> 57) as u8;
        // Ensure H2 is in 0..128 range (top bit 0)
        (h1, h2 & 0x7F)
    }

    /// Finds the slot for a key. Returns (index, true) if found, (index, false) if not found.
    /// If not found, index is the insertion slot (first Empty or Deleted).
    #[inline]
    fn find_slot(&self, key: &K, h1: usize, h2: u8) -> (usize, bool) {
        if self.capacity == 0 {
            return (0, false);
        }

        let mut idx = h1;
        let mut step = GROUP_WIDTH;
        let mask = self.capacity - 1;

        // Use SWAR group probing
        // Loop guarantees termination because capacity check ensures at least one EMPTY slot
        // if load factor is managed correctly. But for robustness against deleted slots,
        // we track the first deleted slot.

        let mut first_deleted = None;
        let mut probes = 0;

        loop {
            // Unsafe load of 8 bytes
            // Safe because ctrl is capacity + 8, and idx < capacity.
            // When idx wraps, we start from 0.
            // Wait, standard SwissTable loads from idx and masks?
            // Yes, because of mirror bytes, loading at idx covers the wrap around case
            // if idx is near the end.

            let group_word = unsafe {
                let ptr = self.ctrl.as_ptr().add(idx);
                std::ptr::read_unaligned(ptr as *const u64)
            };

            // Check for match
            let match_mask = match_byte(group_word, h2);
            if match_mask != 0 {
                // Iterate over matches
                let mut m = match_mask;
                while m != 0 {
                    let trailing = m.trailing_zeros() / 8;
                    let slot_idx = (idx + trailing as usize) & mask;

                    unsafe {
                         let k = self.keys.get_unchecked(slot_idx).assume_init_ref();
                         if *k == *key {
                             return (slot_idx, true);
                         }
                    }

                    // Clear lowest set bit
                    m &= m - 1;
                }
            }

            // Check for empty
            let empty_mask = match_byte(group_word, EMPTY);
            if empty_mask != 0 {
                // Found empty slot.
                // We return the first deleted slot we found previously, or this empty slot.
                // The empty slot index needs to be calculated.
                let trailing = empty_mask.trailing_zeros() / 8;
                let empty_idx = (idx + trailing as usize) & mask;

                return match first_deleted {
                    Some(d) => (d, false),
                    None => (empty_idx, false),
                };
            }

            // Check for deleted slots to remember the first one
            if first_deleted.is_none() {
                 let deleted_mask = match_byte(group_word, DELETED);
                 if deleted_mask != 0 {
                     let trailing = deleted_mask.trailing_zeros() / 8;
                     first_deleted = Some((idx + trailing as usize) & mask);
                 }
            }

            // Quadratic probing
            idx = (idx + step) & mask;
            step += GROUP_WIDTH;
            probes += 1;

            // Safety break for full table (should not happen if grown correctly)
            if probes > self.capacity {
                 // If we probed the whole table and found nothing, return failure or first deleted.
                 return match first_deleted {
                     Some(d) => (d, false),
                     None => (0, false), // Should panic or be handled, but (0, false) signals insert at 0 if unchecked
                 };
            }
        }
    }

    #[inline]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if self.capacity == 0 { return None; }
        let (h1, h2) = self.hash(key);
        let (idx, found) = self.find_slot(key, h1, h2);
        if found {
            unsafe {
                Some(self.values.get_unchecked(idx).assume_init_ref().borrow(token))
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a mut V> {
        if self.capacity == 0 { return None; }
        let (h1, h2) = self.hash(key);
        let (idx, found) = self.find_slot(key, h1, h2);
        if found {
            unsafe {
                Some(self.values.get_unchecked(idx).assume_init_ref().borrow_mut(token))
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &K) -> bool {
        if self.capacity == 0 { return false; }
        let (h1, h2) = self.hash(key);
        self.find_slot(key, h1, h2).1
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.capacity == 0 || self.items_count >= self.capacity * 7 / 8 { // Load factor 0.875
            let new_cap = (self.capacity * 2).max(8);
            self.grow(new_cap);
        }

        let (h1, h2) = self.hash(&key);
        let (idx, found) = self.find_slot(&key, h1, h2);

        // Safety check: if table is somehow full and we got an invalid index, we must not overwrite.
        // find_slot guarantees returning a valid empty/deleted slot if !found, assuming table not full.
        // Grow above guarantees space.

        if found {
            unsafe {
                let cell = self.values.get_unchecked_mut(idx).assume_init_mut();
                let old_cell = std::mem::replace(cell, GhostCell::new(value));
                Some(old_cell.into_inner())
            }
        } else {
            // Insert
            unsafe {
                let ctrl_byte = self.ctrl.get_unchecked(idx);
                let was_deleted = *ctrl_byte == DELETED;

                self.keys.get_unchecked_mut(idx).write(key);
                self.values.get_unchecked_mut(idx).write(GhostCell::new(value));
                self.ctrl[idx] = h2;
                if idx < GROUP_WIDTH {
                    self.ctrl[self.capacity + idx] = h2;
                }

                if !was_deleted {
                    self.items_count += 1;
                }
                self.len += 1;
            }
            None
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.capacity == 0 { return None; }
        let (h1, h2) = self.hash(key);
        let (idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                // Mark as deleted
                self.ctrl[idx] = DELETED;
                if idx < GROUP_WIDTH {
                    self.ctrl[self.capacity + idx] = DELETED;
                }

                self.len -= 1;

                let key_ptr = self.keys.get_unchecked_mut(idx).as_mut_ptr();
                let val_ptr = self.values.get_unchecked_mut(idx).as_mut_ptr();

                std::ptr::drop_in_place(key_ptr);
                let val = std::ptr::read(val_ptr);

                Some(val.into_inner())
            }
        } else {
            None
        }
    }

    fn grow(&mut self, new_cap: usize) {
        let old_ctrl = std::mem::take(&mut self.ctrl);
        let old_keys = std::mem::take(&mut self.keys);
        let old_values = std::mem::take(&mut self.values);
        let old_cap = self.capacity;

        // Allocate new arrays
        self.capacity = new_cap;
        if new_cap > 0 {
            let ctrl_len = new_cap + GROUP_WIDTH;
            self.ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
            self.keys = alloc_slice(new_cap);
            self.values = alloc_slice(new_cap);
        } else {
            self.items_count = 0;
            self.len = 0;
            return;
        }

        self.len = 0;
        self.items_count = 0;

        // Rehash
        for i in 0..old_cap {
            if old_ctrl[i] & 0x80 == 0 { // Is occupied (0..127)
                unsafe {
                    let key = old_keys.get_unchecked(i).assume_init_read();
                    let val = old_values.get_unchecked(i).assume_init_read();

                    self.insert_internal_during_grow(key, val);
                }
            }
        }
    }

    fn insert_internal_during_grow(&mut self, key: K, value: GhostCell<'brand, V>) {
        let (h1, h2) = self.hash(&key);

        let mask = self.capacity - 1;
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
                 let empty_idx = (idx + trailing as usize) & mask;

                 unsafe {
                    self.keys.get_unchecked_mut(empty_idx).write(key);
                    self.values.get_unchecked_mut(empty_idx).write(value);
                    self.ctrl[empty_idx] = h2;
                    if empty_idx < GROUP_WIDTH {
                        self.ctrl[self.capacity + empty_idx] = h2;
                    }
                    self.items_count += 1;
                    self.len += 1;
                }
                return;
            }

            idx = (idx + step) & mask;
            step += GROUP_WIDTH;
        }
    }

    pub fn clear(&mut self) {
        if self.len == 0 { return; }
        for i in 0..self.capacity {
             if self.ctrl[i] & 0x80 == 0 {
                 unsafe {
                     self.keys.get_unchecked_mut(i).assume_init_drop();
                     self.values.get_unchecked_mut(i).assume_init_drop();
                 }
             }
             self.ctrl[i] = EMPTY;
        }
        // Restore padding
        for i in 0..GROUP_WIDTH {
             if i < self.ctrl.len() && self.capacity > 0 {
                  self.ctrl[self.capacity + i] = EMPTY;
             }
        }

        self.len = 0;
        self.items_count = 0;
    }

    pub fn reserve(&mut self, additional: usize) {
        let needed = self.len + additional;
        if needed > self.capacity * 7 / 8 {
             let new_cap = (needed * 8 / 7).next_power_of_two().max(8);
             if new_cap > self.capacity {
                 self.grow(new_cap);
             }
        }
    }

    // --- Iterators ---

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        // Iterate only up to capacity, ignore padding
        self.ctrl[0..self.capacity].iter().enumerate().filter_map(move |(i, &c)| {
            if c & 0x80 == 0 {
                unsafe { Some(self.keys.get_unchecked(i).assume_init_ref()) }
            } else {
                None
            }
        })
    }

    pub fn values<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a V> {
        self.ctrl[0..self.capacity].iter().enumerate().filter_map(move |(i, &c)| {
            if c & 0x80 == 0 {
                unsafe {
                    Some(self.values.get_unchecked(i).assume_init_ref().borrow(token))
                }
            } else {
                None
            }
        })
    }

    /// Applies `f` to all entries in the map, allowing mutation of values.
    pub fn for_each_mut<F>(&self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&K, &mut V),
    {
        for i in 0..self.capacity {
            if self.ctrl[i] & 0x80 == 0 {
                unsafe {
                    let k = self.keys.get_unchecked(i).assume_init_ref();
                    let v = self.values.get_unchecked(i).assume_init_ref().borrow_mut(token);
                    f(k, v);
                }
            }
        }
    }
}

impl<'brand, K, V, S> Drop for BrandedHashMap<'brand, K, V, S> {
    fn drop(&mut self) {
        if self.capacity > 0 {
            for i in 0..self.capacity {
                if self.ctrl[i] & 0x80 == 0 {
                    unsafe {
                        self.keys.get_unchecked_mut(i).assume_init_drop();
                        self.values.get_unchecked_mut(i).assume_init_drop();
                    }
                }
            }
        }
    }
}

impl<'brand, K, V, S> Default for BrandedHashMap<'brand, K, V, S>
where K: Eq + Hash, S: BuildHasher + Default
{
    fn default() -> Self {
        Self::with_capacity_and_hasher(0, S::default())
    }
}

// Implement ZeroCopyOps
use crate::collections::{BrandedCollection, ZeroCopyMapOps};

impl<'brand, K, V, S> BrandedCollection<'brand> for BrandedHashMap<'brand, K, V, S> {
    fn len(&self) -> usize { self.len }
    fn is_empty(&self) -> bool { self.len == 0 }
}

impl<'brand, K, V, S> ZeroCopyMapOps<'brand, K, V> for BrandedHashMap<'brand, K, V, S>
where K: Eq + Hash, S: BuildHasher
{
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<(&'a K, &'a V)>
    where F: Fn(&K, &V) -> bool
    {
        for i in 0..self.capacity {
            if self.ctrl[i] & 0x80 == 0 {
                unsafe {
                    let k = self.keys.get_unchecked(i).assume_init_ref();
                    let v = self.values.get_unchecked(i).assume_init_ref().borrow(token);
                    if f(k, v) {
                        return Some((k, v));
                    }
                }
            }
        }
        None
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where F: Fn(&K, &V) -> bool
    {
         self.find_ref(token, f).is_some()
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where F: Fn(&K, &V) -> bool
    {
        if self.is_empty() { return true; }
        for i in 0..self.capacity {
            if self.ctrl[i] & 0x80 == 0 {
                unsafe {
                     let k = self.keys.get_unchecked(i).assume_init_ref();
                     let v = self.values.get_unchecked(i).assume_init_ref().borrow(token);
                     if !f(k, v) {
                         return false;
                     }
                }
            }
        }
        true
    }
}

// IntoIterator
pub struct IntoIter<'brand, K, V> {
    ctrl: Box<[u8]>,
    keys: Box<[MaybeUninit<K>]>,
    values: Box<[MaybeUninit<GhostCell<'brand, V>>]>,
    index: usize,
    len: usize,
    capacity: usize, // Needed for iteration limit
}

impl<'brand, K, V> Iterator for IntoIter<'brand, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.capacity { // Only up to capacity
            let i = self.index;
            self.index += 1;
            if self.ctrl[i] & 0x80 == 0 {
                unsafe {
                    let k = self.keys.get_unchecked_mut(i).assume_init_read();
                    let v = self.values.get_unchecked_mut(i).assume_init_read();
                    self.len -= 1;
                    return Some((k, v.into_inner()));
                }
            }
        }
        None
    }
}

impl<'brand, K, V> Drop for IntoIter<'brand, K, V> {
    fn drop(&mut self) {
        while self.index < self.capacity {
             let i = self.index;
             self.index += 1;
             if self.ctrl[i] & 0x80 == 0 {
                 unsafe {
                     self.keys.get_unchecked_mut(i).assume_init_drop();
                     self.values.get_unchecked_mut(i).assume_init_drop();
                 }
             }
        }
    }
}

impl<'brand, K, V, S> IntoIterator for BrandedHashMap<'brand, K, V, S> {
    type Item = (K, V);
    type IntoIter = IntoIter<'brand, K, V>;

    fn into_iter(mut self) -> Self::IntoIter {
        let ctrl = std::mem::take(&mut self.ctrl);
        let keys = std::mem::take(&mut self.keys);
        let values = std::mem::take(&mut self.values);
        let len = self.len;
        let capacity = self.capacity;

        // Prevent Drop from doing anything
        self.capacity = 0;
        self.len = 0;

        IntoIter {
            ctrl, keys, values, index: 0, len, capacity
        }
    }
}

impl<'brand, K, V, S> FromIterator<(K, V)> for BrandedHashMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let mut map = Self::with_capacity_and_hasher(lower, S::default());
        map.extend(iter);
        map
    }
}

impl<'brand, K, V, S> Extend<(K, V)> for BrandedHashMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        iter.into_iter().for_each(|(k, v)| {
            self.insert(k, v);
        });
    }
}

    /// Tests for zero-copy operations and advanced features.
    #[cfg(test)]
    mod zero_copy_tests {
        use super::*;
        use crate::GhostToken;

        #[test]
        fn test_zero_copy_map_operations() {
            GhostToken::new(|token| {
                let mut map = BrandedHashMap::new();
                map.insert("key1", 1);
                map.insert("key2", 2);
                map.insert("key3", 3);

                // Test find_ref
                let found = map.find_ref(&token, |k, v| *k == "key2" && *v == 2);
                assert_eq!(found, Some((&"key2", &2)));

                let not_found = map.find_ref(&token, |k, _| *k == "nonexistent");
                assert_eq!(not_found, None);

                // Test any_ref
                assert!(map.any_ref(&token, |_, v| *v > 2));
                assert!(!map.any_ref(&token, |_, v| *v > 10));

                // Test all_ref
                assert!(map.all_ref(&token, |_, v| *v > 0));
                assert!(!map.all_ref(&token, |_, v| *v > 1));
            });
        }

        #[test]
        fn test_zero_copy_map_empty() {
            GhostToken::new(|token| {
                let map: BrandedHashMap<&str, i32> = BrandedHashMap::new();

                assert_eq!(map.find_ref(&token, |_, _| true), None);
                assert!(!map.any_ref(&token, |_, _| true));
                assert!(map.all_ref(&token, |_, _| false)); // vacuously true
            });
        }

        #[test]
        fn test_zero_copy_map_single_entry() {
            GhostToken::new(|token| {
                let mut map = BrandedHashMap::new();
                map.insert("single", 42);

                assert_eq!(map.find_ref(&token, |k, v| *k == "single" && *v == 42), Some((&"single", &42)));
                assert!(map.any_ref(&token, |k, v| *k == "single" && *v == 42));
                assert!(map.all_ref(&token, |k, v| *k == "single" && *v == 42));
            });
        }

        #[test]
        fn test_zero_copy_map_collision_handling() {
            GhostToken::new(|token| {
                let mut map = BrandedHashMap::new();

                // Insert entries that might collide
                map.insert("key1", 1);
                map.insert("key2", 2);
                map.insert("key3", 3);

                // All should be findable despite potential collisions
                assert!(map.find_ref(&token, |k, v| *k == "key1" && *v == 1).is_some());
                assert!(map.find_ref(&token, |k, v| *k == "key2" && *v == 2).is_some());
                assert!(map.find_ref(&token, |k, v| *k == "key3" && *v == 3).is_some());

                // Test removal and tombstone handling
                assert_eq!(map.remove(&"key2"), Some(2));
                assert_eq!(map.find_ref(&token, |k, _| *k == "key2"), None);

                // Other entries should still be accessible
                assert!(map.find_ref(&token, |k, v| *k == "key1" && *v == 1).is_some());
                assert!(map.find_ref(&token, |k, v| *k == "key3" && *v == 3).is_some());
            });
        }
    }

    #[cfg(test)]
    mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn branded_hash_map_basic() {
        GhostToken::new(|mut token| {
            let mut map = BrandedHashMap::new();
            map.insert("a", 1);
            map.insert("b", 2);

            assert_eq!(map.len(), 2);
            assert_eq!(*map.get(&token, &"a").unwrap(), 1);
            assert_eq!(*map.get(&token, &"b").unwrap(), 2);

            *map.get_mut(&mut token, &"a").unwrap() += 10;
            assert_eq!(*map.get(&token, &"a").unwrap(), 11);
        });
    }

    #[test]
    fn branded_hash_map_remove() {
        GhostToken::new(|token| {
            let mut map = BrandedHashMap::new();

            // Insert some elements
            map.insert("a", 1);
            map.insert("b", 2);
            map.insert("c", 3);
            assert_eq!(map.len(), 3);

            // Remove existing element
            let removed = map.remove(&"b");
            assert_eq!(removed, Some(2));
            assert_eq!(map.len(), 2);
            assert!(!map.contains_key(&"b"));
            assert_eq!(*map.get(&token, &"a").unwrap(), 1);
            assert_eq!(*map.get(&token, &"c").unwrap(), 3);

            // Remove non-existing element
            let removed = map.remove(&"d");
            assert_eq!(removed, None);
            assert_eq!(map.len(), 2);

            // Remove remaining elements
            let removed = map.remove(&"a");
            assert_eq!(removed, Some(1));
            assert_eq!(map.len(), 1);

            let removed = map.remove(&"c");
            assert_eq!(removed, Some(3));
            assert_eq!(map.len(), 0);
            assert!(map.is_empty());
        });
    }

    #[test]
    fn branded_hash_map_interior_mutability() {
        GhostToken::new(|mut token| {
            let mut map = BrandedHashMap::new();
            map.insert("a", 1);

            // Shared borrow of map
            let map_ref = &map;

            // Mutable borrow of value via token
            if let Some(val) = map_ref.get_mut(&mut token, &"a") {
                *val += 100;
            }

            assert_eq!(*map.get(&token, &"a").unwrap(), 101);
        });
    }
}
