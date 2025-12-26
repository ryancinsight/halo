//! `BrandedVecDeque` — a double-ended queue of token-gated cells.
//!
//! This provides a double-ended version of `BrandedVec`, allowing efficient
//! push/pop from both ends while maintaining the GhostCell safety model.
//!
//! Implementation:
//! - Backed by a `VecDeque<GhostCell<'brand, T>>`.
//! - Access is gated by `GhostToken<'brand>`.

use std::collections::VecDeque;
use crate::{GhostCell, GhostToken};

/// A double-ended queue of token-gated elements.
#[repr(transparent)]
pub struct BrandedVecDeque<'brand, T> {
    inner: VecDeque<GhostCell<'brand, T>>,
}

impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Creates an empty deque.
    pub fn new() -> Self {
        Self { inner: VecDeque::new() }
    }

    /// Creates an empty deque with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(capacity),
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

    /// Clears the deque.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) {
        self.inner.push_back(GhostCell::new(value));
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) {
        self.inner.push_front(GhostCell::new(value));
    }

    /// Pops from the back.
    pub fn pop_back(&mut self) -> Option<GhostCell<'brand, T>> {
        self.inner.pop_back()
    }

    /// Pops from the front.
    pub fn pop_front(&mut self) -> Option<GhostCell<'brand, T>> {
        self.inner.pop_front()
    }

    /// Returns a shared reference to the element at `idx`, if in bounds.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        self.inner.get(idx).map(|c| c.borrow(token))
    }

    /// Returns an exclusive reference to the element at `idx`, if in bounds.
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> Option<&'a mut T> {
        self.inner.get(idx).map(|c| c.borrow_mut(token))
    }


    /// Iterates over the elements.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a T> + 'a {
        self.inner.iter().map(move |c| c.borrow(token))
    }

    /// Exclusive iteration via callback.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        for cell in &self.inner {
            let x = cell.borrow_mut(token);
            f(x);
        }
    }

    /// Bulk operation: applies `f` to all elements by shared reference.
    ///
    /// This provides direct access to the internal storage for maximum efficiency
    /// when you need to read all elements.
    #[inline]
    pub fn for_each_bulk<'a>(&'a self, token: &'a GhostToken<'brand>, mut f: impl FnMut(&'a T)) {
        for cell in &self.inner {
            let x = cell.borrow(token);
            f(x);
        }
    }
}

impl<'brand, T> Default for BrandedVecDeque<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T> From<T> for BrandedVecDeque<'brand, T> {
    fn from(value: T) -> Self {
        let mut deque = Self::new();
        deque.push_back(value);
        deque
    }
}

impl<'brand, T> From<VecDeque<T>> for BrandedVecDeque<'brand, T> {
    fn from(deque: VecDeque<T>) -> Self {
        Self {
            inner: deque.into_iter().map(GhostCell::new).collect(),
        }
    }
}

// Zero-cost conversion back to VecDeque (requires token for safety)
impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Consumes the branded deque and returns the inner `VecDeque<T>`.
    ///
    /// This is a zero-cost operation as it only moves the deque.
    pub fn into_vec_deque(self) -> VecDeque<T> {
        // SAFETY: GhostToken linearity ensures no outstanding borrows
        GhostToken::new(|_token| {
            self.inner.into_iter().map(|cell| cell.into_inner()).collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn branded_vec_deque_basic() {
        GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::new();
            dq.push_back(10);
            dq.push_front(20);
            assert_eq!(dq.len(), 2);
            assert_eq!(*dq.get(&token, 0).unwrap(), 20);
            assert_eq!(*dq.get(&token, 1).unwrap(), 10);

            *dq.get_mut(&mut token, 0).unwrap() += 5;
            assert_eq!(*dq.get(&token, 0).unwrap(), 25);
        });
    }
}

