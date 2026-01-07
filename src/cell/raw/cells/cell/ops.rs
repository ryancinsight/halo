use core::mem::MaybeUninit;

use crate::{GhostToken, GhostUnsafeCell};
use crate::cell::raw::access::ghost_unsafe_cell as guc;
use crate::cell::raw::access::maybe_uninit as mu;

/// Reads an initialized `T: Copy` out of a branded slot.
#[inline(always)]
pub(super) fn get_copy<'brand, T: Copy>(cell: &GhostUnsafeCell<'brand, MaybeUninit<T>>, token: &GhostToken<'brand>) -> T {
    // SAFETY: initialized in constructors; mutation requires exclusive token elsewhere.
    unsafe { *mu::assume_init_ref(cell.get(token)) }
}

/// Replaces the value in a branded slot (returns old).
#[inline(always)]
pub(super) fn replace_copy<'brand, T: Copy>(
    cell: &GhostUnsafeCell<'brand, MaybeUninit<T>>,
    token: &mut GhostToken<'brand>,
    value: T,
) -> T {
    let slot = cell.as_mut_ptr(token);
    // SAFETY: exclusive via `&mut GhostToken`, slot initialized.
    unsafe {
        let old = mu::read_ptr(slot);
        mu::write_ptr(slot, value);
        old
    }
}

/// Writes a value into a branded slot (overwriting).
#[inline(always)]
pub(super) fn set_copy<'brand, T: Copy>(
    cell: &GhostUnsafeCell<'brand, MaybeUninit<T>>,
    token: &mut GhostToken<'brand>,
    value: T,
) {
    *cell.get_mut(token) = MaybeUninit::new(value)
}

/// Swaps two branded slots.
#[inline(always)]
pub(super) fn swap_slots<'brand, T>(
    a: &GhostUnsafeCell<'brand, MaybeUninit<T>>,
    b: &GhostUnsafeCell<'brand, MaybeUninit<T>>,
    token: &mut GhostToken<'brand>,
) {
    let ap = a.as_mut_ptr(token);
    let bp = b.as_mut_ptr(token);
    // SAFETY: exclusive via `&mut GhostToken`.
    unsafe { mu::swap_ptr(ap, bp) }
}

/// Drops an initialized `T` in a branded slot without requiring a token.
///
/// # Safety
/// - Slot must be initialized.
/// - Must not be called more than once for the same logical value.
#[inline(always)]
pub(super) unsafe fn drop_unchecked<'brand, T>(cell: &GhostUnsafeCell<'brand, MaybeUninit<T>>) {
    let slot = unsafe { guc::as_mut_ptr_unchecked(cell) };
    unsafe { mu::drop_in_place_ptr(slot) }
}


