//! `GhostCell` — safe interior mutability via branded tokens.
//!
//! This is the ergonomic, safe wrapper over [`GhostUnsafeCell`](crate::GhostUnsafeCell).
//! It is intentionally "thin": in optimized builds token arguments should optimize away,
//! yielding code close to raw `UnsafeCell` access while preserving aliasing invariants.

use crate::cell::raw::GhostUnsafeCell;
use crate::token::traits::{GhostBorrow, GhostBorrowMut};
// use crate::GhostToken;
use core::ptr;

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

    /// Returns a reference to the contained value.
    ///
    /// Requires a token with read permission (implementing `GhostBorrow`).
    #[inline(always)]
    pub fn borrow<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> &'a T {
        self.inner.get(token)
    }

    /// Returns a mutable reference to the contained value.
    ///
    /// Requires a token with write permission (implementing `GhostBorrowMut`).
    #[inline(always)]
    pub fn borrow_mut<'a>(&'a self, token: &'a mut impl GhostBorrowMut<'brand>) -> &'a mut T {
        self.inner.get_mut(token)
    }

    /// Returns a mutable reference to the contained value without requiring a token.
    ///
    /// This is safe because `&mut self` guarantees exclusive access to the cell,
    /// so no other references (token-gated or otherwise) can exist.
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut_exclusive()
    }

    /// Replaces the contained value, returning the old value.
    #[inline]
    pub fn replace(&self, token: &mut impl GhostBorrowMut<'brand>, value: T) -> T {
        self.inner.replace(value, token)
    }

    /// Swaps the values of two `GhostCell`s.
    #[inline]
    pub fn swap(&self, token: &mut impl GhostBorrowMut<'brand>, other: &Self) {
        let a = self.inner.as_mut_ptr(token);
        let b = other.inner.as_mut_ptr(token);

        // SAFETY:
        // - `token` is a linear capability, so safe code cannot concurrently access
        //   either cell mutably.
        // - `ptr::swap` is safe for possibly-equal pointers.
        unsafe { ptr::swap(a, b) };
    }

    /// Returns a raw pointer to the contained value.
    #[inline(always)]
    pub fn as_ptr(&self, token: &impl GhostBorrow<'brand>) -> *const T {
        self.inner.as_ptr(token)
    }

    /// Returns a raw mutable pointer to the contained value.
    #[inline(always)]
    pub fn as_mut_ptr(&self, token: &mut impl GhostBorrowMut<'brand>) -> *mut T {
        self.inner.as_mut_ptr(token)
    }

    /// Applies a function to the shared borrow and returns its result.
    #[inline]
    pub fn apply<F, R>(&self, token: &impl GhostBorrow<'brand>, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(self.borrow(token))
    }

    /// Applies a function to the mutable borrow and returns its result.
    #[inline]
    pub fn apply_mut<F, R>(&self, token: &mut impl GhostBorrowMut<'brand>, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        f(self.borrow_mut(token))
    }

    /// Mutates the contained value using `f`.
    #[inline]
    pub fn update<F>(&self, token: &mut impl GhostBorrowMut<'brand>, f: F)
    where
        F: FnOnce(&mut T),
    {
        f(self.borrow_mut(token));
    }

    /// Maps the cell's value into a new `GhostCell` of the same brand.
    #[inline]
    pub fn map<F, U>(&self, token: &impl GhostBorrow<'brand>, f: F) -> GhostCell<'brand, U>
    where
        F: FnOnce(&T) -> U,
    {
        GhostCell::new(f(self.borrow(token)))
    }
}

impl<'brand, T: Copy> GhostCell<'brand, T> {
    /// Copies the contained value.
    #[inline(always)]
    pub fn get(&self, token: &impl GhostBorrow<'brand>) -> T {
        *self.borrow(token)
    }

    /// Overwrites the contained value.
    #[inline(always)]
    pub fn set(&self, token: &mut impl GhostBorrowMut<'brand>, value: T) {
        *self.borrow_mut(token) = value;
    }
}

impl<'brand, T: Clone> GhostCell<'brand, T> {
    /// Clones the contained value.
    #[inline]
    pub fn cloned(&self, token: &impl GhostBorrow<'brand>) -> T {
        self.borrow(token).clone()
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
