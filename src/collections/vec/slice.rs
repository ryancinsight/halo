//! Token-carrying slice abstractions for BrandedVec.
//!
//! These types represent "sub-tokens" or "scoped capabilities" over a region of a vector.
//!
//! - `BrandedSlice<'a, 'brand, T>` bundles a shared slice of values with a shared token,
//!   enabling safe read access.
//! - `BrandedSliceMut<'a, 'brand, T>` wraps a mutable slice of values. It acts as a
//!   capability itself.

use crate::{token::InvariantLifetime, GhostToken};
use std::marker::PhantomData;
use std::slice;

/// A slice of token-gated elements, bundled with the token required to read them.
pub struct BrandedSlice<'a, 'brand, T> {
    pub(crate) slice: &'a [T],
    pub(crate) token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, T> BrandedSlice<'a, 'brand, T> {
    /// Creates a new branded slice.
    pub fn new(slice: &'a [T], token: &'a GhostToken<'brand>) -> Self {
        Self { slice, token }
    }

    /// Returns the length of the slice.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.slice.len()
    }

    /// Returns true if the slice is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }

    /// Returns a shared reference to the element at the given index.
    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&'a T> {
        self.slice.get(index)
    }

    /// Returns a shared reference to the element at the given index, without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure index is within bounds.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &'a T {
        self.slice.get_unchecked(index)
    }

    /// Returns the underlying slice as a standard `&[T]`.
    #[inline(always)]
    pub fn as_slice(&self) -> &'a [T] {
        self.slice
    }

    /// Consumes the BrandedSlice and returns the underlying slice as a standard `&[T]`.
    #[inline(always)]
    pub fn into_slice(self) -> &'a [T] {
        self.slice
    }

    /// Returns an iterator over the slice.
    #[inline(always)]
    pub fn iter(&self) -> slice::Iter<'a, T> {
        self.slice.iter()
    }

    /// Divides one slice into two at an index.
    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        let (left, right) = self.slice.split_at(mid);
        (
            Self {
                slice: left,
                token: self.token,
            },
            Self {
                slice: right,
                token: self.token,
            },
        )
    }

    /// Returns a sub-slice.
    pub fn sub_slice<R>(&self, range: R) -> Self
    where
        R: std::ops::RangeBounds<usize> + std::slice::SliceIndex<[T], Output = [T]>,
    {
        Self {
            slice: &self.slice[range],
            token: self.token,
        }
    }
}

/// A mutable slice of token-gated elements.
///
/// This type does **not** carry a `GhostToken`. Instead, it relies on the fact that
/// exclusive access to the memory is already proven by the existence of `&mut [T]`.
pub struct BrandedSliceMut<'a, 'brand, T> {
    pub(crate) slice: &'a mut [T],
    pub(crate) _marker: PhantomData<InvariantLifetime<'brand>>,
}

impl<'a, 'brand, T> BrandedSliceMut<'a, 'brand, T> {
    /// Creates a new mutable branded slice.
    pub fn new(slice: &'a mut [T]) -> Self {
        Self {
            slice,
            _marker: PhantomData,
        }
    }

    /// Returns the length of the slice.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.slice.len()
    }

    /// Returns true if the slice is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }

    /// Returns a mutable reference to the element at the given index.
    #[inline(always)]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.slice.get_mut(index)
    }

    /// Returns a mutable reference to the element at the given index, without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure index is within bounds.
    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        self.slice.get_unchecked_mut(index)
    }

    /// Returns the underlying slice as a standard `&[T]`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        self.slice
    }

    /// Returns the underlying mutable slice as a standard `&mut [T]`.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.slice
    }

    /// Consumes the BrandedSliceMut and returns the underlying mutable slice as a standard `&mut [T]`.
    #[inline(always)]
    pub fn into_mut_slice(self) -> &'a mut [T] {
        self.slice
    }

    /// Returns a mutable iterator over the slice.
    #[inline(always)]
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, T> {
        self.slice.iter_mut()
    }

    /// Divides one mutable slice into two at an index.
    pub fn split_at_mut(self, mid: usize) -> (Self, Self) {
        let (left, right) = self.slice.split_at_mut(mid);
        (
            Self {
                slice: left,
                _marker: PhantomData,
            },
            Self {
                slice: right,
                _marker: PhantomData,
            },
        )
    }

    /// Returns a mutable sub-slice.
    pub fn sub_slice_mut<R>(self, range: R) -> Self
    where
        R: std::ops::RangeBounds<usize> + std::slice::SliceIndex<[T], Output = [T]>,
    {
        Self {
            slice: &mut self.slice[range],
            _marker: PhantomData,
        }
    }

    /// Sorts the slice.
    pub fn sort(&mut self)
    where
        T: Ord,
    {
        self.slice.sort();
    }

    /// Sorts the slice with a comparator function.
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        self.slice.sort_by(compare);
    }

    /// Sorts the slice with a key extraction function.
    pub fn sort_by_key<K, F>(&mut self, f: F)
    where
        F: FnMut(&T) -> K,
        K: Ord,
    {
        self.slice.sort_by_key(f);
    }
}

impl<'a, 'brand, T> IntoIterator for BrandedSliceMut<'a, 'brand, T> {
    type Item = &'a mut T;
    type IntoIter = slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.slice.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::vec::BrandedVec;
    use crate::GhostToken;

    #[test]
    fn test_branded_slice_read() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(1);
            vec.push(2);
            vec.push(3);

            let slice = BrandedSlice::new(vec.as_slice(&token), &token);
            assert_eq!(slice.len(), 3);
            assert_eq!(slice.as_slice(), &[1, 2, 3]);

            assert_eq!(*slice.get(0).unwrap(), 1);
            assert_eq!(*slice.get(2).unwrap(), 3);
        });
    }

    #[test]
    fn test_branded_slice_mut_as_mut_slice() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(3);
            vec.push(1);
            vec.push(2);

            let mut slice_mut = BrandedSliceMut::new(vec.as_mut_slice(&mut token));
            slice_mut.as_mut_slice().sort();

            assert_eq!(*vec.get(&token, 0).unwrap(), 1);
        });
    }
}
