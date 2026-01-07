//! Centralized access to `GhostUnsafeCell` unchecked raw pointers.
//!
//! Higher layers should prefer going through this module rather than calling
//! `GhostUnsafeCell::as_mut_ptr_unchecked()` directly. This keeps all unchecked
//! interior-mutation entry points discoverable in one place.

use crate::cell::raw::GhostUnsafeCell;

/// Returns a raw mutable pointer to the cell contents without requiring a token.
///
/// # Safety
/// - Same safety contract as [`GhostUnsafeCell::as_mut_ptr_unchecked`].
/// - The returned pointer must be treated as a raw pointer: dereferencing and/or
///   writing through it is `unsafe` and must uphold aliasing and initialization rules.
#[inline(always)]
pub(crate) unsafe fn as_mut_ptr_unchecked<'brand, T: ?Sized>(
    cell: &GhostUnsafeCell<'brand, T>,
) -> *mut T {
    cell.as_mut_ptr_unchecked()
}



