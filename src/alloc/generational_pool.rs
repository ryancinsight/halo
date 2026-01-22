//! `GenerationalPool` — a token-gated object pool with generational indices.
//!
//! Prevents ABA problems by checking generations on access.
//! Useful when indices are held for long periods and might become stale.

use crate::collections::vec::BrandedVec;
use crate::{GhostCell, GhostToken};
use core::mem::ManuallyDrop;
use core::marker::PhantomData;

/// A generational index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenerationalIndex<'brand> {
    index: usize,
    generation: u32,
    _marker: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> GenerationalIndex<'brand> {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn generation(&self) -> u32 {
        self.generation
    }
}

/// Internal slot data.
union SlotData<T> {
    occupied: ManuallyDrop<T>,
    next_free: usize,
}

/// A slot with generation.
struct Slot<T> {
    generation: u32,
    data: SlotData<T>,
}

/// Internal state.
struct PoolState<'brand, T> {
    storage: BrandedVec<'brand, Slot<T>>,
    occupied: Vec<u64>, // BitSet
    free_head: Option<usize>,
    len: usize,
}

/// A generational pool allocator.
pub struct GenerationalPool<'brand, T> {
    state: GhostCell<'brand, PoolState<'brand, T>>,
}

const BIT_SHIFT: usize = 6;
const BIT_MASK: usize = 63;

impl<'brand, T> GenerationalPool<'brand, T> {
    pub fn new() -> Self {
        Self {
            state: GhostCell::new(PoolState {
                storage: BrandedVec::new(),
                occupied: Vec::new(),
                free_head: None,
                len: 0,
            }),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let words = (capacity + 63) / 64;
        Self {
            state: GhostCell::new(PoolState {
                storage: BrandedVec::with_capacity(capacity),
                occupied: Vec::with_capacity(words),
                free_head: None,
                len: 0,
            }),
        }
    }

    pub fn alloc(&self, token: &mut GhostToken<'brand>, value: T) -> GenerationalIndex<'brand> {
        let state = self.state.borrow_mut(token);
        state.len += 1;

        if let Some(idx) = state.free_head {
            unsafe {
                let slot = state.storage.get_unchecked_mut_exclusive(idx);
                let next = slot.data.next_free;

                state.free_head = if next == usize::MAX { None } else { Some(next) };

                // Increment generation (wrapping)
                slot.generation = slot.generation.wrapping_add(1);
                slot.data.occupied = ManuallyDrop::new(value);

                let word_idx = idx >> BIT_SHIFT;
                let bit_idx = idx & BIT_MASK;
                state.occupied[word_idx] |= 1 << bit_idx;

                GenerationalIndex {
                    index: idx,
                    generation: slot.generation,
                    _marker: PhantomData,
                }
            }
        } else {
            let idx = state.storage.len();
            state.storage.push(Slot {
                generation: 0,
                data: SlotData { occupied: ManuallyDrop::new(value) },
            });

            let word_idx = idx >> BIT_SHIFT;
            let bit_idx = idx & BIT_MASK;
            if word_idx >= state.occupied.len() {
                state.occupied.push(0);
            }
            state.occupied[word_idx] |= 1 << bit_idx;

            GenerationalIndex {
                index: idx,
                generation: 0,
                _marker: PhantomData,
            }
        }
    }

    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: GenerationalIndex<'brand>) -> Option<&'a T> {
        let state = self.state.borrow(token);
        let i = idx.index;
        if i < state.storage.len() {
             let word_idx = i >> BIT_SHIFT;
             let bit_idx = i & BIT_MASK;
             if (state.occupied[word_idx] & (1 << bit_idx)) != 0 {
                 let slot = unsafe { state.storage.get_unchecked(token, i) };
                 if slot.generation == idx.generation {
                     return Some(unsafe { &slot.data.occupied });
                 }
             }
        }
        None
    }

    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: GenerationalIndex<'brand>) -> Option<&'a mut T> {
        let state = self.state.borrow_mut(token);
        let i = idx.index;
        // We need unsafe access because we borrowed state
        unsafe {
            if i < state.storage.len() {
                let word_idx = i >> BIT_SHIFT;
                let bit_idx = i & BIT_MASK;
                 if (state.occupied[word_idx] & (1 << bit_idx)) != 0 {
                     let slot = state.storage.get_unchecked_mut_exclusive(i);
                     if slot.generation == idx.generation {
                         return Some(&mut slot.data.occupied);
                     }
                 }
            }
        }
        None
    }

    pub fn free(&self, token: &mut GhostToken<'brand>, idx: GenerationalIndex<'brand>) -> bool {
        let state = self.state.borrow_mut(token);
        let i = idx.index;

        unsafe {
            if i < state.storage.len() {
                 let word_idx = i >> BIT_SHIFT;
                 let bit_idx = i & BIT_MASK;
                 if (state.occupied[word_idx] & (1 << bit_idx)) != 0 {
                     let slot = state.storage.get_unchecked_mut_exclusive(i);
                     if slot.generation == idx.generation {
                         // Valid free
                         ManuallyDrop::drop(&mut slot.data.occupied);

                         state.occupied[word_idx] &= !(1 << bit_idx);

                         slot.data.next_free = state.free_head.unwrap_or(usize::MAX);
                         state.free_head = Some(i);

                         state.len -= 1;
                         return true;
                     }
                 }
            }
        }
        false
    }

    pub fn len(&self, token: &GhostToken<'brand>) -> usize {
        self.state.borrow(token).len
    }

    pub fn is_empty(&self, token: &GhostToken<'brand>) -> bool {
        self.len(token) == 0
    }
}

impl<'brand, T> Default for GenerationalPool<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}
