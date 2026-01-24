//! `BrandedRc` — a branded reference-counted pointer.
//!
//! This smart pointer wraps `std::rc::Rc` but associates it with a specific brand,
//! allowing for token-gated Copy-On-Write (COW) semantics.

use crate::token::InvariantLifetime;
use std::ops::Deref;
use std::rc::Rc;

/// A branded reference-counted pointer.
///
/// This works like `Rc<T>`, but provides `make_mut` capabilities that
/// utilize the `GhostToken` to perform cloning if necessary.
#[derive(Debug)]
pub struct BrandedRc<'brand, T> {
    inner: Rc<T>,
    _brand: InvariantLifetime<'brand>,
}

impl<'brand, T> BrandedRc<'brand, T> {
    /// Constructs a new `BrandedRc`.
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(value),
            _brand: InvariantLifetime::default(),
        }
    }

    /// Returns a mutable reference to the inner value, cloning it if necessary.
    ///
    /// The `cloner` closure is used to clone the value if the reference count is greater than 1.
    /// This allows for cloning types that are not `Clone` (e.g., `BrandedVec`) by using the token.
    pub fn make_mut<F>(&mut self, cloner: F) -> &mut T
    where
        F: FnOnce(&T) -> T,
    {
        if Rc::get_mut(&mut self.inner).is_some() {
            return Rc::get_mut(&mut self.inner).unwrap();
        }

        // Clone required
        let new_val = cloner(&self.inner);
        self.inner = Rc::new(new_val);
        Rc::get_mut(&mut self.inner).unwrap()
    }

    /// Gets the number of strong pointers to this allocation.
    pub fn strong_count(&self) -> usize {
        Rc::strong_count(&self.inner)
    }
}

impl<'brand, T> Clone for BrandedRc<'brand, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _brand: InvariantLifetime::default(),
        }
    }
}

impl<'brand, T> Deref for BrandedRc<'brand, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'brand, T: Default> Default for BrandedRc<'brand, T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
