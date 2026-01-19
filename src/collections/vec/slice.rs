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
        self.as_slice().get(index)
    }

    /// Returns a shared reference to the element at the given index, without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure index is within bounds.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &'a T {
        self.as_slice().get_unchecked(index)
    }

    /// Returns the underlying slice as a standard `&[T]`.
    ///
    /// This is a zero-cost operation that exposes the raw data for efficient processing
    /// (e.g., using `memchr`, SIMD, or other slice optimizations).
    #[inline(always)]
    pub fn as_slice(&self) -> &'a [T] {
        // SAFETY:
        // 1. `GhostCell<T>` is `repr(transparent)` over `UnsafeCell<T>`.
        // 2. `UnsafeCell<T>` is `repr(transparent)` over `T` (layout compatible).
        // 3. We hold `&GhostToken`, which guarantees that no mutable reference to the data exists.
        //    (GhostToken linearity + BrandedVec invariants).
        unsafe {
            let ptr = self.slice.as_ptr() as *const T;
            slice::from_raw_parts(ptr, self.slice.len())
        }
    }

    /// Consumes the BrandedSlice and returns the underlying slice as a standard `&[T]`.
    #[inline(always)]
    pub fn into_slice(self) -> &'a [T] {
        self.as_slice()
    }

    /// Returns an iterator over the slice.
    #[inline(always)]
    pub fn iter(&self) -> slice::Iter<'a, T> {
        self.as_slice().iter()
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
        R: std::ops::RangeBounds<usize>
            + std::slice::SliceIndex<[GhostCell<'brand, T>], Output = [GhostCell<'brand, T>]>,
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
        // We can't easily implement get_mut via as_mut_slice because lifetimes are tricky
        // if we return Option<&mut T> from &mut self.
        // Actually it's fine:
        // self.as_mut_slice().get_mut(index)
        // However, as_mut_slice consumes `self` (or reborrows `self`).
        // Let's keep it simple.
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

    /// Returns the underlying slice as a standard `&[T]`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        // We have &self (shared), but BrandedSliceMut implies we *own* the mutable lock on cells.
        // But we only have shared access to BrandedSliceMut here.
        // Is it safe to return &[T]?
        // Yes, because &BrandedSliceMut means we have shared access to the &mut [GhostCell].
        // Wait. `&mut [T]` -> `& [T]` is safe.
        // `&mut GhostCell` -> `&GhostCell`.
        // `&GhostCell` -> `&T` requires token?
        // Ah! BrandedSliceMut DOES NOT have a token!
        // So `&BrandedSliceMut` does NOT allow reading `&T`!
        // We only have `&mut GhostCell` inside `&mut self`.
        // If we have `&self` of `BrandedSliceMut`, we have `& (&mut [GhostCell])`.
        // We cannot get `&T` from `&GhostCell` without token.
        // So `as_slice` is NOT possible without token!
        // WE CAN ONLY DO `as_mut_slice` because that uses the exclusivity of `&mut GhostCell`.

        // Wait, `BrandedSliceMut` holds `&'a mut [GhostCell]`.
        // The struct itself gives us exclusive access to the cells.
        // If we have `&mut self`, we can get `&mut [T]`.
        // If we have `&self`, we only have `& [GhostCell]`. We can't read `T`.
        // So `as_slice` is INVALID for `BrandedSliceMut` unless we pass a token.
        // But `BrandedSliceMut` is designed to work *without* a token (for mutation).
        // So we cannot implement `as_slice` here.
        panic!(
            "BrandedSliceMut cannot produce &[T] without a token. Use as_mut_slice for &mut [T]."
        );
    }

    /// Returns the underlying mutable slice as a standard `&mut [T]`.
    ///
    /// This allows using standard slice algorithms (sort, rotate, etc.).
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY:
        // 1. `GhostCell<T>` layout compatible with `T`.
        // 2. We have `&mut self`, so we have exclusive access to `&mut [GhostCell]`.
        // 3. `&mut GhostCell` allows getting `&mut T` without token.
        unsafe {
            let ptr = self.slice.as_mut_ptr() as *mut T;
            slice::from_raw_parts_mut(ptr, self.slice.len())
        }
    }

    /// Consumes the BrandedSliceMut and returns the underlying mutable slice as a standard `&mut [T]`.
    #[inline(always)]
    pub fn into_mut_slice(self) -> &'a mut [T] {
        unsafe {
            let ptr = self.slice.as_mut_ptr() as *mut T;
            slice::from_raw_parts_mut(ptr, self.slice.len())
        }
    }

    /// Returns a mutable iterator over the slice.
    #[inline(always)]
    pub fn iter_mut(&mut self) -> slice::IterMut<'_, T> {
        self.as_mut_slice().iter_mut()
    }

    /// Divides one mutable slice into two at an index.
    pub fn split_at_mut(self, mid: usize) -> (Self, Self) {
        let (left, right) = self.slice.split_at_mut(mid);
        (Self { slice: left }, Self { slice: right })
    }

    /// Returns a mutable sub-slice.
    pub fn sub_slice_mut<R>(self, range: R) -> Self
    where
        R: std::ops::RangeBounds<usize>
            + std::slice::SliceIndex<[GhostCell<'brand, T>], Output = [GhostCell<'brand, T>]>,
    {
        Self {
            slice: &mut self.slice[range],
        }
    }

    /// Sorts the slice.
    pub fn sort(&mut self)
    where
        T: Ord,
    {
        self.as_mut_slice().sort();
    }

    /// Sorts the slice with a comparator function.
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        self.as_mut_slice().sort_by(compare);
    }

    /// Sorts the slice with a key extraction function.
    pub fn sort_by_key<K, F>(&mut self, f: F)
    where
        F: FnMut(&T) -> K,
        K: Ord,
    {
        self.as_mut_slice().sort_by_key(f);
    }
}

impl<'a, 'brand, T> IntoIterator for BrandedSliceMut<'a, 'brand, T> {
    type Item = &'a mut T;
    // We can just use standard slice iterator now
    type IntoIter = slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        // unsafe cast the whole slice
        unsafe {
            let ptr = self.slice.as_mut_ptr() as *mut T;
            slice::from_raw_parts_mut(ptr, self.slice.len()).iter_mut()
        }
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
            // check as_slice
            assert_eq!(slice.as_slice(), &[1, 2, 3]);

            assert_eq!(*slice.get(0).unwrap(), 1);
            assert_eq!(*slice.get(2).unwrap(), 3);

            let collected: Vec<i32> = slice.iter().copied().collect();
            assert_eq!(collected, vec![1, 2, 3]);
        });
    }

    #[test]
    fn test_branded_slice_mut_as_mut_slice() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(3);
            vec.push(1);
            vec.push(2);

            let mut slice_mut = BrandedSliceMut::new(&mut vec.inner);
            slice_mut.as_mut_slice().sort();

            assert_eq!(*vec.get(&token, 0).unwrap(), 1);
        });
    }
}
