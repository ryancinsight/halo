//! Token-carrying slice abstractions for BrandedVec.
//!
//! These types represent "sub-tokens" or "scoped capabilities" over a region of a vector.
//!
//! - `BrandedSlice<'a, 'brand, T>` bundles a shared slice of cells with a shared token,
//!   enabling safe read access without passing the token explicitly.
//! - `BrandedSliceMut<'a, 'brand, T>` wraps a mutable slice of cells. It acts as a
//!   capability itself because holding `&mut GhostCell` allows exclusive access to the
//!   inner value *without* needing the original `GhostToken`. This allows splitting
//!   mutable access to a vector into disjoint regions that can be mutated in parallel.

use crate::{GhostCell, GhostToken};
use std::slice;

/// A slice of token-gated elements, bundled with the token required to read them.
pub struct BrandedSlice<'a, 'brand, T> {
    pub(crate) slice: &'a [GhostCell<'brand, T>],
    pub(crate) token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, T> BrandedSlice<'a, 'brand, T> {
    /// Creates a new branded slice.
    pub fn new(slice: &'a [GhostCell<'brand, T>], token: &'a GhostToken<'brand>) -> Self {
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
        self.slice.get(index).map(|cell| cell.borrow(self.token))
    }

    /// Returns a shared reference to the element at the given index, without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure index is within bounds.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &'a T {
        self.slice.get_unchecked(index).borrow(self.token)
    }

    /// Returns an iterator over the slice.
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &'a T> + use<'a, 'brand, T> {
        let token = self.token;
        self.slice.iter().map(move |cell| cell.borrow(token))
    }

    /// Divides one slice into two at an index.
    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        let (left, right) = self.slice.split_at(mid);
        (
            Self { slice: left, token: self.token },
            Self { slice: right, token: self.token },
        )
    }

    /// Returns a sub-slice.
    pub fn sub_slice<R>(&self, range: R) -> Self
    where
        R: std::ops::RangeBounds<usize> + std::slice::SliceIndex<[GhostCell<'brand, T>], Output = [GhostCell<'brand, T>]>,
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
/// exclusive access to a `GhostCell` (`&mut GhostCell`) is sufficient to access its content
/// mutably (`GhostCell::get_mut`), bypassing the token requirement.
///
/// This enables:
/// 1. **Parallel Mutation**: You can split a `BrandedSliceMut` into disjoint parts and mutate them in parallel.
/// 2. **Sub-token hierarchy**: This acts as a "sub-token" granting authority over a specific range.
pub struct BrandedSliceMut<'a, 'brand, T> {
    pub(crate) slice: &'a mut [GhostCell<'brand, T>],
}

impl<'a, 'brand, T> BrandedSliceMut<'a, 'brand, T> {
    /// Creates a new mutable branded slice.
    pub fn new(slice: &'a mut [GhostCell<'brand, T>]) -> Self {
        Self { slice }
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
        self.slice.get_mut(index).map(|cell| cell.get_mut())
    }

    /// Returns a mutable reference to the element at the given index, without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure index is within bounds.
    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        self.slice.get_unchecked_mut(index).get_mut()
    }

    /// Returns a mutable iterator over the slice.
    #[inline(always)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + use<'_, 'brand, T> {
        self.slice.iter_mut().map(|cell| cell.get_mut())
    }

    /// Divides one mutable slice into two at an index.
    ///
    /// The returned slices are disjoint and can be mutated independently.
    pub fn split_at_mut(self, mid: usize) -> (Self, Self) {
        let (left, right) = self.slice.split_at_mut(mid);
        (
            Self { slice: left },
            Self { slice: right },
        )
    }

    /// Returns a mutable sub-slice.
    pub fn sub_slice_mut<R>(self, range: R) -> Self
    where
        R: std::ops::RangeBounds<usize> + std::slice::SliceIndex<[GhostCell<'brand, T>], Output = [GhostCell<'brand, T>]>,
    {
        Self {
            slice: &mut self.slice[range],
        }
    }

    /// Sorts the slice.
    ///
    /// This uses the standard library sort, which is efficient and stable.
    pub fn sort(&mut self)
    where
        T: Ord,
    {
        // We need to expose &mut [T] to std::slice::sort.
        // We can do this safely because GhostCell<T> is transparent over UnsafeCell<T>,
        // and UnsafeCell<T> is layout compatible with T.
        // Also, we have exclusive access to the cells.
        unsafe {
            let ptr = self.slice.as_mut_ptr() as *mut T;
            let len = self.slice.len();
            let slice = slice::from_raw_parts_mut(ptr, len);
            slice.sort();
        }
    }

    /// Sorts the slice with a comparator function.
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        unsafe {
            let ptr = self.slice.as_mut_ptr() as *mut T;
            let len = self.slice.len();
            let slice = slice::from_raw_parts_mut(ptr, len);
            slice.sort_by(compare);
        }
    }

    /// Sorts the slice with a key extraction function.
    pub fn sort_by_key<K, F>(&mut self, f: F)
    where
        F: FnMut(&T) -> K,
        K: Ord,
    {
        unsafe {
            let ptr = self.slice.as_mut_ptr() as *mut T;
            let len = self.slice.len();
            let slice = slice::from_raw_parts_mut(ptr, len);
            slice.sort_by_key(f);
        }
    }
}

impl<'a, 'brand, T> IntoIterator for BrandedSliceMut<'a, 'brand, T> {
    type Item = &'a mut T;
    type IntoIter = std::iter::Map<std::slice::IterMut<'a, GhostCell<'brand, T>>, fn(&'a mut GhostCell<'brand, T>) -> &'a mut T>;

    fn into_iter(self) -> Self::IntoIter {
        self.slice.iter_mut().map(GhostCell::get_mut)
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

            let slice = BrandedSlice::new(&vec.inner, &token);
            assert_eq!(slice.len(), 3);
            assert_eq!(*slice.get(0).unwrap(), 1);
            assert_eq!(*slice.get(2).unwrap(), 3);

            let collected: Vec<i32> = slice.iter().copied().collect();
            assert_eq!(collected, vec![1, 2, 3]);

            let (left, right) = slice.split_at(1);
            assert_eq!(*left.get(0).unwrap(), 1);
            assert_eq!(*right.get(0).unwrap(), 2);
        });
    }

    #[test]
    fn test_branded_slice_mut_write_parallel_concept() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(1);
            vec.push(2);
            vec.push(3);
            vec.push(4);

            // Create a BrandedSliceMut from the vector.
            // Note: BrandedVec stores Vec<GhostCell<T>>.
            // We need exclusive access to it.
            let slice_mut = BrandedSliceMut::new(&mut vec.inner);

            // Split into two mutable slices
            let (mut left, mut right) = slice_mut.split_at_mut(2);

            // Mutate independently (simulating parallel access possibility)
            if let Some(x) = left.get_mut(0) { *x *= 10; } // 1 -> 10
            if let Some(x) = right.get_mut(0) { *x *= 10; } // 3 -> 30

            // Verify with token
            assert_eq!(*vec.get(&token, 0).unwrap(), 10);
            assert_eq!(*vec.get(&token, 2).unwrap(), 30);
        });
    }

    #[test]
    fn test_branded_slice_mut_sort() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(3);
            vec.push(1);
            vec.push(2);

            let mut slice = BrandedSliceMut::new(&mut vec.inner);
            slice.sort();

            assert_eq!(*vec.get(&token, 0).unwrap(), 1);
            assert_eq!(*vec.get(&token, 1).unwrap(), 2);
            assert_eq!(*vec.get(&token, 2).unwrap(), 3);
        });
    }
}
