//! `ActiveVec` — a BrandedVec bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope. By holding the token exclusively, it can expose a standard `Vec`-like
//! API (push, pop, get, get_mut) without requiring the token as an argument for every call.
//!
//! It also supports slicing into `BrandedSliceMut`, enabling parallel mutation patterns.

use crate::GhostToken;
use super::BrandedVec;
use super::slice::{BrandedSlice, BrandedSliceMut};

/// A wrapper around a mutable reference to a `BrandedVec` and a mutable reference to a `GhostToken`.
///
/// This type acts as an "active handle" to the vector, allowing mutation and access without
/// repeatedly passing the token.
pub struct ActiveVec<'a, 'brand, T> {
    vec: &'a mut BrandedVec<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveVec<'a, 'brand, T> {
    /// Creates a new active vector handle.
    pub fn new(vec: &'a mut BrandedVec<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { vec, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// Pushes a new element.
    pub fn push(&mut self, value: T) {
        self.vec.push(value);
    }

    /// Pops the last element.
    pub fn pop(&mut self) -> Option<T> {
        self.vec.pop().map(|c| c.into_inner())
    }

    /// Returns a shared reference to element `idx`.
    pub fn get(&self, idx: usize) -> Option<&T> {
        self.vec.get(self.token, idx)
    }

    /// Returns a mutable reference to element `idx`.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        self.vec.get_mut(self.token, idx)
    }

    /// Returns a shared reference to element `idx`, panicking if out of bounds.
    pub fn borrow(&self, idx: usize) -> &T {
        self.vec.borrow(self.token, idx)
    }

    /// Returns a mutable reference to element `idx`, panicking if out of bounds.
    pub fn borrow_mut(&mut self, idx: usize) -> &mut T {
        self.vec.borrow_mut(self.token, idx)
    }

    /// Returns a mutable slice of the vector content.
    ///
    /// This returns a `BrandedSliceMut`, which allows parallel mutation via splitting.
    pub fn as_mut_slice(&mut self) -> BrandedSliceMut<'_, 'brand, T> {
        BrandedSliceMut::new(&mut self.vec.inner)
    }

    /// Returns a shared slice of the vector content.
    pub fn as_slice(&self) -> BrandedSlice<'_, 'brand, T> {
        BrandedSlice::new(&self.vec.inner, self.token)
    }

    /// Iterates over elements by mutable reference.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + use<'_, 'brand, T> {
        // We can delegate to BrandedSliceMut which implements efficient iteration
        self.as_mut_slice().into_iter()
    }

    /// Sorts the vector.
    pub fn sort(&mut self)
    where
        T: Ord,
    {
        self.as_mut_slice().sort();
    }
}

/// Extension trait to easily create ActiveVec from BrandedVec.
pub trait ActivateVec<'brand, T> {
    /// Activates the vector with the given token, returning a handle that bundles them.
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveVec<'a, 'brand, T>;
}

impl<'brand, T> ActivateVec<'brand, T> for BrandedVec<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveVec<'a, 'brand, T> {
        ActiveVec::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_vec_workflow() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();

            // Activate scope
            {
                let mut active = vec.activate(&mut token);
                active.push(10);
                active.push(20);
                active.push(30);

                assert_eq!(active.len(), 3);
                assert_eq!(*active.get(0).unwrap(), 10);

                *active.get_mut(1).unwrap() += 5; // 20 -> 25

                active.pop(); // Remove 30
            }

            // Token is released, can be used again
            assert_eq!(vec.len(), 2);
            assert_eq!(*vec.get(&token, 1).unwrap(), 25);
        });
    }

    #[test]
    fn test_active_vec_sort() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(3);
            vec.push(1);
            vec.push(2);

            vec.activate(&mut token).sort();

            assert_eq!(*vec.get(&token, 0).unwrap(), 1);
            assert_eq!(*vec.get(&token, 1).unwrap(), 2);
            assert_eq!(*vec.get(&token, 2).unwrap(), 3);
        });
    }
}
