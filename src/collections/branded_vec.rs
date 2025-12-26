//! `BrandedVec` — a vector of token-gated cells.
//!
//! This is the canonical “branded vector” pattern from the GhostCell/RustBelt paper:
//! store many independently-mutable elements in one owned container, while using a
//! **single** linear token to gate all borrows.
//!
//! Design:
//! - The container owns a `Vec<GhostCell<'brand, T>>`.
//! - Structural mutations (`push`, `pop`, `reserve`, …) follow normal Rust rules via
//!   `&mut self`.
//! - Element access is token-gated:
//!   - shared access: `&GhostToken<'brand>` → `&T`
//!   - exclusive access: `&mut GhostToken<'brand>` → `&mut T`
//!
//! This is exactly the separation of *permissions* (token) from *data* (cells).

use crate::{GhostCell, GhostToken};

/// A vector of token-gated elements.
pub struct BrandedVec<'brand, T> {
    inner: Vec<GhostCell<'brand, T>>,
}

impl<'brand, T> BrandedVec<'brand, T> {
    /// Creates an empty vector.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Creates an empty vector with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Current capacity.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    /// Pushes a new element.
    pub fn push(&mut self, value: T) {
        self.inner.push(GhostCell::new(value));
    }

    /// Pops the last element.
    pub fn pop(&mut self) -> Option<GhostCell<'brand, T>> {
        self.inner.pop()
    }

    /// Inserts an element at position `index`.
    pub fn insert(&mut self, index: usize, value: T) {
        self.inner.insert(index, GhostCell::new(value));
    }

    /// Removes and returns the element at position `index`.
    pub fn remove(&mut self, index: usize) -> GhostCell<'brand, T> {
        self.inner.remove(index)
    }

    /// Removes an element from the vector and returns it, replaces it with the last element.
    pub fn swap_remove(&mut self, index: usize) -> GhostCell<'brand, T> {
        self.inner.swap_remove(index)
    }

    /// Retains only the elements specified by the predicate.
    pub fn retain<F>(&mut self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        self.inner.retain(|c| f(c.borrow_mut(token)));
    }

    /// Returns a token-gated shared reference to element `idx`, if in bounds.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        self.inner.get(idx).map(|c| c.borrow(token))
    }

    /// Returns a token-gated exclusive reference to element `idx`, if in bounds.
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        self.inner.get(idx).map(|c| c.borrow_mut(token))
    }

    /// Returns a token-gated shared reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        self.get(token, idx).expect("index out of bounds")
    }

    /// Returns a token-gated exclusive reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> &'a mut T {
        self.get_mut(token, idx).expect("index out of bounds")
    }

    /// Iterates over all elements by shared reference.
    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = &'a T> + 'a {
        self.inner.iter().map(|c| c.borrow(token))
    }

    /// Applies `f` to each element by exclusive reference.
    ///
    /// This is the canonical safe pattern for *sequential* exclusive iteration:
    /// each `&mut T` is scoped to one callback invocation, which preserves the
    /// token linearity invariant without requiring an `Iterator<Item = &mut T>`.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        for cell in &self.inner {
            // Each borrow is scoped to this loop iteration.
            let x = cell.borrow_mut(token);
            f(x);
        }
    }
}

impl<'brand, T> Default for BrandedVec<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn branded_vec_basic_access() {
        GhostToken::new(|mut token| {
            let mut v: BrandedVec<'_, u64> = BrandedVec::new();
            v.push(10);
            v.push(20);

            assert_eq!(v.len(), 2);
            assert_eq!(*v.borrow(&token, 0), 10);
            assert_eq!(*v.borrow(&token, 1), 20);

            *v.borrow_mut(&mut token, 0) += 7;
            assert_eq!(*v.borrow(&token, 0), 17);
        });
    }

    #[test]
    fn branded_vec_iter_and_iter_mut() {
        GhostToken::new(|mut token| {
            let mut v: BrandedVec<'_, i32> = BrandedVec::new();
            for i in 0..10 {
                v.push(i);
            }

            let sum: i32 = v.iter(&token).copied().sum();
            assert_eq!(sum, (0..10).sum());

            v.for_each_mut(&mut token, |x| *x *= 2);
            let doubled: Vec<i32> = v.iter(&token).copied().collect();
            assert_eq!(doubled, (0..10).map(|x| x * 2).collect::<Vec<_>>());
        });
    }
}


