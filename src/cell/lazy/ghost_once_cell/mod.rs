//! `GhostOnceCell` — one-time set + many reads (token-gated, no heap).

mod inner;

use core::{
    mem::MaybeUninit,
    ptr,
};

use crate::{GhostToken, GhostUnsafeCell};
use crate::cell::raw::access::ghost_unsafe_cell as guc;
use inner::Inner;

/// A token-gated once cell: can be set once, then read many times.
pub struct GhostOnceCell<'brand, T> {
    inner: GhostUnsafeCell<'brand, Inner<T>>,
}

impl<'brand, T> GhostOnceCell<'brand, T> {
    /// Creates a new empty `GhostOnceCell`.
    pub const fn new() -> Self {
        Self {
            inner: GhostUnsafeCell::new(Inner {
                value: MaybeUninit::uninit(),
                is_init: false,
            }),
        }
    }

    /// Returns `true` if the cell has been initialized.
    #[inline(always)]
    pub fn is_initialized(&self, _token: &GhostToken<'brand>) -> bool {
        // SAFETY: read-only; mutations require `&mut GhostToken`.
        self.inner.get(_token).is_init
    }

    /// Gets a shared reference to the value if initialized.
    #[inline(always)]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        let inner = self.inner.get(token);
        if !inner.is_init {
            return None;
        }
        // SAFETY: `is_init` is true.
        unsafe { Some(inner.value.assume_init_ref()) }
    }

    /// Gets an exclusive reference to the value if initialized.
    #[inline]
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> Option<&'a mut T> {
        let inner = self.inner.get_mut(token);
        if inner.is_init {
            // SAFETY: token provides exclusivity; `is_init` is true.
            unsafe { Some(inner.value.assume_init_mut()) }
        } else {
            None
        }
    }

    /// Sets the value if uninitialized.
    ///
    /// Returns `Ok(())` if the value was set, or `Err(value)` if it was already set.
    #[inline]
    pub fn set(&self, _token: &mut GhostToken<'brand>, value: T) -> Result<(), T> {
        // SAFETY: `_token` proves exclusive access; we may mutate the cell.
        let inner = self.inner.get_mut(_token);
        if inner.is_init {
            return Err(value);
        }
        inner.value.write(value);
        inner.is_init = true;
        Ok(())
    }

    /// Gets the value, initializing with `init` if needed.
    pub fn get_or_init<'a, F>(&'a self, token: &'a mut GhostToken<'brand>, init: F) -> &'a T
    where
        F: FnOnce() -> T,
    {
        // Use only the exclusive-token path to avoid borrowing `token` immutably
        // and then mutably for the full `'a` lifetime.
        let inner = self.inner.get_mut(token);
        if !inner.is_init {
            inner.value.write(init());
            inner.is_init = true;
        }
        // SAFETY: initialized.
        unsafe { inner.value.assume_init_ref() }
    }

    /// Takes the value out, leaving the cell uninitialized.
    pub fn take(&self, _token: &mut GhostToken<'brand>) -> Option<T> {
        // SAFETY: `_token` proves exclusivity; we may move out and update state.
        let inner = self.inner.get_mut(_token);
        if !inner.is_init {
            return None;
        }
        // SAFETY: initialized.
        let value = unsafe { ptr::read(inner.value.as_ptr()) };
        inner.is_init = false;
        Some(value)
    }
}

impl<'brand, T> Default for GhostOnceCell<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T> Drop for GhostOnceCell<'brand, T> {
    fn drop(&mut self) {
        // SAFETY: in drop we have exclusive access to `self`.
        unsafe {
            let inner = &mut *guc::as_mut_ptr_unchecked(&self.inner);
            if inner.is_init {
                ptr::drop_in_place(inner.value.as_mut_ptr());
            }
        }
    }
}

// SAFETY: token-gated aliasing reasoning; no implicit synchronization.
unsafe impl<'brand, T: Send> Send for GhostOnceCell<'brand, T> {}
unsafe impl<'brand, T: Sync> Sync for GhostOnceCell<'brand, T> {}


