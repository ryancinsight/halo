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
use crate::collections::ZeroCopyOps;

/// Zero-cost iterator for BrandedVecDeque.
pub struct BrandedVecDequeIter<'a, 'brand, T> {
    iter: std::collections::vec_deque::Iter<'a, GhostCell<'brand, T>>,
    token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, T> Iterator for BrandedVecDequeIter<'a, 'brand, T> {
    type Item = &'a T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|cell| cell.borrow(self.token))
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline(always)]
    fn count(self) -> usize {
        self.iter.count()
    }
}

impl<'a, 'brand, T> DoubleEndedIterator for BrandedVecDequeIter<'a, 'brand, T> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|cell| cell.borrow(self.token))
    }
}

impl<'a, 'brand, T> ExactSizeIterator for BrandedVecDequeIter<'a, 'brand, T> {}

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
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> BrandedVecDequeIter<'a, 'brand, T> {
        BrandedVecDequeIter {
            iter: self.inner.iter(),
            token,
        }
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

impl<'brand, T> FromIterator<T> for BrandedVecDeque<'brand, T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().map(GhostCell::new).collect(),
        }
    }
}

impl<'brand, T> IntoIterator for BrandedVecDeque<'brand, T> {
    type Item = T;
    type IntoIter = std::iter::Map<std::collections::vec_deque::IntoIter<GhostCell<'brand, T>>, fn(GhostCell<'brand, T>) -> T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter().map(GhostCell::into_inner)
    }
}

impl<'brand, T> Extend<T> for BrandedVecDeque<'brand, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.inner.extend(iter.into_iter().map(GhostCell::new));
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
        self.inner.into_iter().map(|cell| cell.into_inner()).collect()
    }

    /// Creates a draining iterator that removes the specified range in the deque
    /// and yields the removed items.
    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = T> + '_
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.inner.drain(range).map(GhostCell::into_inner)
    }
}

impl<'brand, T> ZeroCopyOps<'brand, T> for BrandedVecDeque<'brand, T> {
    #[inline(always)]
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).find(|&item| f(item))
    }

    #[inline(always)]
    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).any(|item| f(item))
    }

    #[inline(always)]
    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).all(|item| f(item))
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

    #[test]
    fn test_iter_and_zero_copy() {
        GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::new();
            dq.push_back(1);
            dq.push_back(2);
            dq.push_back(3);

            // Test iter
            let collected: Vec<i32> = dq.iter(&token).copied().collect();
            assert_eq!(collected, vec![1, 2, 3]);

            // Test iter rev
            let collected_rev: Vec<i32> = dq.iter(&token).rev().copied().collect();
            assert_eq!(collected_rev, vec![3, 2, 1]);

            // Test zero copy ops
            assert_eq!(dq.find_ref(&token, |&x| x == 2), Some(&2));
            assert!(dq.any_ref(&token, |&x| x == 3));
            assert!(dq.all_ref(&token, |&x| x > 0));
        });
    }
}

