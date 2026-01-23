//! `BrandedPool` — a shared object pool with token-gated allocation.
//!
//! Provides a shared allocator for objects of type `T`, reusing memory slots via a free list.
//! Useful for implementing linked data structures (linked lists, graphs) where nodes
//! can be allocated and freed individually but share a common backing store.
//!
//! # Features
//! - **Shared Access**: Allocation and deallocation require `&self` and `&mut GhostToken`.
//!   This allows multiple data structures to share the same pool.
//! - **Free List Reuse**: Frees slots are reused O(1).
//! - **Token Gated**: Access to values requires a `GhostToken`, ensuring safety.
//! - **Memory Efficient**: Uses a union and bitset to minimize overhead.

use crate::collections::vec::BrandedVec;
use crate::{GhostCell, GhostToken};
use core::mem::ManuallyDrop;

/// A slot in the pool.
///
/// Uses a union to overlap storage for occupied values and free list indices,
/// saving memory compared to an enum. Occupancy is tracked separately.
pub union PoolSlot<T> {
    pub(crate) occupied: ManuallyDrop<T>,
    pub(crate) next_free: usize,
}

/// Internal state of the pool.
struct PoolState<'brand, T> {
    storage: BrandedVec<'brand, PoolSlot<T>>,
    occupied: Vec<u64>, // BitSet
    free_head: Option<usize>,
    len: usize,
}

/// A branded pool allocator.
pub struct BrandedPool<'brand, T> {
    state: GhostCell<'brand, PoolState<'brand, T>>,
}

/// A view into the pool for iteration.
pub struct PoolView<'a, T> {
    pub storage: &'a [PoolSlot<T>],
    pub occupied: &'a [u64],
}

/// A mutable view into the pool for iteration.
pub struct PoolViewMut<'a, T> {
    pub storage: &'a mut [PoolSlot<T>],
    pub occupied: &'a [u64],
}

const BIT_SHIFT: usize = 6;
const BIT_MASK: usize = 63;

impl<'brand, T> BrandedPool<'brand, T> {
    /// Creates a new empty pool.
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

    /// Creates a new pool with specified capacity.
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

    /// Allocates a value in the pool, returning its index.
    #[inline]
    pub fn alloc(&self, token: &mut GhostToken<'brand>, value: T) -> usize {
        let state = self.state.borrow_mut(token);

        state.len += 1;

        if let Some(idx) = state.free_head {
            // Reuse slot
            unsafe {
                let slot = state.storage.get_unchecked_mut_exclusive(idx);

                // Read next_free from the slot (it was free)
                let next = slot.next_free;

                state.free_head = if next == usize::MAX {
                    None
                } else {
                    Some(next)
                };

                // Write value
                slot.occupied = ManuallyDrop::new(value);

                // Set occupied bit
                let word_idx = idx >> BIT_SHIFT;
                let bit_idx = idx & BIT_MASK;
                // occupied vec should be large enough because idx < len
                state.occupied[word_idx] |= 1 << bit_idx;

                idx
            }
        } else {
            // Push new slot
            // TODO: Optimize bulk allocation by reserving capacity ahead of time if possible.
            let idx = state.storage.len();
            state.storage.push(PoolSlot {
                occupied: ManuallyDrop::new(value),
            });

            // Update bitset
            let word_idx = idx >> BIT_SHIFT;
            let bit_idx = idx & BIT_MASK;
            if word_idx >= state.occupied.len() {
                state.occupied.push(0);
            }
            state.occupied[word_idx] |= 1 << bit_idx;

            idx
        }
    }

    /// Deallocates the value at `index`.
    ///
    /// # Safety
    /// The `index` must be currently allocated (occupied).
    /// Double-freeing or freeing an invalid index causes undefined behavior.
    #[inline]
    pub unsafe fn free(&self, token: &mut GhostToken<'brand>, index: usize) {
        let state = self.state.borrow_mut(token);

        state.len -= 1;

        let slot = state.storage.get_unchecked_mut_exclusive(index);

        // Drop the value
        ManuallyDrop::drop(&mut slot.occupied);

        // Clear occupied bit
        let word_idx = index >> BIT_SHIFT;
        let bit_idx = index & BIT_MASK;
        state.occupied[word_idx] &= !(1 << bit_idx);

        // Add to free list
        slot.next_free = state.free_head.unwrap_or(usize::MAX);
        state.free_head = Some(index);

        // TODO: Implement pool shrinking (reclaiming memory) when utilization drops below a threshold.
    }

    /// Deallocates the value at `index` and returns it.
    ///
    /// # Safety
    /// The `index` must be currently allocated (occupied).
    #[inline]
    pub unsafe fn take(&self, token: &mut GhostToken<'brand>, index: usize) -> T {
        let state = self.state.borrow_mut(token);

        state.len -= 1;

        let slot = state.storage.get_unchecked_mut_exclusive(index);

        // Take value
        let value = ManuallyDrop::take(&mut slot.occupied);

        // Clear occupied bit
        let word_idx = index >> BIT_SHIFT;
        let bit_idx = index & BIT_MASK;
        state.occupied[word_idx] &= !(1 << bit_idx);

        // Add to free list
        slot.next_free = state.free_head.unwrap_or(usize::MAX);
        state.free_head = Some(index);

        value
    }

    /// Returns a shared reference to the value at `index`.
    ///
    /// Returns `None` if the slot is free or index is out of bounds (safe).
    #[inline]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> Option<&'a T> {
        let state = self.state.borrow(token);
        if index < state.storage.len() {
            let word_idx = index >> BIT_SHIFT;
            let bit_idx = index & BIT_MASK;
            if (state.occupied[word_idx] & (1 << bit_idx)) != 0 {
                // Safety: checked occupied bit
                unsafe {
                    Some(&state.storage.get_unchecked(token, index).occupied)
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Returns a shared reference to the value at `index` without checking bounds or occupancy.
    ///
    /// # Safety
    /// The caller must ensure that `index` is within bounds and points to an `Occupied` slot.
    #[inline]
    pub unsafe fn get_unchecked<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> &'a T {
        let state = self.state.borrow(token);
        &state.storage.get_unchecked(token, index).occupied
    }

    /// Returns a mutable reference to the value at `index`.
    ///
    /// Returns `None` if the slot is free or index is out of bounds.
    #[inline]
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        index: usize,
    ) -> Option<&'a mut T> {
        let state = self.state.borrow_mut(token);
        unsafe {
            if index < state.storage.len() {
                let word_idx = index >> BIT_SHIFT;
                let bit_idx = index & BIT_MASK;
                if (state.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    Some(&mut state.storage.get_unchecked_mut_exclusive(index).occupied)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    /// Returns a mutable reference to the value at `index` without a token.
    ///
    /// This requires exclusive access to the pool (`&mut self`).
    #[inline]
    pub fn get_mut_exclusive<'a>(&'a mut self, index: usize) -> Option<&'a mut T> {
        let state = self.state.get_mut();
        unsafe {
            if index < state.storage.len() {
                let word_idx = index >> BIT_SHIFT;
                let bit_idx = index & BIT_MASK;
                if (state.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    Some(&mut state.storage.get_unchecked_mut_exclusive(index).occupied)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    /// Returns the raw capacity of the underlying storage.
    /// Used for iterating in Drop.
    #[inline]
    pub fn capacity_len(&mut self) -> usize {
        self.state.get_mut().storage.len()
    }

    /// Returns the number of allocated items.
    #[inline]
    pub fn len(&self, token: &GhostToken<'brand>) -> usize {
        self.state.borrow(token).len
    }

    /// Returns true if empty.
    #[inline]
    pub fn is_empty(&self, token: &GhostToken<'brand>) -> bool {
        self.len(token) == 0
    }

    /// Returns a view of the underlying storage and occupancy map.
    #[inline]
    pub fn view<'a>(&'a self, token: &'a GhostToken<'brand>) -> PoolView<'a, T> {
        let state = self.state.borrow(token);
        PoolView {
            storage: state.storage.as_slice(token),
            occupied: &state.occupied,
        }
    }

    /// Returns a mutable view of the underlying storage and occupancy map.
    #[inline]
    pub fn view_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> PoolViewMut<'a, T> {
        let state = self.state.borrow_mut(token);
        PoolViewMut {
            storage: state.storage.as_mut_slice_exclusive(),
            occupied: &state.occupied,
        }
    }

    /// Clones the pool structure to a new brand, mapping elements via `clone_fn`.
    ///
    /// Preserves the exact structure, including free lists and indices.
    /// Returns the new pool and an auxiliary vector containing the secondary output of `clone_fn`
    /// for occupied slots (and `None` for free slots).
    pub fn clone_structure<'new_brand, U, Aux, F>(
        &self,
        token: &GhostToken<'brand>,
        mut clone_fn: F,
    ) -> (BrandedPool<'new_brand, U>, Vec<Option<Aux>>)
    where
        F: FnMut(&T) -> (U, Aux),
    {
        let state = self.state.borrow(token);
        let mut new_storage = BrandedVec::with_capacity(state.storage.len());
        let mut aux_vec = Vec::with_capacity(state.storage.len());

        let storage_slice = state.storage.as_slice(token);

        for (i, slot) in storage_slice.iter().enumerate() {
            let word_idx = i >> BIT_SHIFT;
            let bit_idx = i & BIT_MASK;
            let is_occupied = (state.occupied[word_idx] & (1 << bit_idx)) != 0;

            unsafe {
                if is_occupied {
                    let (new_val, aux) = clone_fn(&slot.occupied);
                    new_storage.push(PoolSlot {
                        occupied: ManuallyDrop::new(new_val)
                    });
                    aux_vec.push(Some(aux));
                } else {
                    new_storage.push(PoolSlot {
                        next_free: slot.next_free
                    });
                    aux_vec.push(None);
                }
            }
        }

        (
            BrandedPool {
                state: GhostCell::new(PoolState {
                    storage: new_storage,
                    occupied: state.occupied.clone(),
                    free_head: state.free_head,
                    len: state.len,
                }),
            },
            aux_vec,
        )
    }
}

impl<'brand, T> Default for BrandedPool<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}
