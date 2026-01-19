//! `BrandedSlotMap` — a generational arena with token-gated access.
//!
//! A high-performance slot map (generational arena) that uses branding to ensure
//! keys are valid for the specific map instance and have not expired (ABA protection).
//!
//! Implementation details:
//! - Uses `BrandedVec` for storage.
//! - Uses a `union` to overlap occupied and free states, minimizing memory usage.
//! - Keys are branded to prevent misuse across different maps.
//! - Generation counters prevent ABA problems when slots are reused.

use crate::collections::BrandedCollection;
use crate::{BrandedVec, GhostCell, GhostToken};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;

/// A key for accessing a `BrandedSlotMap`.
///
/// Contains an index and a generation counter. The lifetime `'brand` ensures
/// the key cannot be used with a different map instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotKey<'brand> {
    index: u32,
    generation: u32,
    _marker: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> SlotKey<'brand> {
    fn new(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            _marker: PhantomData,
        }
    }
}

/// Internal entry in the slot map.
union SlotData<T> {
    /// If occupied, contains the value.
    value: ManuallyDrop<T>,
    /// If free, contains the index of the next free slot.
    next_free: u32,
}

struct Entry<T> {
    /// Generation counter. Even = occupied, Odd = free.
    generation: u32,
    data: SlotData<T>,
}

/// A generational slot map protected by a ghost token.
pub struct BrandedSlotMap<'brand, T> {
    slots: BrandedVec<'brand, Entry<T>>,
    free_head: u32,
    len: usize,
}

impl<'brand, T> BrandedSlotMap<'brand, T> {
    /// Creates a new empty slot map.
    pub fn new() -> Self {
        Self {
            slots: BrandedVec::new(),
            free_head: u32::MAX, // Sentinel for no free slots
            len: 0,
        }
    }

    /// Creates a new slot map with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: BrandedVec::with_capacity(capacity),
            free_head: u32::MAX,
            len: 0,
        }
    }

    /// Inserts a value into the map, returning a branded key.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, value: T) -> SlotKey<'brand> {
        self.len += 1;

        if self.free_head != u32::MAX {
            // Reuse a free slot
            let idx = self.free_head as usize;

            unsafe {
                let entry = self.slots.get_unchecked_mut(token, idx);

                // Read next_free from the union (it was free)
                let next_free = entry.data.next_free;
                self.free_head = next_free;

                // Write value to union
                entry.data.value = ManuallyDrop::new(value);

                // Entry was Free (Odd). Increment to make it Occupied (Even).
                entry.generation = entry.generation.wrapping_add(1);

                SlotKey::new(idx as u32, entry.generation)
            }
        } else {
            // Allocate new slot
            let idx = self.slots.len();
            // New slot starts at generation 0 (Even, Occupied)
            let generation = 0;

            let entry = Entry {
                generation,
                data: SlotData {
                    value: ManuallyDrop::new(value),
                },
            };

            self.slots.push(entry);

            SlotKey::new(idx as u32, generation)
        }
    }

    /// Returns a shared reference to the value associated with the key.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, key: SlotKey<'brand>) -> Option<&'a T> {
        let idx = key.index as usize;

        if let Some(entry) = self.slots.get(token, idx) {
            // Check generation match AND parity (Occupied = Even)
            // Actually, if key.generation matches entry.generation, and key was issued by insert,
            // then it implies Occupied state unless we have 2^32 wraparound exactly on a free slot,
            // which is incredibly unlikely and standard slotmap risk.
            if entry.generation == key.generation && entry.generation % 2 == 0 {
                unsafe {
                    return Some(&entry.data.value);
                }
            }
        }
        None
    }

    /// Returns a mutable reference to the value associated with the key.
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        key: SlotKey<'brand>,
    ) -> Option<&'a mut T> {
        let idx = key.index as usize;

        if let Some(entry) = self.slots.get_mut(token, idx) {
            if entry.generation == key.generation && entry.generation % 2 == 0 {
                unsafe {
                    return Some(&mut entry.data.value);
                }
            }
        }
        None
    }

    /// Removes a key from the map, returning the value.
    pub fn remove(&mut self, token: &mut GhostToken<'brand>, key: SlotKey<'brand>) -> Option<T> {
        let idx = key.index as usize;

        if let Some(entry) = self.slots.get_mut(token, idx) {
            if entry.generation == key.generation && entry.generation % 2 == 0 {
                self.len -= 1;
                unsafe {
                    let value = ManuallyDrop::take(&mut entry.data.value);

                    entry.data.next_free = self.free_head;
                    self.free_head = idx as u32;

                    // Increment to Odd (Free)
                    entry.generation = entry.generation.wrapping_add(1);

                    return Some(value);
                }
            }
        }
        None
    }

    /// Returns true if the map contains the key.
    pub fn contains_key(&self, token: &GhostToken<'brand>, key: SlotKey<'brand>) -> bool {
        let idx = key.index as usize;
        if let Some(entry) = self.slots.get(token, idx) {
            return entry.generation == key.generation && entry.generation % 2 == 0;
        }
        false
    }

    /// Clears the map, removing all values.
    pub fn clear(&mut self, token: &mut GhostToken<'brand>) {
        if self.len == 0 {
            return;
        }

        // Iterate and drop occupied slots
        for idx in 0..self.slots.len() {
            let entry = unsafe { self.slots.get_unchecked_mut(token, idx) };
            if entry.generation % 2 == 0 {
                // Occupied (Even)
                unsafe {
                    ManuallyDrop::drop(&mut entry.data.value);
                }
                // Mark free (Odd)
                entry.generation = entry.generation.wrapping_add(1);
            }
        }

        // Rebuild free list
        self.free_head = 0;
        self.len = 0;

        let cap = self.slots.len();
        if cap > 0 {
            for idx in 0..cap {
                let entry = unsafe { self.slots.get_unchecked_mut(token, idx) };
                // Set next free
                if idx < cap - 1 {
                    entry.data.next_free = (idx + 1) as u32;
                } else {
                    entry.data.next_free = u32::MAX;
                }
            }
        } else {
            self.free_head = u32::MAX;
        }
    }
}

impl<'brand, T> BrandedCollection<'brand> for BrandedSlotMap<'brand, T> {
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn len(&self) -> usize {
        self.len
    }
}

// Iterator
pub struct Iter<'a, 'brand, T> {
    map: &'a BrandedSlotMap<'brand, T>,
    token: &'a GhostToken<'brand>,
    index: usize,
    count: usize,
}

impl<'a, 'brand, T> Iterator for Iter<'a, 'brand, T> {
    type Item = (SlotKey<'brand>, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }

        while self.index < self.map.slots.len() {
            let idx = self.index;
            self.index += 1;

            unsafe {
                let entry = self.map.slots.get_unchecked(self.token, idx);
                if entry.generation % 2 == 0 {
                    // Occupied
                    self.count -= 1;
                    let key = SlotKey::new(idx as u32, entry.generation);
                    return Some((key, &entry.data.value));
                }
            }
        }
        None
    }
}

impl<'brand, T> BrandedSlotMap<'brand, T> {
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> Iter<'a, 'brand, T> {
        Iter {
            map: self,
            token,
            index: 0,
            count: self.len,
        }
    }
}

impl<'brand, T> Default for BrandedSlotMap<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: BrandedSlotMap is Send/Sync if T is.
unsafe impl<'brand, T: Send> Send for BrandedSlotMap<'brand, T> {}
unsafe impl<'brand, T: Sync> Sync for BrandedSlotMap<'brand, T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_slot_map_basic() {
        GhostToken::new(|mut token| {
            let mut map = BrandedSlotMap::new();
            assert!(map.is_empty());

            let k1 = map.insert(&mut token, 10);
            let k2 = map.insert(&mut token, 20);

            assert_eq!(map.len(), 2);
            assert_eq!(*map.get(&token, k1).unwrap(), 10);
            assert_eq!(*map.get(&token, k2).unwrap(), 20);

            // Mutation
            *map.get_mut(&mut token, k1).unwrap() = 11;
            assert_eq!(*map.get(&token, k1).unwrap(), 11);

            // Removal
            assert_eq!(map.remove(&mut token, k1), Some(11));
            assert_eq!(map.len(), 1);
            assert!(map.get(&token, k1).is_none());
            assert!(!map.contains_key(&token, k1));

            // Reuse
            let k3 = map.insert(&mut token, 30);
            // k3 should likely reuse k1's index but have new generation
            assert_eq!(map.len(), 2);
            assert_eq!(*map.get(&token, k3).unwrap(), 30);

            // k1 should still be invalid
            assert!(map.get(&token, k1).is_none());
        });
    }

    #[test]
    fn test_slot_map_iter() {
        GhostToken::new(|mut token| {
            let mut map = BrandedSlotMap::new();
            let mut keys = Vec::new();
            for i in 0..10 {
                keys.push(map.insert(&mut token, i * 10));
            }

            let mut count = 0;
            for (k, v) in map.iter(&token) {
                assert!(keys.contains(&k));
                assert_eq!(k.index as i32 * 10, *v as i32);
                count += 1;
            }
            assert_eq!(count, 10);
        });
    }

    #[test]
    fn test_slot_map_clear() {
        GhostToken::new(|mut token| {
            let mut map = BrandedSlotMap::new();
            for i in 0..10 {
                map.insert(&mut token, i);
            }
            assert_eq!(map.len(), 10);

            map.clear(&mut token);
            assert_eq!(map.len(), 0);

            // Reinsert
            for i in 0..5 {
                map.insert(&mut token, i + 100);
            }
            assert_eq!(map.len(), 5);
        });
    }
}
