//! Unsafe, centralized operations on `MaybeUninit<T>` slots.
//!
//! The raw cell layer stores values as `MaybeUninit<T>` inside branded storage
//! (`GhostUnsafeCell`). These helpers provide a single place to audit:
//! - reads (`ptr::read`)
//! - writes (`ptr::write`)
//! - drops (`drop_in_place`)
//! - conversion to references (`assume_init_ref` / `assume_init_mut`)
//!
//! ## Core invariant
//! For all callers in this crate, the relevant slot is initialized *exactly when*:
//! - a constructor (`new`, `from_*`) completes, and
//! - after any mutating operation completes,
//! and it remains initialized until:
//! - `drop` begins for the owning cell.
//!
//! Callers must additionally ensure they uphold aliasing rules for any produced references.

use core::{mem::MaybeUninit, ptr};

/// Interprets an initialized slot as `&T`.
///
/// # Safety
/// - `slot` must be initialized.
/// - The returned `&T` must not be used concurrently with an outstanding `&mut T`
///   to the same location (normal Rust aliasing rules).
#[inline(always)]
pub(crate) unsafe fn assume_init_ref<'a, T>(slot: &'a MaybeUninit<T>) -> &'a T {
    // SAFETY: caller asserts `slot` is initialized.
    unsafe { slot.assume_init_ref() }
}

/// Interprets an initialized slot as `&mut T`.
///
/// # Safety
/// - `slot` must be initialized.
/// - The returned `&mut T` must be exclusive for its lifetime.
#[inline(always)]
pub(crate) unsafe fn assume_init_mut<'a, T>(slot: &'a mut MaybeUninit<T>) -> &'a mut T {
    // SAFETY: caller asserts `slot` is initialized and exclusive.
    unsafe { slot.assume_init_mut() }
}

/// Bitwise-moves an initialized value out of a slot.
///
/// # Safety
/// - `slot` must be initialized.
/// - The read must not cause double-drop; callers typically follow with `write_ptr`.
#[inline(always)]
pub(crate) unsafe fn read_ptr<T>(slot: *const MaybeUninit<T>) -> T {
    // SAFETY: caller asserts initialization + `ptr::read` contract.
    unsafe { ptr::read(slot.cast::<T>()) }
}

/// Writes a value into a slot (overwriting the prior bytes).
///
/// # Safety
/// - If the slot currently contains an initialized `T`, it must have been
///   previously `read_ptr`'d or otherwise logically moved out; otherwise this would leak/drop twice.
#[inline(always)]
pub(crate) unsafe fn write_ptr<T>(slot: *mut MaybeUninit<T>, value: T) {
    // SAFETY: caller upholds overwrite contract.
    unsafe { ptr::write(slot.cast::<T>(), value) }
}

/// Swaps two slots by bytes.
///
/// # Safety
/// - Both slots must be valid for reads/writes of `MaybeUninit<T>`.
/// - If callers rely on logical initialization, both must be initialized.
#[inline(always)]
pub(crate) unsafe fn swap_ptr<T>(a: *mut MaybeUninit<T>, b: *mut MaybeUninit<T>) {
    // SAFETY: caller ensures pointers are valid and non-overlapping or `ptr::swap`-safe.
    unsafe { ptr::swap(a, b) }
}

/// Drops an initialized value in place.
///
/// # Safety
/// - `slot` must be initialized.
/// - Must not be called more than once for the same logical value.
#[inline(always)]
pub(crate) unsafe fn drop_in_place_ptr<T>(slot: *mut MaybeUninit<T>) {
    // SAFETY: caller asserts initialization and drop uniqueness.
    unsafe { ptr::drop_in_place(slot.cast::<T>()) }
}



