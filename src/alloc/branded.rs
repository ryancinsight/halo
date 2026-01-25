use super::allocator::{AllocError, GhostAlloc};
use crate::GhostToken;
use core::alloc::Layout;
use core::ptr::NonNull;

// Note: BrandedHeap no longer supports GhostAlloc directly because it requires
// shared access (&GhostToken) which the single-threaded BuddyAllocator cannot provide safely.
// Users needing GhostAlloc should use BrandedSlab.
//
// However, BrandedAllocator is used as a default impl in some places.
// We must either update BrandedAllocator to use BrandedSlab, or remove GhostAlloc impl.
// Given BrandedSlab IS the recommended concurrent allocator, we switch BrandedAllocator to use it.
// Wait, BrandedAllocator wraps BrandedHeap. If BrandedHeap is Buddy, and Buddy is exclusive-only...
// Then BrandedAllocator cannot implement GhostAlloc backed by BrandedHeap.

// For now, we remove GhostAlloc impl for BrandedAllocator if it wraps BrandedHeap.
// Or we change BrandedAllocator to wrap BrandedSlab instead?
// But BrandedAllocator name implies general purpose.

// Let's modify BrandedAllocator to use BrandedSlab internally, as that's the "production" concurrent allocator.
// And BrandedHeap is the "isolated, manual" allocator.

use super::slab::BrandedSlab;

/// A simple implementation of `GhostAlloc` that delegates to `BrandedSlab`.
pub struct BrandedAllocator<'brand> {
    slab: BrandedSlab<'brand>,
}

impl<'brand> BrandedAllocator<'brand> {
    /// Creates a new branded allocator.
    pub fn new() -> Self {
        Self {
            slab: BrandedSlab::new(),
        }
    }
}

impl<'brand> GhostAlloc<'brand> for BrandedAllocator<'brand> {
    fn allocate(
        &self,
        token: &GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        self.slab.allocate(token, layout)
    }

    unsafe fn deallocate(
        &self,
        token: &GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        self.slab.deallocate(token, ptr, layout);
    }
}
