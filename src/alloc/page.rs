use core::alloc::Layout;
use std::alloc::{alloc, dealloc};

/// A trait for allocating and deallocating pages (4KB aligned/sized).
///
/// This allows `BrandedSlab` to be used with different backing stores,
/// such as the global system allocator (for library usage) or direct syscalls
/// (for the Halo system allocator).
pub trait PageAlloc {
    /// Allocates a page of memory.
    ///
    /// The returned pointer must be aligned to the page size (4096 bytes).
    /// The size of the allocation is determined by `layout.size()`.
    ///
    /// # Safety
    /// Caller must ensure layout is valid.
    unsafe fn alloc_page(&self, layout: Layout) -> *mut u8;

    /// Deallocates a page of memory.
    ///
    /// # Safety
    /// Caller must ensure ptr was allocated by this allocator with the given layout.
    unsafe fn dealloc_page(&self, ptr: *mut u8, layout: Layout);
}

/// A page allocator that uses the global system allocator.
#[derive(Default, Clone, Copy, Debug)]
pub struct GlobalPageAlloc;

impl PageAlloc for GlobalPageAlloc {
    unsafe fn alloc_page(&self, layout: Layout) -> *mut u8 {
        alloc(layout)
    }

    unsafe fn dealloc_page(&self, ptr: *mut u8, layout: Layout) {
        dealloc(ptr, layout)
    }
}
