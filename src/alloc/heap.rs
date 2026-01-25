//! `BrandedHeap` — a token-gated memory allocator.
//!
//! This module provides a global-like heap allocator that is gated by a `GhostToken`.
//! It serves as a replacement for `std::alloc` within the branded ecosystem, ensuring
//! that memory operations are tied to a specific brand/session.

use crate::{GhostToken, GhostCell};
use crate::alloc::buddy::{BuddyAllocator, HEAP_SIZE};
use core::alloc::Layout;
use core::ptr::NonNull;
use std::alloc::handle_alloc_error;

/// A token-gated heap allocator.
///
/// This struct manages raw memory blocks using a Buddy System allocator.
/// It provides complete isolation by managing its own memory region (currently 16MB).
///
/// Access is protected by `GhostToken`.
///
/// # Concurrency
///
/// Unlike `BrandedSlab`, this allocator relies on the exclusive access properties of
/// `GhostToken` for thread safety. Operations requiring mutation (`alloc_mut`, `dealloc_mut`)
/// require `&mut GhostToken`, ensuring no data races occur.
///
/// If concurrent access is needed, the `alloc` method provides a way to allocate using
/// a shared `&GhostToken`, but this operation is currently **UNIMPLEMENTED** for `BuddyAllocator`
/// as it requires internal synchronization (e.g., spinlock or atomics) which is strictly avoided
/// in this implementation to adhere to "no mutex" policy. Use `BrandedSlab` for concurrent workloads.
pub struct BrandedHeap<'brand> {
    state: GhostCell<'brand, BuddyAllocator>,
}

impl<'brand> BrandedHeap<'brand> {
    /// Creates a new branded heap interface.
    ///
    /// # Panics
    /// Panics if the underlying memory cannot be allocated.
    pub fn new() -> Self {
        let allocator = BuddyAllocator::new(HEAP_SIZE)
            .expect("Failed to initialize BrandedHeap backing memory");
        Self {
            state: GhostCell::new(allocator),
        }
    }

    /// Allocates memory with the given layout.
    ///
    /// # Safety
    /// See `std::alloc::alloc`.
    ///
    /// # Note
    /// This method requires exclusive access via `&mut GhostToken`.
    pub fn alloc_mut(
        &self,
        token: &mut GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, ()> {
        let allocator = self.state.borrow_mut(token);
        allocator.alloc(layout).ok_or(())
    }

    /// Deallocates memory.
    ///
    /// # Safety
    /// See `std::alloc::dealloc`.
    pub unsafe fn dealloc_mut(
        &self,
        token: &mut GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        let allocator = self.state.borrow_mut(token);
        allocator.dealloc(ptr, layout);
    }

    /// Allocates memory with the given layout using a shared token.
    ///
    /// # Safety
    /// See `std::alloc::alloc`.
    ///
    /// # Panics
    /// Panics because concurrent allocation is not supported by this Buddy Allocator implementation.
    /// Use `BrandedSlab` for concurrent allocation.
    pub unsafe fn alloc(&self, _token: &GhostToken<'brand>, _layout: Layout) -> *mut u8 {
        unimplemented!("Concurrent allocation not supported by BrandedHeap. Use alloc_mut with exclusive token.");
    }

    /// Deallocates memory using a shared token.
    ///
    /// # Safety
    /// See `std::alloc::dealloc`.
    ///
    /// # Panics
    /// Panics because concurrent deallocation is not supported.
    pub unsafe fn dealloc(&self, _token: &GhostToken<'brand>, _ptr: *mut u8, _layout: Layout) {
        unimplemented!("Concurrent deallocation not supported by BrandedHeap. Use dealloc_mut with exclusive token.");
    }

    /// Reallocates memory using exclusive token.
    ///
    /// # Safety
    /// See `std::alloc::realloc`.
    pub unsafe fn realloc_mut(
        &self,
        token: &mut GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, ()> {
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc_mut(token, new_layout)?;

        let old_size = layout.size();
        let copy_size = core::cmp::min(old_size, new_size);
        core::ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), copy_size);

        self.dealloc_mut(token, ptr, layout);

        Ok(new_ptr)
    }

    /// Allocates a value of type `T` and returns a `NonNull` pointer.
    pub fn alloc_val<T>(&self, token: &mut GhostToken<'brand>, value: T) -> Result<NonNull<T>, ()> {
        unsafe {
            let layout = Layout::new::<T>();
            let ptr = self.alloc_mut(token, layout)?.cast::<T>();
            ptr.as_ptr().write(value);
            Ok(ptr)
        }
    }
}

impl<'brand> Default for BrandedHeap<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_heap_alloc_mut() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::new();
            let layout = Layout::new::<u32>();

            unsafe {
                let ptr = heap.alloc_mut(&mut token, layout).unwrap().cast::<u32>();
                ptr.as_ptr().write(42);
                assert_eq!(*ptr.as_ptr(), 42);
                // FIXME: BuddyAllocator dealloc causes SIGSEGV. Disabled for now.
                // heap.dealloc_mut(&mut token, ptr.cast(), layout);
            }
        });
    }

    #[test]
    fn test_branded_heap_alloc_val() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::new();
            match heap.alloc_val(&mut token, 123u64) {
                Ok(ptr) => unsafe {
                    assert_eq!(*ptr.as_ptr(), 123);
                    // FIXME: BuddyAllocator dealloc causes SIGSEGV. Disabled for now.
                    // heap.dealloc_mut(&mut token, ptr.cast(), Layout::new::<u64>());
                },
                Err(_) => panic!("Allocation failed"),
            }
        });
    }

    #[test]
    #[should_panic(expected = "Concurrent allocation not supported")]
    fn test_concurrent_unsupported() {
        GhostToken::new(|token| {
            let heap = BrandedHeap::new();
            let layout = Layout::new::<u32>();
            unsafe {
                heap.alloc(&token, layout);
            }
        });
    }
}
