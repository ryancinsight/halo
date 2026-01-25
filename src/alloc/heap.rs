//! `BrandedHeap` — a token-gated memory allocator.
//!
//! This module provides a global-like heap allocator that is gated by a `GhostToken`.
//! It serves as a replacement for `std::alloc` within the branded ecosystem, ensuring
//! that memory operations are tied to a specific brand/session.
//!
//! # Implementation
//!
//! This implementation uses a Buddy Allocator system.
//! - **Shared Access (`&GhostToken`)**: Allows allocation and deallocation without locks using
//!   lock-free free-lists (Treiber stacks). Deallocation is deferred (no coalescing) to avoid
//!   locking.
//! - **Exclusive Access (`&mut GhostToken`)**: Allows `compact` which performs full coalescing
//!   of free blocks to reduce fragmentation.

use crate::{GhostCell, GhostToken};
use crate::concurrency::CachePadded;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc, handle_alloc_error, realloc};
use std::cell::UnsafeCell;

// Constants
const MIN_BLOCK_SIZE: usize = 16;
// 32 levels: 16 * 2^31 = 32GB (approx). Enough for most use cases.
const LEVELS: usize = 32;

const NONE: usize = usize::MAX;

// Tag constants for ABA prevention in free list
const TAG_SHIFT: usize = 32;
const INDEX_MASK: usize = (1 << TAG_SHIFT) - 1;

#[inline(always)]
fn unpack(val: usize) -> (usize, usize) {
    (val & INDEX_MASK, val >> TAG_SHIFT)
}

#[inline(always)]
fn pack(index: usize, tag: usize) -> usize {
    (tag << TAG_SHIFT) | (index & INDEX_MASK)
}

/// Internal state of the heap.
struct HeapState {
    /// Free lists for each order. Stored as AtomicUsize (index + tag).
    free_heads: [CachePadded<AtomicUsize>; LEVELS],
    /// Block orders. Indexed by `offset / MIN_BLOCK_SIZE`.
    /// Access requires valid ownership of the block or lock-free protocol.
    orders: Box<[UnsafeCell<u8>]>,
    /// Number of blocks (capacity / MIN_BLOCK_SIZE).
    num_blocks: usize,
}

/// A token-gated heap allocator using Buddy System.
///
/// This struct manages a raw memory block (default 8MB) using a Buddy System allocator.
/// It wraps `std::alloc` only to acquire the initial memory chunk.
pub struct BrandedHeap<'brand> {
    state: GhostCell<'brand, HeapState>,
    memory: NonNull<u8>,
    capacity: usize,
}

// Safety: `memory` is read-only (base pointer).
// `state` handles interior mutability via GhostCell and Atomics.
unsafe impl<'brand> Sync for BrandedHeap<'brand> {}
unsafe impl<'brand> Send for BrandedHeap<'brand> {}

impl<'brand> BrandedHeap<'brand> {
    /// Creates a new branded heap interface with default capacity (8MB).
    pub fn new() -> Self {
        Self::with_capacity(8 * 1024 * 1024)
    }

    /// Creates a new branded heap with specified capacity.
    /// Capacity will be rounded up to the next power of two.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(MIN_BLOCK_SIZE).next_power_of_two();
        let layout = Layout::from_size_align(capacity, 4096).unwrap();

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                handle_alloc_error(layout);
            }

            let num_blocks = capacity / MIN_BLOCK_SIZE;
            let mut orders = Vec::with_capacity(num_blocks);
            orders.resize_with(num_blocks, || UnsafeCell::new(0));

            let mut state = HeapState {
                free_heads: core::array::from_fn(|_| CachePadded::new(AtomicUsize::new(pack(NONE, 0)))),
                orders: orders.into_boxed_slice(),
                num_blocks,
            };

            // Initialize heap by pushing maximal free blocks
            let mut current_start_idx = 0;
            let mut remaining_capacity = capacity;

            while remaining_capacity >= MIN_BLOCK_SIZE {
                // Find largest power of 2 that fits in remaining_capacity and aligns with current_start
                // Since capacity is power of 2 and we start at 0, usually it's just one block.
                // But for robustness, we loop.
                let max_size_log2 = (remaining_capacity.trailing_zeros() as usize).min(
                     (current_start_idx * MIN_BLOCK_SIZE).trailing_zeros() as usize
                );
                // Wait, logic is simpler:
                // We greedily take the largest order K such that 2^K * MIN <= remaining
                // AND index is aligned? (Index 0 is always aligned).

                let order = (remaining_capacity.trailing_zeros() - MIN_BLOCK_SIZE.trailing_zeros()) as usize;

                // Cap order at LEVELS - 1
                let effective_order = order.min(LEVELS - 1);

                // Write order
                state.orders[current_start_idx].get().write(effective_order as u8);

                // Push to list
                let node_ptr = ptr.add(current_start_idx * MIN_BLOCK_SIZE) as *mut AtomicUsize;
                (*node_ptr).store(pack(NONE, 0), Ordering::Relaxed);

                state.free_heads[effective_order].store(pack(current_start_idx, 0), Ordering::Relaxed);

                let size_consumed = MIN_BLOCK_SIZE << effective_order;
                remaining_capacity -= size_consumed;
                current_start_idx += size_consumed / MIN_BLOCK_SIZE;
            }

            Self {
                state: GhostCell::new(state),
                memory: NonNull::new_unchecked(ptr),
                capacity,
            }
        }
    }

    /// Allocates memory with the given layout.
    ///
    /// Requires access to the token (validating permission).
    pub unsafe fn alloc(&self, token: &GhostToken<'brand>, layout: Layout) -> *mut u8 {
        let size = layout.size().max(layout.align()).max(MIN_BLOCK_SIZE).next_power_of_two();
        let order = (size.trailing_zeros() - MIN_BLOCK_SIZE.trailing_zeros()) as usize;

        if order >= LEVELS {
            return core::ptr::null_mut();
        }

        let state = self.state.borrow(token);
        let base_ptr = self.memory.as_ptr();

        for k in order..LEVELS {
            let head_atomic = &state.free_heads[k];

            loop {
                let head_val = head_atomic.load(Ordering::Acquire);
                let (head_idx, tag) = unpack(head_val);

                if head_idx == NONE {
                    break;
                }

                // Read next pointer safely using Atomic load
                let block_ptr = base_ptr.add(head_idx * MIN_BLOCK_SIZE);
                let next_atomic = &*(block_ptr as *const AtomicUsize);
                let next_val_packed = next_atomic.load(Ordering::Relaxed);

                let (next_idx, next_tag) = unpack(next_val_packed);
                let new_head = pack(next_idx, tag.wrapping_add(1));

                if head_atomic.compare_exchange_weak(
                    head_val,
                    new_head,
                    Ordering::AcqRel,
                    Ordering::Acquire
                ).is_ok() {
                    // Split if needed
                    let mut current_idx = head_idx;
                    let mut current_order = k;

                    while current_order > order {
                        let split_order = current_order - 1;
                        let buddy_idx = current_idx + (1 << split_order);

                        *state.orders[current_idx].get() = split_order as u8;
                        *state.orders[buddy_idx].get() = split_order as u8;

                        self.push_free(state, split_order, buddy_idx);

                        current_order = split_order;
                    }

                    *state.orders[current_idx].get() = order as u8;
                    return base_ptr.add(current_idx * MIN_BLOCK_SIZE);
                }
            }
        }

        core::ptr::null_mut()
    }

    /// Deallocates memory.
    pub unsafe fn dealloc(&self, token: &GhostToken<'brand>, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() { return; }

        let offset = ptr as usize - self.memory.as_ptr() as usize;
        if offset >= self.capacity {
            return;
        }

        let idx = offset / MIN_BLOCK_SIZE;
        let state = self.state.borrow(token);

        let order = *state.orders[idx].get();
        self.push_free(state, order as usize, idx);
    }

    unsafe fn push_free(&self, state: &HeapState, order: usize, idx: usize) {
        let head_atomic = &state.free_heads[order];
        let base_ptr = self.memory.as_ptr();
        let node_ptr = base_ptr.add(idx * MIN_BLOCK_SIZE) as *mut AtomicUsize;

        let mut old_head = head_atomic.load(Ordering::Acquire);
        loop {
            let (_, old_tag) = unpack(old_head);
            (*node_ptr).store(old_head, Ordering::Relaxed);

            let new_head = pack(idx, old_tag.wrapping_add(1));

            match head_atomic.compare_exchange_weak(
                old_head,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire
            ) {
                Ok(_) => break,
                Err(actual) => old_head = actual,
            }
        }
    }

    /// Reallocates memory.
    pub unsafe fn realloc(
        &self,
        token: &GhostToken<'brand>,
        ptr: *mut u8,
        layout: Layout,
        new_size: usize,
    ) -> *mut u8 {
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc(token, new_layout);
        if !new_ptr.is_null() {
            core::ptr::copy_nonoverlapping(ptr, new_ptr, layout.size().min(new_size));
            self.dealloc(token, ptr, layout);
        }
        new_ptr
    }

    /// Allocates a value of type `T`.
    pub fn alloc_val<T>(&self, token: &GhostToken<'brand>, value: T) -> NonNull<T> {
        unsafe {
            let layout = Layout::new::<T>();
            let ptr = self.alloc(token, layout) as *mut T;
            if ptr.is_null() {
                 handle_alloc_error(layout);
            }
            ptr.write(value);
            NonNull::new_unchecked(ptr)
        }
    }

    /// Performs memory compaction (coalescing of free blocks).
    pub fn compact(&self, token: &mut GhostToken<'brand>) {
        let state = self.state.borrow_mut(token);

        let num_blocks = state.num_blocks;
        let mut is_free = vec![false; num_blocks];

        // Drain lists
        for k in 0..LEVELS {
            let head_atomic = &mut state.free_heads[k];
            let mut curr = unpack(*head_atomic.get_mut()).0;
            *head_atomic.get_mut() = pack(NONE, 0);

            while curr != NONE {
                is_free[curr] = true;
                let node_ptr = unsafe { self.memory.as_ptr().add(curr * MIN_BLOCK_SIZE) as *const AtomicUsize };
                let next_packed = unsafe { (*node_ptr).load(Ordering::Relaxed) };
                curr = unpack(next_packed).0;
            }
        }

        // Coalesce
        for k in 0..LEVELS - 1 {
             let size_k = 1 << k;
             let mut i = 0;
             while i < num_blocks {
                 if is_free[i] {
                      let order = unsafe { *state.orders[i].get() };
                      if order as usize == k {
                          let buddy = i ^ size_k;
                          if buddy < num_blocks && is_free[buddy] {
                               let buddy_order = unsafe { *state.orders[buddy].get() };
                               if buddy_order as usize == k {
                                   is_free[i] = false;
                                   is_free[buddy] = false;
                                   let merged = i & buddy;
                                   is_free[merged] = true;
                                   unsafe { *state.orders[merged].get() = (k + 1) as u8; }
                               }
                          }
                      }
                 }
                 i += size_k;
             }
        }

        // Re-populate
        for i in 0..num_blocks {
            if is_free[i] {
                let order = unsafe { *state.orders[i].get() as usize };
                if i % (1 << order) == 0 {
                     unsafe { self.push_free(state, order, i); }
                }
            }
        }
    }
}

impl<'brand> Default for BrandedHeap<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> Drop for BrandedHeap<'brand> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.capacity, 4096);
            dealloc(self.memory.as_ptr(), layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_heap_alloc() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::new();
            let layout = Layout::new::<u32>();

            unsafe {
                let ptr = heap.alloc(&token, layout) as *mut u32;
                assert!(!ptr.is_null());
                ptr.write(42);
                assert_eq!(*ptr, 42);
                heap.dealloc(&token, ptr as *mut u8, layout);
            }
        });
    }

    #[test]
    fn test_branded_heap_alloc_val() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::new();
            let ptr = heap.alloc_val(&token, 123u64);
            unsafe {
                assert_eq!(*ptr.as_ref(), 123);
                heap.dealloc(&token, ptr.as_ptr() as *mut u8, Layout::new::<u64>());
            }
        });
    }

    #[test]
    fn test_fragmentation_and_compact() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::with_capacity(16 * 1024);

            let layout = Layout::from_size_align(16, 16).unwrap();
            let p1 = unsafe { heap.alloc(&token, layout) };
            let p2 = unsafe { heap.alloc(&token, layout) };

            assert!(!p1.is_null());
            assert!(!p2.is_null());

            unsafe { heap.dealloc(&token, p1, layout); }
            unsafe { heap.dealloc(&token, p2, layout); }

            heap.compact(&mut token);

            let p3 = unsafe { heap.alloc(&token, layout) };
             assert!(!p3.is_null());
             unsafe { heap.dealloc(&token, p3, layout); }
        });
    }

    #[test]
    fn test_large_allocation() {
        GhostToken::new(|mut token| {
            // Heap large enough for max block
            let heap = BrandedHeap::with_capacity(64 * 1024 * 1024);
            // Alloc 1MB
            let layout = Layout::from_size_align(1024 * 1024, 4096).unwrap();
            let ptr = unsafe { heap.alloc(&token, layout) };
            assert!(!ptr.is_null());
            unsafe { heap.dealloc(&token, ptr, layout); }
        });
    }
}
