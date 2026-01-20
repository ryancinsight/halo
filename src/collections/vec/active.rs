//! `ActiveVec` — a BrandedVec bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope. By holding the token exclusively, it can expose a standard `Vec`-like
//! API (push, pop, get, get_mut) without requiring the token as an argument for every call.
//!
//! It also supports slicing into `BrandedSliceMut`, enabling parallel mutation patterns.

use super::slice::{BrandedSlice, BrandedSliceMut};
use super::{BrandedVec, BrandedVecDeque};
use crate::GhostToken;
use std::slice;

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
        self.vec.pop()
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
        BrandedSliceMut::new(self.vec.as_mut_slice(self.token))
    }

    /// Returns a shared slice of the vector content.
    pub fn as_slice(&self) -> BrandedSlice<'_, 'brand, T> {
        BrandedSlice::new(self.vec.as_slice(self.token), self.token)
    }

    /// Returns the underlying slice as a standard `&[T]`.
    #[inline(always)]
    pub fn as_native_slice(&self) -> &[T] {
        self.as_slice().into_slice()
    }

    /// Returns the underlying mutable slice as a standard `&mut [T]`.
    #[inline(always)]
    pub fn as_native_mut_slice(&mut self) -> &mut [T] {
        self.as_mut_slice().into_mut_slice()
    }

    /// Iterates over elements by shared reference.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.as_slice().into_slice().iter()
    }

    /// Iterates over elements by mutable reference.
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, T> {
        self.as_mut_slice().into_mut_slice().iter_mut()
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

/// A wrapper around a mutable reference to a `BrandedVecDeque` and a mutable reference to a `GhostToken`.
pub struct ActiveVecDeque<'a, 'brand, T> {
    deque: &'a mut BrandedVecDeque<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveVecDeque<'a, 'brand, T> {
    /// Creates a new active deque handle.
    pub fn new(
        deque: &'a mut BrandedVecDeque<'brand, T>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { deque, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.deque.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }

    /// Clears the deque.
    pub fn clear(&mut self) {
        self.deque.clear();
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) {
        self.deque.push_back(value);
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) {
        self.deque.push_front(value);
    }

    /// Pops from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.deque.pop_back()
    }

    /// Pops from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.deque.pop_front()
    }

    /// Returns a shared reference to the element at `idx`.
    pub fn get(&self, idx: usize) -> Option<&T> {
        self.deque.get(self.token, idx)
    }

    /// Returns a mutable reference to the element at `idx`.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        self.deque.get_mut(self.token, idx)
    }

    /// Returns the front element.
    pub fn front(&self) -> Option<&T> {
        self.deque.get(self.token, 0)
    }

    /// Returns the back element.
    pub fn back(&self) -> Option<&T> {
        if self.len() == 0 {
            None
        } else {
            self.deque.get(self.token, self.len() - 1)
        }
    }

    /// Iterates over elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.deque.iter(self.token)
    }

    /// Exclusive iteration via callback.
    pub fn for_each_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut T),
    {
        self.deque.for_each_mut(self.token, f)
    }
}

/// Extension trait to easily create ActiveVecDeque from BrandedVecDeque.
pub trait ActivateVecDeque<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveVecDeque<'a, 'brand, T>;
}

impl<'brand, T> ActivateVecDeque<'brand, T> for BrandedVecDeque<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveVecDeque<'a, 'brand, T> {
        ActiveVecDeque::new(self, token)
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
    fn test_active_vec_native_slice() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(1);
            vec.push(2);

            let mut active = vec.activate(&mut token);
            assert_eq!(active.as_native_slice(), &[1, 2]);

            active.as_native_mut_slice()[0] = 10;
            assert_eq!(active.as_native_slice(), &[10, 2]);
        });
    }

    #[test]
    fn test_active_vec_iter() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(1);
            vec.push(2);

            let active = vec.activate(&mut token);
            let collected: Vec<&i32> = active.iter().collect();
            assert_eq!(collected, vec![&1, &2]);
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
