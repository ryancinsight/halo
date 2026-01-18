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

use crate::{GhostCell, GhostToken};
use crate::collections::vec::BrandedVec;
use core::mem::ManuallyDrop;

/// A slot in the pool.
#[derive(Copy, Clone)]
pub(crate) union PoolSlot<T> {
    pub(crate) value: ManuallyDrop<T>,
    pub(crate) next_free: usize,
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
            unsafe {
                // Use get_unchecked_mut_exclusive to avoid borrowing token again
                let slot = state.storage.get_unchecked_mut_exclusive(idx);

                // Read next_free from the slot (it was free)
                let next = slot.next_free;
                state.free_head = if next == usize::MAX { None } else { Some(next) };

                // Write value
                slot.value = ManuallyDrop::new(value);
                idx
            }
        } else {
            // Push new slot
            let idx = state.storage.len();
            state.storage.push(PoolSlot {
                value: ManuallyDrop::new(value),
            });
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
        ManuallyDrop::drop(&mut slot.value);

        // Add to free list
        slot.next_free = state.free_head.unwrap_or(usize::MAX);
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

        // Take the value
        let value = ManuallyDrop::take(&mut slot.value);

        // Add to free list
        slot.next_free = state.free_head.unwrap_or(usize::MAX);
        state.free_head = Some(index);

        value
    }

    /// Returns a shared reference to the value at `index`.
    ///
    /// # Safety
    /// `index` must be occupied.
    #[inline]
    pub unsafe fn get<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> &'a T {
        let state = self.state.borrow(token);
        let slot = state.storage.get_unchecked(token, index);
        &slot.value
    }

    /// Returns a mutable reference to the value at `index`.
    ///
    /// # Safety
    /// `index` must be occupied.
    #[inline]
    pub unsafe fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, index: usize) -> &'a mut T {
        let state = self.state.borrow_mut(token);
        let slot = state.storage.get_unchecked_mut_exclusive(index);
        &mut slot.value
    }

    /// Returns a mutable reference to the value at `index` without a token.
    ///
    /// This requires exclusive access to the pool (`&mut self`).
    ///
    /// # Safety
    /// `index` must be occupied.
    #[inline]
    pub unsafe fn get_mut_exclusive<'a>(&'a mut self, index: usize) -> &'a mut T {
        let state = self.state.get_mut();
        let slot = state.storage.get_unchecked_mut_exclusive(index);
        &mut slot.value
    }

    /// Returns a reference to the underlying storage.
    ///
    /// Useful for caching the storage reference during iteration to avoid repeated
    /// state borrowing.
    #[inline]
    pub fn storage<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a BrandedVec<'brand, PoolSlot<T>> {
        &self.state.borrow(token).storage
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
}

impl<'brand, T> Default for BrandedPool<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}
