//! `GhostLazyLock` — one-time lazy initialization (token-gated, no heap).
//!
//! Stores either an initializer `F: FnOnce() -> T` or the initialized value `T`
//! in-place (union), plus a small state byte.

mod inner;

use core::{
    mem::{ManuallyDrop, MaybeUninit},
    ptr,
};

use crate::cell::raw::access::ghost_unsafe_cell as guc;
use crate::{GhostToken, GhostUnsafeCell};
use inner::{Inner, State};

/// A token-gated one-shot lazy value.
pub struct GhostLazyLock<'brand, T, F = fn() -> T> {
    inner: GhostUnsafeCell<'brand, Inner<T, F>>,
}

impl<'brand, T, F> GhostLazyLock<'brand, T, F>
where
    F: FnOnce() -> T,
{
    /// Creates a new `GhostLazyLock` with the given initializer.
    pub fn new(init: F) -> Self {
        Self {
            inner: GhostUnsafeCell::new(Inner {
                slot: inner::Slot {
                    init: ManuallyDrop::new(init),
                },
                state: State::Uninit,
            }),
        }
    }

    /// Returns `true` if the value has been initialized.
    #[inline]
    pub fn is_initialized(&self, _token: &GhostToken<'brand>) -> bool {
        // SAFETY: read-only; state transitions are token-gated.
        self.inner.get(_token).state == State::Init
    }

    /// Gets the value, initializing it on first access.
    ///
    /// Requires `&mut GhostToken` because initialization mutates internal state.
    pub fn get<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a T {
        if !self.is_initialized(token.as_ref()) {
            self.init(token);
        }

        // SAFETY: state is `Init`, so `slot.value` is initialized.
        unsafe {
            let inner = self.inner.get(token.as_ref());
            debug_assert!(inner.state == State::Init);
            &*(&inner.slot.value as *const ManuallyDrop<T>).cast::<T>()
        }
    }

    /// Gets a mutable reference to the value, initializing it on first access.
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a mut T {
        if !self.is_initialized(token.as_ref()) {
            self.init(token);
        }

        // SAFETY: token provides exclusivity; state is `Init`.
        unsafe {
            let inner = self.inner.get_mut(token);
            debug_assert!(inner.state == State::Init);
            &mut *(&mut inner.slot.value as *mut ManuallyDrop<T>).cast::<T>()
        }
    }

    #[inline]
    fn init(&self, _token: &mut GhostToken<'brand>) {
        // SAFETY: `_token` proves exclusivity; we may safely mutate state/slot.
        let inner = self.inner.get_mut(_token);
        debug_assert!(inner.state == State::Uninit);

        // SAFETY: state is Uninit; init slot is initialized.
        let init = unsafe { ptr::read(&inner.slot.init) };
        let init = ManuallyDrop::into_inner(init);
        let value = init();
        inner.slot.value = ManuallyDrop::new(value);
        inner.state = State::Init;
    }
}

impl<'brand, T: Default> Default for GhostLazyLock<'brand, T, fn() -> T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

impl<'brand, T, F> Drop for GhostLazyLock<'brand, T, F> {
    fn drop(&mut self) {
        // SAFETY: in drop, we have exclusive access to `self`.
        unsafe {
            let inner = &mut *guc::as_mut_ptr_unchecked(&self.inner);
            match inner.state {
                State::Uninit => ManuallyDrop::drop(&mut inner.slot.init),
                State::Init => ManuallyDrop::drop(&mut inner.slot.value),
            }
        }
    }
}

// SAFETY: token-gated aliasing reasoning; no implicit synchronization.
unsafe impl<'brand, T: Send, F: Send> Send for GhostLazyLock<'brand, T, F> {}
unsafe impl<'brand, T: Sync, F: Sync> Sync for GhostLazyLock<'brand, T, F> {}

// Sanity check: we are a real type at compile time (and keep this module "used").
const _: () = {
    let _ = MaybeUninit::<GhostLazyLock<'static, u64, fn() -> u64>>::uninit();
};
