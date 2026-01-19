//! `BrandedLinkedHashMap` — an insertion-ordered hash map with O(1) LRU operations.
//!
//! This implementation combines a **SwissTable-inspired** hash table for lookups with
//! a **doubly-linked list** embedded in SoA (Structure of Arrays) layout.
//!
//! Features:
//! - **Order Preservation**: Iteration order matches insertion order.
//! - **LRU/MRU Support**: `move_to_front` and `move_to_back` in O(1).
//! - **Stable Indices**: Uses a free list for storage, so indices are stable (unlike `swap_remove`).
//! - **SoA Layout**: Separate arrays for keys, values, prev, next, to improve cache locality for lookups.

use crate::collections::{BrandedCollection, ZeroCopyMapOps};
use crate::{GhostCell, GhostToken};
use core::hash::{BuildHasher, Hash, Hasher};
use core::mem::MaybeUninit;
use std::alloc::{self, Layout};
use std::collections::hash_map::RandomState;
use std::ptr::NonNull;

// Control byte constants
const EMPTY: u8 = 0xFF;
const DELETED: u8 = 0xFE;
const GROUP_WIDTH: usize = 8;
const END_OF_LIST: usize = usize::MAX;

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

pub struct BrandedLinkedHashMap<'brand, K, V, S = RandomState> {
    ctrl: Box<[u8]>,
    slots: Box<[usize]>, // Hash table: maps hash slot -> storage index

    // Storage arrays (SoA)
    keys: Box<[MaybeUninit<K>]>,
    values: Box<[MaybeUninit<GhostCell<'brand, V>>]>,
    prev: Box<[usize]>,
    next: Box<[usize]>,

    head: usize,
    tail: usize,
    free_head: usize, // Head of the free list (using `next` array)

    capacity: usize,
    items_count: usize, // occupied + deleted in hash table
    len: usize,         // actual elements

    hash_builder: S,
}

impl<'brand, K, V, S> BrandedLinkedHashMap<'brand, K, V, S> {
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let capacity = if capacity == 0 {
            0
        } else {
            (capacity * 8 / 7).next_power_of_two().max(8)
        };

        if capacity == 0 {
            return Self {
                ctrl: Box::new([]),
                slots: Box::new([]),
                keys: Box::new([]),
                values: Box::new([]),
                prev: Box::new([]),
                next: Box::new([]),
                head: END_OF_LIST,
                tail: END_OF_LIST,
                free_head: END_OF_LIST,
                capacity: 0,
                items_count: 0,
                len: 0,
                hash_builder,
            };
        }

        let ctrl_len = capacity + GROUP_WIDTH;
        let ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
        let slots = vec![0; capacity].into_boxed_slice(); // 0 is valid index, but we check ctrl

        let keys = alloc_slice(capacity);
        let values = alloc_slice(capacity);
        let prev = vec![END_OF_LIST; capacity].into_boxed_slice();
        let mut next = vec![END_OF_LIST; capacity].into_boxed_slice();

        // Initialize free list
        for i in 0..capacity - 1 {
            next[i] = i + 1;
        }
        next[capacity - 1] = END_OF_LIST;
        let free_head = 0;

        Self {
            ctrl,
            slots,
            keys,
            values,
            prev,
            next,
            head: END_OF_LIST,
            tail: END_OF_LIST,
            free_head,
            capacity,
            items_count: 0,
            len: 0,
            hash_builder,
        }
    }
}

impl<'brand, K, V> BrandedLinkedHashMap<'brand, K, V, RandomState> {
    pub fn new() -> Self {
        Self::with_capacity_and_hasher(0, RandomState::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<'brand, K, V, S> BrandedLinkedHashMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    #[inline]
    fn hash(&self, key: &K) -> (usize, u8) {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let h1 = (hash as usize) & (self.capacity - 1);
        let h2 = (hash >> 57) as u8;
        (h1, h2 & 0x7F)
    }

    #[inline]
    fn find_slot(&self, key: &K, h1: usize, h2: u8) -> (usize, bool) {
        if self.capacity == 0 {
            return (0, false);
        }

        let mut idx = h1;
        let mut step = GROUP_WIDTH;
        let mask = self.capacity - 1;
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

                    unsafe {
                        let storage_idx = *self.slots.get_unchecked(slot_idx);
                        let k = self.keys.get_unchecked(storage_idx).assume_init_ref();
                        if *k == *key {
                            return (slot_idx, true);
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

            if probes > self.capacity {
                return match first_deleted {
                    Some(d) => (d, false),
                    None => (0, false),
                };
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.capacity == 0
            || self.items_count >= self.capacity * 7 / 8
            || self.len == self.capacity
        {
            let new_cap = (self.capacity * 2).max(8);
            self.grow(new_cap);
        }

        let (h1, h2) = self.hash(&key);
        let (slot_idx, found) = self.find_slot(&key, h1, h2);

        if found {
            unsafe {
                let storage_idx = *self.slots.get_unchecked(slot_idx);
                let cell = self.values.get_unchecked_mut(storage_idx).assume_init_mut();
                let old = std::mem::replace(cell, GhostCell::new(value));
                Some(old.into_inner())
            }
        } else {
            // Allocate new slot from free list
            let storage_idx = self.free_head;
            debug_assert!(storage_idx != END_OF_LIST, "No free slots despite check");
            self.free_head = self.next[storage_idx];

            unsafe {
                self.keys.get_unchecked_mut(storage_idx).write(key);
                self.values
                    .get_unchecked_mut(storage_idx)
                    .write(GhostCell::new(value));

                let ctrl_byte = *self.ctrl.get_unchecked(slot_idx);
                let was_deleted = ctrl_byte == DELETED;

                *self.slots.get_unchecked_mut(slot_idx) = storage_idx;
                self.ctrl[slot_idx] = h2;
                if slot_idx < GROUP_WIDTH {
                    self.ctrl[self.capacity + slot_idx] = h2;
                }

                if !was_deleted {
                    self.items_count += 1;
                }
            }

            self.link_to_tail(storage_idx);
            self.len += 1;
            None
        }
    }

    fn link_to_tail(&mut self, idx: usize) {
        if self.tail == END_OF_LIST {
            self.head = idx;
            self.tail = idx;
            self.prev[idx] = END_OF_LIST;
            self.next[idx] = END_OF_LIST;
        } else {
            self.next[self.tail] = idx;
            self.prev[idx] = self.tail;
            self.next[idx] = END_OF_LIST;
            self.tail = idx;
        }
    }

    fn unlink(&mut self, idx: usize) {
        let prev = self.prev[idx];
        let next = self.next[idx];

        if prev != END_OF_LIST {
            self.next[prev] = next;
        } else {
            self.head = next;
        }

        if next != END_OF_LIST {
            self.prev[next] = prev;
        } else {
            self.tail = prev;
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if self.capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let storage_idx = *self.slots.get_unchecked(slot_idx);

                // Mark hash table slot as deleted
                self.ctrl[slot_idx] = DELETED;
                if slot_idx < GROUP_WIDTH {
                    self.ctrl[self.capacity + slot_idx] = DELETED;
                }

                self.unlink(storage_idx);

                // Return to free list
                self.next[storage_idx] = self.free_head;
                self.free_head = storage_idx;
                self.len -= 1;

                // Drop key, move out value
                self.keys.get_unchecked_mut(storage_idx).assume_init_drop();
                let val_cell = self
                    .values
                    .get_unchecked_mut(storage_idx)
                    .assume_init_read();

                Some(val_cell.into_inner())
            }
        } else {
            None
        }
    }

    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, key: &K) -> Option<&'a V> {
        if self.capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let storage_idx = *self.slots.get_unchecked(slot_idx);
                Some(
                    self.values
                        .get_unchecked(storage_idx)
                        .assume_init_ref()
                        .borrow(token),
                )
            }
        } else {
            None
        }
    }

    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a mut V> {
        if self.capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let storage_idx = *self.slots.get_unchecked(slot_idx);
                Some(
                    self.values
                        .get_unchecked(storage_idx)
                        .assume_init_ref()
                        .borrow_mut(token),
                )
            }
        } else {
            None
        }
    }

    /// Moves the element associated with `key` to the front of the list (LRU -> MRU usually implies back, but depends on definition).
    /// Here we move to tail (MRU).
    pub fn move_to_back(&mut self, key: &K) {
        if self.capacity == 0 {
            return;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let storage_idx = *self.slots.get_unchecked(slot_idx);
                if storage_idx != self.tail {
                    self.unlink(storage_idx);
                    self.link_to_tail(storage_idx);
                }
            }
        }
    }

    /// Moves the element to the front (LRU position?).
    pub fn move_to_front(&mut self, key: &K) {
        if self.capacity == 0 {
            return;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2);

        if found {
            unsafe {
                let idx = *self.slots.get_unchecked(slot_idx);
                if idx != self.head {
                    self.unlink(idx);
                    // Link to head
                    if self.head == END_OF_LIST {
                        self.head = idx;
                        self.tail = idx;
                        self.prev[idx] = END_OF_LIST;
                        self.next[idx] = END_OF_LIST;
                    } else {
                        self.prev[self.head] = idx;
                        self.next[idx] = self.head;
                        self.prev[idx] = END_OF_LIST;
                        self.head = idx;
                    }
                }
            }
        }
    }

    /// Removes the first element (head), which corresponds to LRU if we insert at tail.
    pub fn pop_front(&mut self) -> Option<(K, V)> {
        if self.head == END_OF_LIST {
            return None;
        }

        let idx = self.head;
        unsafe {
            // We need to find the key to remove from map!
            // But we have the storage index. We need the map slot index to mark as DELETED.
            // But `find_slot` needs the key.
            // We have the key in `keys[idx]`.
            let key_ref = self.keys.get_unchecked(idx).assume_init_ref();

            // We can now find the slot.
            let (h1, h2) = self.hash(key_ref);
            let (slot_idx, found) = self.find_slot(key_ref, h1, h2);

            debug_assert!(found, "Head element not found in hash table");

            // Now remove
            self.ctrl[slot_idx] = DELETED;
            if slot_idx < GROUP_WIDTH {
                self.ctrl[self.capacity + slot_idx] = DELETED;
            }

            self.unlink(idx);

            // Return to free list
            self.next[idx] = self.free_head;
            self.free_head = idx;
            self.len -= 1;

            let key = self.keys.get_unchecked_mut(idx).assume_init_read();
            let val = self
                .values
                .get_unchecked_mut(idx)
                .assume_init_read()
                .into_inner();

            Some((key, val))
        }
    }

    fn grow(&mut self, new_cap: usize) {
        let old_keys = std::mem::take(&mut self.keys);
        let old_values = std::mem::take(&mut self.values);
        let _old_prev = std::mem::take(&mut self.prev);
        let old_next = std::mem::take(&mut self.next);
        let old_head = self.head;

        // Re-initialize with new capacity
        let ctrl_len = new_cap + GROUP_WIDTH;
        self.ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
        self.slots = vec![0; new_cap].into_boxed_slice();
        self.keys = alloc_slice(new_cap);
        self.values = alloc_slice(new_cap);
        self.prev = vec![END_OF_LIST; new_cap].into_boxed_slice();
        self.next = vec![END_OF_LIST; new_cap].into_boxed_slice();

        // Initialize free list
        for i in 0..new_cap - 1 {
            self.next[i] = i + 1;
        }
        self.next[new_cap - 1] = END_OF_LIST;
        self.free_head = 0;

        self.head = END_OF_LIST;
        self.tail = END_OF_LIST;
        self.items_count = 0;
        self.len = 0;
        self.capacity = new_cap;

        // Iterate old list and re-insert
        // Using the list allows us to preserve order efficiently
        let mut curr = old_head;
        while curr != END_OF_LIST {
            unsafe {
                let key = old_keys.get_unchecked(curr).assume_init_read();
                let val = old_values
                    .get_unchecked(curr)
                    .assume_init_read()
                    .into_inner();
                // We must use `insert` to populate hash table correctly
                // This is O(N)
                self.insert(key, val);

                curr = old_next[curr];
            }
        }

        // Clean up old arrays (done by Drop, but keys/values moved out)
        // We moved everything out, so we don't need to drop elements in old_keys/values.
        // We just need to deallocate.
        // `alloc_slice` returns `Box`, which handles deallocation.
        // But `assume_init_read` moves values.
        // We need to make sure we don't double drop.
        // `Box<[MaybeUninit<T>]>` won't drop elements, only memory. Correct.
    }

    // Iterators

    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = (&'a K, &'a V)> + use<'a, 'brand, K, V, S> {
        Iter {
            map: self,
            token,
            curr: self.head,
        }
    }
}

pub struct Iter<'a, 'brand, K, V, S> {
    map: &'a BrandedLinkedHashMap<'brand, K, V, S>,
    token: &'a GhostToken<'brand>,
    curr: usize,
}

impl<'a, 'brand, K, V, S> Iterator for Iter<'a, 'brand, K, V, S> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr == END_OF_LIST {
            return None;
        }
        unsafe {
            let key = self.map.keys.get_unchecked(self.curr).assume_init_ref();
            let val = self
                .map
                .values
                .get_unchecked(self.curr)
                .assume_init_ref()
                .borrow(self.token);
            self.curr = self.map.next[self.curr];
            Some((key, val))
        }
    }
}

impl<'brand, K, V, S> BrandedCollection<'brand> for BrandedLinkedHashMap<'brand, K, V, S> {
    fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn len(&self) -> usize {
        self.len
    }
}

impl<'brand, K, V, S> ZeroCopyMapOps<'brand, K, V> for BrandedLinkedHashMap<'brand, K, V, S>
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

impl<'brand, K, V, S> Drop for BrandedLinkedHashMap<'brand, K, V, S> {
    fn drop(&mut self) {
        let mut curr = self.head;
        while curr != END_OF_LIST {
            unsafe {
                self.keys.get_unchecked_mut(curr).assume_init_drop();
                self.values.get_unchecked_mut(curr).assume_init_drop();
                curr = self.next[curr];
            }
        }
    }
}

impl<'brand, K, V, S> Default for BrandedLinkedHashMap<'brand, K, V, S>
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
    fn test_linked_map_basic() {
        GhostToken::new(|mut token| {
            let mut map = BrandedLinkedHashMap::new();
            map.insert("a", 1);
            map.insert("b", 2);

            assert_eq!(map.len(), 2);
            assert_eq!(*map.get(&token, &"a").unwrap(), 1);
            assert_eq!(*map.get(&token, &"b").unwrap(), 2);
        });
    }

    #[test]
    fn test_linked_map_order() {
        GhostToken::new(|token| {
            let mut map = BrandedLinkedHashMap::new();
            map.insert(1, "one");
            map.insert(2, "two");
            map.insert(3, "three");

            let vec: Vec<_> = map.iter(&token).map(|(k, v)| (*k, *v)).collect();
            assert_eq!(vec, vec![(1, "one"), (2, "two"), (3, "three")]);
        });
    }

    #[test]
    fn test_lru_operations() {
        GhostToken::new(|mut token| {
            let mut map = BrandedLinkedHashMap::new();
            map.insert("a", 1);
            map.insert("b", 2);
            map.insert("c", 3);

            // Order: a, b, c
            map.move_to_back(&"a");
            // Order: b, c, a

            let vec: Vec<_> = map.iter(&token).map(|(k, _)| *k).collect();
            assert_eq!(vec, vec!["b", "c", "a"]);

            map.move_to_front(&"c");
            // Order: c, b, a
            let vec: Vec<_> = map.iter(&token).map(|(k, _)| *k).collect();
            assert_eq!(vec, vec!["c", "b", "a"]);
        });
    }

    #[test]
    fn test_pop_front() {
        GhostToken::new(|mut token| {
            let mut map = BrandedLinkedHashMap::new();
            map.insert("a", 1);
            map.insert("b", 2);

            let (k, v) = map.pop_front().unwrap();
            assert_eq!(k, "a");
            assert_eq!(v, 1);

            assert_eq!(map.len(), 1);
            assert_eq!(map.get(&token, &"a"), None);
            assert_eq!(map.get(&token, &"b"), Some(&2));
        });
    }
}
