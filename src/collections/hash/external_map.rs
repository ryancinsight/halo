//! `BrandedExternalHashMap` — a hash map that stores only indices.
//!
//! This map is designed for cases where keys are stored externally (e.g., in a pool or list),
//! and we want to avoid duplicating the key in the map.
//!
//! It stores `slots: Box<[usize]>` which point to the external storage.
//! Lookups and Insertions require a `context` closure to resolve the `usize` index to the actual `&K`.

use core::hash::{BuildHasher, Hash, Hasher};
use std::collections::hash_map::RandomState;
use std::borrow::Borrow;

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

pub struct BrandedExternalHashMap<S = RandomState> {
    ctrl: Box<[u8]>,
    slots: Box<[usize]>, // Stores the external index
    items_count: usize,
    capacity: usize,
    hash_builder: S,
}

impl BrandedExternalHashMap<RandomState> {
    pub fn new() -> Self {
        Self::with_capacity_and_hasher(0, RandomState::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<S> BrandedExternalHashMap<S>
where
    S: BuildHasher,
{
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let capacity = if capacity == 0 {
            0
        } else {
            capacity.next_power_of_two().max(8)
        };

        if capacity == 0 {
            return Self {
                ctrl: Box::new([]),
                slots: Box::new([]),
                items_count: 0,
                capacity: 0,
                hash_builder,
            };
        }

        let ctrl_len = capacity + GROUP_WIDTH;
        let ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
        let slots = vec![0; capacity].into_boxed_slice();

        Self {
            ctrl,
            slots,
            items_count: 0,
            capacity,
            hash_builder,
        }
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    fn hash<K: ?Sized + Hash>(&self, key: &K) -> (usize, u8) {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let h1 = (hash as usize) & (self.capacity - 1);
        let h2 = (hash >> 57) as u8;
        (h1, h2 & 0x7F)
    }

    #[inline]
    pub fn find_slot<K, R, Ctx>(&self, key: &K, h1: usize, h2: u8, context: Ctx) -> (usize, bool)
    where
        K: ?Sized + Eq,
        R: Borrow<K>,
        Ctx: Fn(usize) -> Option<R>,
    {
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

                    let external_idx = unsafe { *self.slots.get_unchecked(slot_idx) };
                    // Resolve key
                    if let Some(k) = context(external_idx) {
                        if k.borrow() == key {
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

    pub fn insert<K, R, Ctx>(&mut self, key: &K, external_index: usize, context: Ctx) -> Option<usize>
    where
        K: ?Sized + Hash + Eq,
        R: Borrow<K>,
        Ctx: Fn(usize) -> Option<R> + Copy,
    {
        if self.capacity == 0 || self.items_count >= self.capacity * 7 / 8 {
            let new_cap = (self.capacity * 2).max(8);
            self.grow(new_cap, context);
        }

        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2, context);

        if found {
            unsafe {
                let old_idx = *self.slots.get_unchecked(slot_idx);
                *self.slots.get_unchecked_mut(slot_idx) = external_index;
                Some(old_idx)
            }
        } else {
            unsafe {
                let ctrl_byte = *self.ctrl.get_unchecked(slot_idx);
                let was_deleted = ctrl_byte == DELETED;

                *self.slots.get_unchecked_mut(slot_idx) = external_index;
                self.ctrl[slot_idx] = h2;
                if slot_idx < GROUP_WIDTH {
                    self.ctrl[self.capacity + slot_idx] = h2;
                }

                if !was_deleted {
                    self.items_count += 1;
                }
            }
            None
        }
    }

    pub fn get<K, R, Ctx>(&self, key: &K, context: Ctx) -> Option<usize>
    where
        K: ?Sized + Hash + Eq,
        R: Borrow<K>,
        Ctx: Fn(usize) -> Option<R>,
    {
        if self.capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2, context);

        if found {
            unsafe { Some(*self.slots.get_unchecked(slot_idx)) }
        } else {
            None
        }
    }

    pub fn remove<K, R, Ctx>(&mut self, key: &K, context: Ctx) -> Option<usize>
    where
        K: ?Sized + Hash + Eq,
        R: Borrow<K>,
        Ctx: Fn(usize) -> Option<R>,
    {
        if self.capacity == 0 {
            return None;
        }
        let (h1, h2) = self.hash(key);
        let (slot_idx, found) = self.find_slot(key, h1, h2, context);

        if found {
            unsafe {
                let external_idx = *self.slots.get_unchecked(slot_idx);
                self.ctrl[slot_idx] = DELETED;
                if slot_idx < GROUP_WIDTH {
                    self.ctrl[self.capacity + slot_idx] = DELETED;
                }
                Some(external_idx)
            }
        } else {
            None
        }
    }

    fn grow<K, R, Ctx>(&mut self, new_cap: usize, context: Ctx)
    where
        K: ?Sized + Hash,
        R: Borrow<K>,
        Ctx: Fn(usize) -> Option<R>,
    {
        let old_ctrl = std::mem::take(&mut self.ctrl);
        let old_slots = std::mem::take(&mut self.slots);
        let old_cap = self.capacity;

        self.capacity = new_cap;
        if new_cap > 0 {
            let ctrl_len = new_cap + GROUP_WIDTH;
            self.ctrl = vec![EMPTY; ctrl_len].into_boxed_slice();
            self.slots = vec![0; new_cap].into_boxed_slice();
        } else {
            self.items_count = 0;
            return;
        }

        self.items_count = 0;

        for i in 0..old_cap {
            if old_ctrl[i] & 0x80 == 0 {
                let external_idx = old_slots[i];
                // Resolve key
                if let Some(key) = context(external_idx) {
                    self.insert_internal_during_grow(key.borrow(), external_idx);
                }
            }
        }
    }

    fn insert_internal_during_grow<K: ?Sized + Hash>(&mut self, key: &K, external_index: usize) {
        let (h1, h2) = self.hash(key);
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
                let slot_idx = (idx + trailing as usize) & mask;

                unsafe {
                    *self.slots.get_unchecked_mut(slot_idx) = external_index;
                    self.ctrl[slot_idx] = h2;
                    if slot_idx < GROUP_WIDTH {
                        self.ctrl[self.capacity + slot_idx] = h2;
                    }
                }
                self.items_count += 1;
                return;
            }

            idx = (idx + step) & mask;
            step += GROUP_WIDTH;
        }
    }
}

impl<S: Default + BuildHasher> Default for BrandedExternalHashMap<S> {
    fn default() -> Self {
        Self::with_capacity_and_hasher(0, S::default())
    }
}
