//! `BrandedHeap` — a token-gated memory allocator.
//!
//! This module provides a global-like heap allocator that is gated by a `GhostToken`.
//! It serves as a replacement for `std::alloc` within the branded ecosystem, ensuring
//! that memory operations are tied to a specific brand/session.

use crate::GhostToken;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use std::alloc::{alloc, dealloc, handle_alloc_error, realloc};

/// A token-gated heap allocator.
///
/// This struct doesn't own the memory directly (it delegates to the global allocator),
/// but it enforces that allocations and deallocations are performed with the correct token.
///
/// In a more advanced "ground up" implementation, this would manage raw memory blocks
/// (e.g., via mmap) and implement a slab/buddy allocator. For now, it wraps `std::alloc`
/// to establish the API pattern.
///
/// TODO: Implement a true heap manager (buddy system or slab) instead of wrapping `std::alloc`.
/// This would allow for complete isolation and potentially better performance for specific workloads.
pub struct BrandedHeap<'brand> {
    // Phantom data to tie this heap instance to the brand lifetime.
    // In a real implementation, this might hold state like a list of allocated blocks
    // to prevent leaks or use-after-free by validating pointers against the brand.
    _marker: core::marker::PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> BrandedHeap<'brand> {
    /// Creates a new branded heap interface.
    pub const fn new() -> Self {
        Self {
            _marker: core::marker::PhantomData,
        }
    }

    /// Allocates memory with the given layout.
    ///
    /// Requires exclusive access to the token (simulating locking the heap or validating permission).
    ///
    /// # Safety
    /// See `std::alloc::alloc`.
    pub unsafe fn alloc(&self, _token: &mut GhostToken<'brand>, layout: Layout) -> *mut u8 {
        let ptr = alloc(layout);
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        ptr
    }

    /// Deallocates memory.
    ///
    /// # Safety
    /// See `std::alloc::dealloc`.
    pub unsafe fn dealloc(&self, _token: &mut GhostToken<'brand>, ptr: *mut u8, layout: Layout) {
        dealloc(ptr, layout);
    }

    /// Reallocates memory.
    ///
    /// # Safety
    /// See `std::alloc::realloc`.
    pub unsafe fn realloc(
        &self,
        _token: &mut GhostToken<'brand>,
        ptr: *mut u8,
        layout: Layout,
        new_size: usize,
    ) -> *mut u8 {
        let new_ptr = realloc(ptr, layout, new_size);
        if new_ptr.is_null() {
            // Layout for realloc error handling is tricky, std handles it usually.
            // We'll just construct a layout for the error.
            let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
            handle_alloc_error(new_layout);
        }
        new_ptr
    }

    /// Allocates a value of type `T` and returns a `NonNull` pointer.
    pub fn alloc_val<T>(&self, token: &mut GhostToken<'brand>, value: T) -> NonNull<T> {
        unsafe {
            let layout = Layout::new::<T>();
            let ptr = self.alloc(token, layout) as *mut T;
            ptr.write(value);
            NonNull::new_unchecked(ptr)
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
    fn test_branded_heap_alloc() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::new();
            let layout = Layout::new::<u32>();

            unsafe {
                let ptr = heap.alloc(&mut token, layout) as *mut u32;
                ptr.write(42);
                assert_eq!(*ptr, 42);
                heap.dealloc(&mut token, ptr as *mut u8, layout);
            }
        });
    }

    #[test]
    fn test_branded_heap_alloc_val() {
        GhostToken::new(|mut token| {
            let heap = BrandedHeap::new();
            let ptr = heap.alloc_val(&mut token, 123u64);
            unsafe {
                assert_eq!(*ptr.as_ref(), 123);
                heap.dealloc(&mut token, ptr.as_ptr() as *mut u8, Layout::new::<u64>());
            }
        });
    }
}
