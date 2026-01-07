use core::mem::MaybeUninit;

use crate::cell::raw::access::maybe_uninit as mu;
use crate::cell::raw::access::ghost_unsafe_cell as guc;

use super::GhostRefCell;

/// Immutable borrow guard for [`GhostRefCell`].
pub struct Ref<'brand, 'cell, T> {
    pub(super) cell: &'cell GhostRefCell<'brand, T>,
}

impl<'brand, 'cell, T> core::ops::Deref for Ref<'brand, 'cell, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        // SAFETY:
        // - `Ref` exists only after incrementing the reader count.
        // - While reader count > 0, no writer can obtain `RefMut` (it requires transitioning 0 -> -1).
        // - `value` is initialized in `new` and only written while holding the exclusive writer state.
        let slot: *mut MaybeUninit<T> = unsafe { guc::as_mut_ptr_unchecked(&self.cell.value) };
        unsafe { mu::assume_init_ref(&*slot) }
    }
}

impl<'brand, 'cell, T> Drop for Ref<'brand, 'cell, T> {
    fn drop(&mut self) {
        // Decrement reader count.
        let prev = self.cell.borrow.fetch_sub(1, core::sync::atomic::Ordering::Release);
        debug_assert!(prev > 0, "Borrow count underflow");
    }
}

/// Mutable borrow guard for [`GhostRefCell`].
pub struct RefMut<'brand, 'cell, T> {
    pub(super) cell: &'cell GhostRefCell<'brand, T>,
}

impl<'brand, 'cell, T> core::ops::Deref for RefMut<'brand, 'cell, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        // SAFETY: `RefMut` exists only after transitioning borrow state 0 -> -1 (exclusive).
        let slot: *mut MaybeUninit<T> = unsafe { guc::as_mut_ptr_unchecked(&self.cell.value) };
        unsafe { mu::assume_init_ref(&*slot) }
    }
}

impl<'brand, 'cell, T> core::ops::DerefMut for RefMut<'brand, 'cell, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: `RefMut` exists only after transitioning borrow state 0 -> -1 (exclusive).
        let slot: *mut MaybeUninit<T> = unsafe { guc::as_mut_ptr_unchecked(&self.cell.value) };
        unsafe { mu::assume_init_mut(&mut *slot) }
    }
}

impl<'brand, 'cell, T> Drop for RefMut<'brand, 'cell, T> {
    fn drop(&mut self) {
        // Clear writer flag.
        let prev = self.cell.borrow.fetch_add(1, core::sync::atomic::Ordering::Release);
        debug_assert_eq!(prev, -1, "Expected writer borrow count");
    }
}


