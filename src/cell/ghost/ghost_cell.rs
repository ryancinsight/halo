//! `GhostCell` — safe interior mutability via branded tokens.
//!
//! This is the ergonomic, safe wrapper over [`GhostUnsafeCell`](crate::GhostUnsafeCell).
//! It is intentionally "thin": in optimized builds token arguments should optimize away,
//! yielding code close to raw `UnsafeCell` access while preserving aliasing invariants.
//!
//! Implementation is split into small submodules (see `ops_*` siblings).

use crate::cell::raw::GhostUnsafeCell;

/// A branded cell that can only be accessed using a token of the same brand.
#[repr(transparent)]
#[derive(Debug)]
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

    /// Consumes the cell and returns the contained value.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// This call borrows `GhostCell` mutably (at compile-time) which guarantees
    /// that we possess the only reference.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut_exclusive()
    }

    /// Returns a raw pointer to the contained value.
    pub fn as_ptr(&self, token: &crate::GhostToken<'brand>) -> *const T {
        self.inner.as_ptr(token)
    }

    /// Returns a raw mutable pointer to the contained value.
    pub fn as_mut_ptr(&self, token: &mut crate::GhostToken<'brand>) -> *mut T {
        self.inner.as_mut_ptr(token)
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

#[cfg(feature = "proptest")]
impl<'brand, T: proptest::arbitrary::Arbitrary> proptest::arbitrary::Arbitrary
    for GhostCell<'brand, T>
{
    type Parameters = T::Parameters;
    type Strategy = proptest::strategy::Map<T::Strategy, fn(T) -> Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy;
        T::arbitrary_with(args).prop_map(GhostCell::new)
    }
}
