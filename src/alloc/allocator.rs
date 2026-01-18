use core::alloc::Layout;
use core::ptr::NonNull;

/// A trait for branded memory allocators.
///
/// This trait is similar to `std::alloc::Allocator` but is designed to work with
/// the `GhostCell` ecosystem. The allocated memory is tied to the `'brand` lifetime.
pub trait GhostAlloc<'brand> {
    /// Allocates memory according to the given layout.
    ///
    /// # Errors
    /// Returns `AllocError` if allocation fails.
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError>;

    /// Deallocates memory.
    ///
    /// # Safety
    /// `ptr` must denote a block of memory currently allocated by this allocator.
    /// `layout` must be the same layout that was used to allocate that block of memory.
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
}

/// The error type for allocation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocError;

impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("memory allocation failed")
    }
}

impl std::error::Error for AllocError {}
