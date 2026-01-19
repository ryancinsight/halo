use core::alloc::Layout;
use core::ptr::NonNull;
use crate::GhostToken;
use super::allocator::{GhostAlloc, AllocError};
use super::heap::BrandedHeap;

/// A simple implementation of `GhostAlloc` that delegates to `BrandedHeap`.
///
/// This serves as the default allocator implementation for branded collections
/// when they need to use the `GhostAlloc` trait interface.
pub struct BrandedAllocator<'brand> {
    heap: BrandedHeap<'brand>,
}

impl<'brand> BrandedAllocator<'brand> {
    /// Creates a new branded allocator.
    pub const fn new() -> Self {
        Self {
            heap: BrandedHeap::new(),
        }
    }
}

impl<'brand> Default for BrandedAllocator<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> GhostAlloc<'brand> for BrandedAllocator<'brand> {
    fn allocate(&self, token: &mut GhostToken<'brand>, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        let ptr = unsafe { self.heap.alloc(token, layout) };
        NonNull::new(ptr).ok_or(AllocError)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // We need to conjure a token reference or change trait signature?
        // Wait, deallocate in GhostAlloc trait definition (which I should check) might need token.
        // Let's check src/alloc/allocator.rs
        // ...
        // `unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);`
        // It does NOT take a token. This is interesting.
        // If it doesn't take a token, how do we prove we have access to the brand?
        // `ptr` implies we had it. `&self` is branded.
        // BrandedHeap::dealloc expects a token.
        // If `GhostAlloc` trait doesn't require token for dealloc, then we might have a mismatch.
        // Let's read `src/alloc/allocator.rs` again to be sure.

        // BrandedHeap::dealloc signature: `pub unsafe fn dealloc(&self, _token: &mut GhostToken<'brand>, ...)`
        // If GhostAlloc::deallocate doesn't take token, we can't call BrandedHeap::dealloc if it *requires* one.
        // But BrandedHeap::dealloc just wraps `std::alloc::dealloc`. The token is phantom there.
        // However, strictly speaking, we need to satisfy the inner API.

        // If `GhostAlloc` is the trait we must implement, and it has no token in dealloc,
        // then `BrandedAllocator` should probably not require it for dealloc either,
        // OR `BrandedHeap` should rely on `&self` being branded.

        // Actually, for `std::alloc::dealloc`, we don't need a token.
        // The token is for *safety* (logic).
        // If the trait doesn't require it, we just call std::alloc::dealloc directly here.
        std::alloc::dealloc(ptr.as_ptr(), layout);
    }
}
