// use crate::GhostToken;
use crate::token::traits::GhostBorrow;
use core::alloc::Layout;
use core::ptr::NonNull;

/// A trait for branded memory allocators.
///
/// This trait is similar to `std::alloc::Allocator` but is designed to work with
/// the `GhostCell` ecosystem. The allocated memory is tied to the `'brand` lifetime.
///
/// This trait requires the allocator to be thread-safe (`Sync`) and support
/// concurrent allocation via a shared token (implementing `GhostBorrow`).
pub trait GhostAlloc<'brand>: Sync {
    /// Allocates memory according to the given layout.
    ///
    /// # Errors
    /// Returns `AllocError` if allocation fails.
    fn allocate(
        &self,
        token: &impl GhostBorrow<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError>;

    /// Allocates memory with a shard hint.
    ///
    /// This allows the caller to provide a hint (e.g., a thread-specific hash or index)
    /// to help the allocator select a specific shard or resource pool without needing
    /// to perform expensive thread-local storage lookups or hashing internally.
    ///
    /// The hint is advisory; the allocator may ignore it or normalize it.
    fn allocate_in(
        &self,
        token: &impl GhostBorrow<'brand>,
        layout: Layout,
        _shard_hint: Option<usize>,
    ) -> Result<NonNull<u8>, AllocError> {
        self.allocate(token, layout)
    }

    /// Deallocates memory.
    ///
    /// # Safety
    /// `ptr` must denote a block of memory currently allocated by this allocator.
    /// `layout` must be the same layout that was used to allocate that block of memory.
    unsafe fn deallocate(
        &self,
        token: &impl GhostBorrow<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    );
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
