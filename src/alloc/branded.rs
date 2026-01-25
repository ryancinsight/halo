use super::allocator::{AllocError, GhostAlloc};
use super::heap::BrandedHeap;
use crate::GhostToken;
use core::alloc::Layout;
use core::ptr::NonNull;

/// A simple implementation of `GhostAlloc` that delegates to `BrandedHeap`.
///
/// This serves as the default allocator implementation for branded collections
/// when they need to use the `GhostAlloc` trait interface.
pub struct BrandedAllocator<'brand> {
    heap: BrandedHeap<'brand>,
}

impl<'brand> BrandedAllocator<'brand> {
    /// Creates a new branded allocator.
    pub fn new() -> Self {
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
    fn allocate(
        &self,
        token: &GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        let ptr = unsafe { self.heap.alloc(token, layout) };
        NonNull::new(ptr).ok_or(AllocError)
    }

    unsafe fn deallocate(
        &self,
        token: &GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        self.heap.dealloc(token, ptr.as_ptr(), layout);
    }
}
