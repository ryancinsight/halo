//! `GhostCell` — safe interior mutability via branded tokens.
//!
//! This is the ergonomic, safe wrapper over [`GhostUnsafeCell`](crate::GhostUnsafeCell).
//! It is intentionally "thin": in optimized builds token arguments should optimize away,
//! yielding code close to raw `UnsafeCell` access while preserving aliasing invariants.
//!
//! Implementation is split into small submodules (see `ops_*` siblings).

use crate::cell::raw::GhostUnsafeCell;

/// A branded cell that can only be accessed using a token of the same brand.
pub struct GhostCell<'brand, T> {
    pub(super) inner: GhostUnsafeCell<'brand, T>,
}

impl<'brand, T> GhostCell<'brand, T> {
    /// Creates a new `GhostCell`.
    pub const fn new(value: T) -> Self {
        Self {
            inner: GhostUnsafeCell::new(value),
        }
    }
}

impl<'brand, T: Default> Default for GhostCell<'brand, T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<'brand, T> From<T> for GhostCell<'brand, T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

// SAFETY: same reasoning as `GhostUnsafeCell` — safe access is token-gated.
unsafe impl<'brand, T: Send> Send for GhostCell<'brand, T> {}
unsafe impl<'brand, T: Sync> Sync for GhostCell<'brand, T> {}


