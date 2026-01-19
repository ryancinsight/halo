//! `GhostLazyCell` — recomputable lazy cache (token-gated, no heap).
//!
//! Unlike [`GhostLazyLock`](super::ghost_lazy_lock::GhostLazyLock), this retains
//! an initializer `F: Fn() -> T` so the cached value can be invalidated and recomputed.

mod inner;

use core::{
    mem::{ManuallyDrop, MaybeUninit},
    ptr,
};

use crate::cell::raw::access::ghost_unsafe_cell as guc;
use crate::{GhostToken, GhostUnsafeCell};
use inner::Inner;

/// A token-gated recomputable lazy cell.
pub struct GhostLazyCell<'brand, T, F = fn() -> T> {
    inner: GhostUnsafeCell<'brand, Inner<T, F>>,
}

impl<'brand, T, F> GhostLazyCell<'brand, T, F>
where
    F: Fn() -> T,
{
    /// Creates a new `GhostLazyCell` with a reusable initializer.
    pub fn new(init: F) -> Self {
        Self {
            inner: GhostUnsafeCell::new(Inner {
                init: ManuallyDrop::new(init),
                value: MaybeUninit::uninit(),
                is_init: false,
            }),
        }
    }

    /// Returns `true` if a value is currently cached.
    #[inline]
    pub fn is_initialized(&self, _token: &GhostToken<'brand>) -> bool {
        // SAFETY: read-only; mutation is gated by `&mut GhostToken`.
        self.inner.get(_token).is_init
    }

    /// Gets the cached value, computing and caching it if needed.
    pub fn get<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a T {
        if !self.is_initialized(token.as_ref()) {
            self.compute(token);
        }

        // SAFETY: value is initialized.
        unsafe {
            let inner = self.inner.get(token.as_ref());
            debug_assert!(inner.is_init);
            inner.value.assume_init_ref()
        }
    }

    /// Gets a mutable reference to the cached value, computing it if needed.
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a mut T {
        if !self.is_initialized(token.as_ref()) {
            self.compute(token);
        }

        // SAFETY: token provides exclusivity and value is initialized.
        unsafe {
            let inner = self.inner.get_mut(token);
            debug_assert!(inner.is_init);
            inner.value.assume_init_mut()
        }
    }

    /// Invalidates the cached value (dropping it) if present.
    pub fn invalidate(&self, _token: &mut GhostToken<'brand>) {
        // SAFETY: `_token` proves exclusivity; we can mutate/drop the value.
        let inner = self.inner.get_mut(_token);
        if inner.is_init {
            // SAFETY: initialized.
            unsafe { ptr::drop_in_place(inner.value.as_mut_ptr()) };
            inner.is_init = false;
        }
    }

    #[inline]
    fn compute(&self, _token: &mut GhostToken<'brand>) {
        // SAFETY: `_token` proves exclusivity.
        let inner = self.inner.get_mut(_token);
        debug_assert!(!inner.is_init);
        let value = (&*inner.init)();
        inner.value.write(value);
        inner.is_init = true;
    }
}

impl<'brand, T: Default> Default for GhostLazyCell<'brand, T, fn() -> T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

impl<'brand, T, F> Drop for GhostLazyCell<'brand, T, F> {
    fn drop(&mut self) {
        // SAFETY: in drop, we have exclusive access to `self`.
        unsafe {
            let inner = &mut *guc::as_mut_ptr_unchecked(&self.inner);
            if inner.is_init {
                ptr::drop_in_place(inner.value.as_mut_ptr());
            }
            ManuallyDrop::drop(&mut inner.init);
        }
    }
}

// SAFETY: token-gated aliasing reasoning; no implicit synchronization.
unsafe impl<'brand, T: Send, F: Send> Send for GhostLazyCell<'brand, T, F> {}
unsafe impl<'brand, T: Sync, F: Sync> Sync for GhostLazyCell<'brand, T, F> {}
