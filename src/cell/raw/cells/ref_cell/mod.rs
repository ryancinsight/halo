//! `GhostRefCell` — runtime borrow checking + ghost branding.
//!
//! This is the raw (foundational) branded ref-cell primitive. Its only interior
//! mutation storage is [`GhostUnsafeCell`], and all low-level `MaybeUninit`/pointer
//! operations are centralized through `cell::raw::access`.

mod guards;

pub use guards::{Ref, RefMut};

use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicIsize, Ordering},
};

use crate::{GhostToken, GhostUnsafeCell};
use crate::cell::raw::access::maybe_uninit as mu;
use crate::cell::raw::access::ghost_unsafe_cell as guc;

/// A runtime borrow-checked cell branded by a ghost token.
#[repr(align(64))] // Cache line alignment for multi-threaded performance
pub struct GhostRefCell<'brand, T> {
    // Atomic borrow count: negative = writing, positive = reading, zero = free.
    pub(super) borrow: AtomicIsize,
    pub(super) value: GhostUnsafeCell<'brand, MaybeUninit<T>>,
}

impl<'brand, T> GhostRefCell<'brand, T> {
    /// Creates a new cell containing the given value.
    #[inline(always)]
    pub fn new(value: T) -> Self {
        Self {
            borrow: AtomicIsize::new(0),
            value: GhostUnsafeCell::new(MaybeUninit::new(value)),
        }
    }

    /// Returns `true` if the cell is currently borrowed.
    #[inline(always)]
    pub fn is_borrowed(&self, _token: &GhostToken<'brand>) -> bool {
        self.borrow.load(Ordering::Relaxed) != 0
    }

    /// Immutably borrows the wrapped value.
    ///
    /// # Panics
    /// Panics if the value is currently mutably borrowed.
    #[inline(always)]
    pub fn borrow<'a>(&'a self, _token: &'a GhostToken<'brand>) -> Ref<'brand, 'a, T> {
        let mut current = self.borrow.load(Ordering::Acquire);
        loop {
            if current < 0 {
                panic!("already mutably borrowed");
            }
            match self.borrow.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        Ref { cell: self }
    }

    /// Mutably borrows the wrapped value.
    ///
    /// # Panics
    /// Panics if the value is currently borrowed.
    #[inline(always)]
    pub fn borrow_mut<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> RefMut<'brand, 'a, T> {
        match self.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => RefMut { cell: self },
            Err(_) => panic!("already borrowed"),
        }
    }

    /// Attempts to immutably borrow the wrapped value.
    #[inline(always)]
    pub fn try_borrow<'a>(
        &'a self,
        _token: &'a GhostToken<'brand>,
    ) -> Option<Ref<'brand, 'a, T>> {
        let mut current = self.borrow.load(Ordering::Acquire);
        loop {
            if current < 0 {
                return None;
            }
            match self.borrow.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(Ref { cell: self }),
                Err(actual) => current = actual,
            }
        }
    }

    /// Attempts to mutably borrow the wrapped value.
    #[inline(always)]
    pub fn try_borrow_mut<'a>(
        &'a self,
        _token: &'a mut GhostToken<'brand>,
    ) -> Option<RefMut<'brand, 'a, T>> {
        match self.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => Some(RefMut { cell: self }),
            Err(_) => None,
        }
    }

    /// Replaces the wrapped value with a new one, returning the old value.
    ///
    /// # Panics
    /// Panics if the value is currently borrowed.
    #[inline(always)]
    pub fn replace(&self, _token: &mut GhostToken<'brand>, value: T) -> T {
        match self.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                let slot = unsafe { guc::as_mut_ptr_unchecked(&self.value) };
                let old = unsafe { mu::read_ptr(slot) };
                unsafe { mu::write_ptr(slot, value) };
                self.borrow.store(0, Ordering::Release);
                old
            }
            Err(_) => panic!("already borrowed"),
        }
    }

    /// Replaces the wrapped value with a new one computed from `f`,
    /// returning the old value.
    ///
    /// # Panics
    /// Panics if the value is currently borrowed.
    #[inline(always)]
    pub fn replace_with<F>(&self, _token: &mut GhostToken<'brand>, f: F) -> T
    where
        F: FnOnce(&mut T) -> T,
    {
        match self.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                let slot = unsafe { guc::as_mut_ptr_unchecked(&self.value) };
                let cur = unsafe { mu::assume_init_mut(&mut *slot) };
                let new_value = f(cur);
                // Returned "old" is the value currently in the slot (after `f` may have mutated it).
                let old = unsafe { ptr::read(cur) };
                unsafe { mu::write_ptr(slot, new_value) };
                self.borrow.store(0, Ordering::Release);
                old
            }
            Err(_) => panic!("already borrowed"),
        }
    }

    /// Swaps the wrapped value of `self` with the wrapped value of `other`.
    ///
    /// # Panics
    /// Panics if either value is currently borrowed.
    #[inline(always)]
    pub fn swap(&self, _token: &mut GhostToken<'brand>, other: &Self) {
        match (
            self.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire),
            other.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire),
        ) {
            (Ok(_), Ok(_)) => {
                let a = unsafe { guc::as_mut_ptr_unchecked(&self.value) };
                let b = unsafe { guc::as_mut_ptr_unchecked(&other.value) };
                unsafe { mu::swap_ptr(a, b) };
                self.borrow.store(0, Ordering::Release);
                other.borrow.store(0, Ordering::Release);
            }
            _ => panic!("already borrowed"),
        }
    }

    /// Takes the wrapped value, leaving `Default::default()` in its place.
    ///
    /// # Panics
    /// Panics if the value is currently borrowed.
    #[inline(always)]
    pub fn take(&self, _token: &mut GhostToken<'brand>) -> T
    where
        T: Default,
    {
        match self.borrow.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                let slot = unsafe { guc::as_mut_ptr_unchecked(&self.value) };
                let old = unsafe { mu::read_ptr(slot) };
                unsafe { mu::write_ptr(slot, T::default()) };
                self.borrow.store(0, Ordering::Release);
                old
            }
            Err(_) => panic!("already borrowed"),
        }
    }
}

impl<'brand, T> Drop for GhostRefCell<'brand, T> {
    fn drop(&mut self) {
        // SAFETY: `new` initializes the slot, and we are in `drop` so no concurrent access exists.
        unsafe { mu::drop_in_place_ptr(guc::as_mut_ptr_unchecked(&self.value)) }
    }
}

// SAFETY: borrow correctness is enforced via atomic borrow state; data is behind branded storage.
unsafe impl<'brand, T: Send> Send for GhostRefCell<'brand, T> {}
unsafe impl<'brand, T: Send + Sync> Sync for GhostRefCell<'brand, T> {}

impl<'brand, T: Default> Default for GhostRefCell<'brand, T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<'brand, T: Clone> Clone for GhostRefCell<'brand, T> {
    fn clone(&self) -> Self {
        panic!("GhostRefCell cannot be cloned without a token - use GhostToken::new() to create and clone")
    }
}

impl<'brand, T: PartialEq> PartialEq for GhostRefCell<'brand, T> {
    fn eq(&self, _other: &Self) -> bool {
        panic!("GhostRefCell cannot be compared without a token - use GhostToken::new() to access values")
    }
}

impl<'brand, T: Eq> Eq for GhostRefCell<'brand, T> {}

impl<'brand, T: core::fmt::Debug> core::fmt::Debug for GhostRefCell<'brand, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GhostRefCell")
            .field("value", &"<requires token>")
            .finish()
    }
}


