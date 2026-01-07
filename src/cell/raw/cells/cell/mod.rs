//! `GhostCell` (raw) — copy-based interior mutability branded by a ghost token.
//!
//! This is the foundational, minimal-overhead cell used by the raw layer.
//! All pointer/`MaybeUninit` unsafe operations are delegated to:
//! - `cell::raw::access` (centralized unsafe building blocks)
//! - `cells::cell::ops` (thin, audited per-type operations)

mod ops;

use core::mem::MaybeUninit;

use crate::GhostToken;
use crate::cell::raw::GhostUnsafeCell;

/// A cacheline-aligned copy-based interior mutable cell branded by a ghost token.
#[repr(align(64))]
pub struct GhostCell<'brand, T> {
    value: GhostUnsafeCell<'brand, MaybeUninit<T>>,
}

impl<'brand, T> GhostCell<'brand, T> {
    /// Creates a new cell containing the given value.
    #[inline(always)]
    pub fn new(value: T) -> Self {
        Self {
            value: GhostUnsafeCell::new(MaybeUninit::new(value)),
        }
    }
}

impl<'brand, T: Copy> GhostCell<'brand, T> {
    /// Returns a copy of the contained value.
    #[inline(always)]
    pub fn get(&self, token: &GhostToken<'brand>) -> T {
        ops::get_copy(&self.value, token)
    }

    /// Sets the contained value.
    #[inline(always)]
    pub fn set(&self, token: &mut GhostToken<'brand>, value: T) {
        ops::set_copy(&self.value, token, value);
    }

    /// Replaces the contained value, returning the old value.
    #[inline(always)]
    pub fn replace(&self, token: &mut GhostToken<'brand>, value: T) -> T {
        ops::replace_copy(&self.value, token, value)
    }

    /// Swaps the values of two cells.
    #[inline(always)]
    pub fn swap(&self, token: &mut GhostToken<'brand>, other: &Self) {
        ops::swap_slots(&self.value, &other.value, token)
    }
}

impl<'brand, T: Copy + Default> GhostCell<'brand, T> {
    /// Takes the value of the cell, leaving `Default::default()` in its place.
    #[inline(always)]
    pub fn take(&self, token: &mut GhostToken<'brand>) -> T {
        let old = ops::replace_copy(&self.value, token, T::default());
        old
    }
}

// SAFETY: access is token-gated via `GhostToken` / branded storage.
unsafe impl<'brand, T: Send> Send for GhostCell<'brand, T> {}
unsafe impl<'brand, T: Sync> Sync for GhostCell<'brand, T> {}

impl<'brand, T: Default> Default for GhostCell<'brand, T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<'brand, T> Drop for GhostCell<'brand, T> {
    fn drop(&mut self) {
        // SAFETY: constructed initialized, and we're in `drop`.
        unsafe { ops::drop_unchecked(&self.value) }
    }
}


