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

use crate::collections::vec::BrandedVec;
use crate::{GhostCell, GhostToken};

/// A slot in the pool.
#[derive(Copy, Clone)]
pub(crate) enum PoolSlot<T> {
    Occupied(T),
    Free(usize),
}

/// Internal state of the pool.
struct PoolState<'brand, T> {
    storage: BrandedVec<'brand, PoolSlot<T>>,
    free_head: Option<usize>,
    len: usize,
}

/// A branded pool allocator.
pub struct BrandedPool<'brand, T> {
    state: GhostCell<'brand, PoolState<'brand, T>>,
}

impl<'brand, T> BrandedPool<'brand, T> {
    /// Creates a new empty pool.
    pub fn new() -> Self {
        Self {
            state: GhostCell::new(PoolState {
                storage: BrandedVec::new(),
                free_head: None,
                len: 0,
            }),
        }
    }

    /// Creates a new pool with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            state: GhostCell::new(PoolState {
                storage: BrandedVec::with_capacity(capacity),
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
            // Use get_unchecked_mut_exclusive to avoid borrowing token again
            // Safety: free_head contains valid indices
            unsafe {
                let slot = state.storage.get_unchecked_mut_exclusive(idx);

                // Read next_free from the slot (it was free)
                if let PoolSlot::Free(next) = slot {
                    state.free_head = if *next == usize::MAX {
                        None
                    } else {
                        Some(*next)
                    };
                } else {
                    // Should be unreachable if free_head invariant holds
                    debug_assert!(false, "Free head pointed to occupied slot");
                }

                // Write value
                *slot = PoolSlot::Occupied(value);
                idx
            }
        } else {
            // Push new slot
            let idx = state.storage.len();
            state.storage.push(PoolSlot::Occupied(value));
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

        // Add to free list
        *slot = PoolSlot::Free(state.free_head.unwrap_or(usize::MAX));
        state.free_head = Some(index);
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

        // Take the value - requires replacing the slot
        let old_slot =
            std::mem::replace(slot, PoolSlot::Free(state.free_head.unwrap_or(usize::MAX)));
        state.free_head = Some(index);

        match old_slot {
            PoolSlot::Occupied(v) => v,
            PoolSlot::Free(_) => panic!("Double free in take()"),
        }
    }

    /// Returns a shared reference to the value at `index`.
    ///
    /// Returns `None` if the slot is free or index is out of bounds (safe).
    #[inline]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> Option<&'a T> {
        let state = self.state.borrow(token);
        match state.storage.get(token, index) {
            Some(PoolSlot::Occupied(val)) => Some(val),
            _ => None,
        }
    }

    /// Returns a shared reference to the value at `index` without checking bounds or occupancy.
    ///
    /// # Safety
    /// The caller must ensure that `index` is within bounds and points to an `Occupied` slot.
    #[inline]
    pub unsafe fn get_unchecked<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> &'a T {
        let state = self.state.borrow(token);
        match state.storage.get_unchecked(token, index) {
            PoolSlot::Occupied(val) => val,
            PoolSlot::Free(_) => std::hint::unreachable_unchecked(),
        }
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
        // We need get_mut_exclusive here because we borrowed state
        unsafe {
            if index < state.storage.len() {
                match state.storage.get_unchecked_mut_exclusive(index) {
                    PoolSlot::Occupied(val) => Some(val),
                    _ => None,
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
        if index < state.storage.len() {
            match unsafe { state.storage.get_unchecked_mut_exclusive(index) } {
                PoolSlot::Occupied(val) => Some(val),
                _ => None,
            }
        } else {
            None
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

    /// Returns a reference to the underlying storage.
    #[inline]
    pub fn storage<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> &'a BrandedVec<'brand, PoolSlot<T>> {
        &self.state.borrow(token).storage
    }

    /// Returns a slice of the underlying storage.
    #[inline]
    pub fn as_slice<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a [PoolSlot<T>] {
        self.state.borrow(token).storage.as_slice(token)
    }

    /// Returns a mutable slice of the underlying storage.
    #[inline]
    pub fn as_mut_slice<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a mut [PoolSlot<T>] {
        let state = self.state.borrow_mut(token);
        state.storage.as_mut_slice_exclusive()
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

        for slot in state.storage.as_slice(token) {
            match slot {
                PoolSlot::Occupied(val) => {
                    let (new_val, aux) = clone_fn(val);
                    new_storage.push(PoolSlot::Occupied(new_val));
                    aux_vec.push(Some(aux));
                }
                PoolSlot::Free(next) => {
                    new_storage.push(PoolSlot::Free(*next));
                    aux_vec.push(None);
                }
            }
        }

        (
            BrandedPool {
                state: GhostCell::new(PoolState {
                    storage: new_storage,
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
