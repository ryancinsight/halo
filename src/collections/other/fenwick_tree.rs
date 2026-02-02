//! `BrandedFenwickTree` — A token-gated Binary Indexed Tree (Fenwick Tree).
//!
//! A Fenwick Tree provides efficient methods for calculation and manipulation
//! of the prefix sums of a table of values.
//!
//! Time Complexity:
//! - Update: O(log n)
//! - Prefix Sum: O(log n)
//! - Range Sum: O(log n)
//!
//! Space Complexity: O(n)
//!
//! This implementation is "branded", meaning it is secured by a `GhostToken`
//! and backed by a `BrandedVec` arena.
//!
//! This implementation uses 0-based indexing logic internally to avoid dummy elements.

use crate::collections::{BrandedCollection, BrandedVec};
use crate::token::traits::{GhostBorrow, GhostBorrowMut};
use core::ops::{AddAssign, SubAssign};
use std::iter::FromIterator;

/// A branded Fenwick Tree.
pub struct BrandedFenwickTree<'brand, T> {
    tree: BrandedVec<'brand, T>,
}

impl<'brand, T> BrandedFenwickTree<'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    /// Creates a new empty Fenwick Tree.
    pub fn new() -> Self {
        Self {
            tree: BrandedVec::new(),
        }
    }

    /// Creates a new Fenwick Tree with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tree: BrandedVec::with_capacity(capacity),
        }
    }

    /// Returns the number of elements in the tree.
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// Returns true if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// Adds `delta` to the element at `index`.
    /// `index` is 0-based.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds.
    pub fn add<Token>(&mut self, token: &mut Token, index: usize, delta: T)
    where
        Token: GhostBorrowMut<'brand>,
    {
        let n = self.len();
        assert!(index < n, "Index out of bounds");

        let mut idx = index;
        // Unrolling loop for performance
        while idx < n {
            unsafe {
                *self.tree.get_unchecked_mut(token, idx) += delta;
            }
            idx = idx | (idx + 1);
            if idx < n {
                 unsafe {
                    *self.tree.get_unchecked_mut(token, idx) += delta;
                }
                idx = idx | (idx + 1);
                if idx < n {
                     unsafe {
                        *self.tree.get_unchecked_mut(token, idx) += delta;
                    }
                    idx = idx | (idx + 1);
                    if idx < n {
                         unsafe {
                            *self.tree.get_unchecked_mut(token, idx) += delta;
                        }
                        idx = idx | (idx + 1);
                    }
                }
            }
        }
    }

    /// Computes the prefix sum up to `index` (inclusive).
    /// `index` is 0-based.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds.
    pub fn prefix_sum<Token>(&self, token: &Token, index: usize) -> T
    where
        Token: GhostBorrow<'brand>,
    {
        let n = self.len();
        if index >= n {
             panic!("Index out of bounds");
        }

        let mut sum = T::default();
        let mut idx = index;

        loop {
            unsafe {
                sum += *self.tree.get_unchecked(token, idx);
            }
            // idx = (idx & (idx + 1)) - 1
            let next_idx = (idx & (idx + 1)).wrapping_sub(1);
            if next_idx >= idx { break; } // Wrapped around
            idx = next_idx;

             unsafe {
                sum += *self.tree.get_unchecked(token, idx);
            }
            let next_idx = (idx & (idx + 1)).wrapping_sub(1);
            if next_idx >= idx { break; }
            idx = next_idx;

            unsafe {
                sum += *self.tree.get_unchecked(token, idx);
            }
            let next_idx = (idx & (idx + 1)).wrapping_sub(1);
            if next_idx >= idx { break; }
            idx = next_idx;

            unsafe {
                sum += *self.tree.get_unchecked(token, idx);
            }
            let next_idx = (idx & (idx + 1)).wrapping_sub(1);
            if next_idx >= idx { break; }
            idx = next_idx;
        }
        sum
    }

    /// Computes the sum of the range `[start, end)`.
    /// `start` is inclusive, `end` is exclusive.
    ///
    /// # Panics
    /// Panics if indices are out of bounds or `start > end`.
    pub fn range_sum<Token>(&self, token: &Token, start: usize, end: usize) -> T
    where
        Token: GhostBorrow<'brand>,
    {
        if start > end {
            panic!("start > end");
        }
        if start == end {
            return T::default();
        }
        let sum_end = self.prefix_sum(token, end - 1);
        if start == 0 {
            sum_end
        } else {
            let sum_start = self.prefix_sum(token, start - 1);
            let mut result = sum_end;
            result -= sum_start;
            result
        }
    }

    /// Pushes a new value to the end of the tree.
    pub fn push<Token>(&mut self, token: &mut Token, val: T)
    where
        Token: GhostBorrowMut<'brand>,
    {
        // Just appending `val` is not enough for Fenwick Tree logic unless we update parents.
        // But `push` implies appending to the array that the Fenwick Tree represents.
        // If we append to array A, A[n] = val.
        // In FT, T[n] = sum(A[k]) for appropriate k.
        // T[n] = val + sum of children in the tree structure?
        // Actually, T[i] stores sum of A[j] where j covers range.
        // When we add a new element at `len`, it covers `[len - (len&-len-logic) + 1, len]`.
        // We can compute T[len] using prefix sums!
        // T[len] = prefix_sum(len) - prefix_sum(len - (len&-len-logic)).
        // But we don't have val at len yet.
        // So `T[len]` is just `val` + (sum of specific previous ranges).
        // A simpler way:
        // 1. Push 0.
        // 2. `add(len, val)`.

        self.tree.push(T::default());
        let idx = self.len() - 1;
        self.add(token, idx, val);
    }

    /// Clears the tree.
    pub fn clear(&mut self) {
        self.tree.clear();
    }

    /// Returns the capacity of the backing storage.
    pub fn capacity(&self) -> usize {
        self.tree.capacity()
    }
}

impl<'brand, T> BrandedCollection<'brand> for BrandedFenwickTree<'brand, T> {
    fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    fn len(&self) -> usize {
        self.tree.len()
    }
}

impl<'brand, T> Default for BrandedFenwickTree<'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Construction from iterator.
/// This performs an O(n) build of the Fenwick Tree.
impl<'brand, T> FromIterator<T> for BrandedFenwickTree<'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();
        let mut tree = BrandedVec::with_capacity(lower);

        // Push elements (raw values first)
        for item in iter {
            tree.push(item);
        }

        // O(n) construction:
        // For each index i from 0 to n-1:
        // parent = i | (i + 1)
        // if parent < n: tree[parent] += tree[i]

        let len = tree.len();

        for i in 0..len {
            let parent = i | (i + 1);
            if parent < len {
                unsafe {
                    let val = *tree.get_unchecked_mut_exclusive(i);
                    *tree.get_unchecked_mut_exclusive(parent) += val;
                }
            }
        }

        Self { tree }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_fenwick_tree_basic() {
        GhostToken::new(|mut token| {
            let mut ft = BrandedFenwickTree::<i32>::new();
            // Size 5
            for _ in 0..5 {
                ft.push(&mut token, 0);
            }
            assert_eq!(ft.len(), 5);

            // Add 1 at index 0
            ft.add(&mut token, 0, 1);
            assert_eq!(ft.prefix_sum(&token, 0), 1);
            assert_eq!(ft.prefix_sum(&token, 4), 1);

            // Add 2 at index 2
            ft.add(&mut token, 2, 2);
            assert_eq!(ft.prefix_sum(&token, 1), 1); // sum[0..1]
            assert_eq!(ft.prefix_sum(&token, 2), 3); // 1 + 2
            assert_eq!(ft.prefix_sum(&token, 4), 3);

            // Add 3 at index 4
            ft.add(&mut token, 4, 3);
            assert_eq!(ft.prefix_sum(&token, 4), 6);

            // Range sum
            assert_eq!(ft.range_sum(&token, 0, 5), 6);
            assert_eq!(ft.range_sum(&token, 1, 3), 2); // indices 1, 2. val[1]=0, val[2]=2. sum=2.
            assert_eq!(ft.range_sum(&token, 2, 3), 2); // index 2. val=2.
        });
    }

    #[test]
    fn test_fenwick_tree_from_iter() {
        GhostToken::new(|mut token| {
            let ft: BrandedFenwickTree<i32> = vec![1, 2, 3, 4, 5].into_iter().collect();

            assert_eq!(ft.len(), 5);
            assert_eq!(ft.prefix_sum(&token, 0), 1);
            assert_eq!(ft.prefix_sum(&token, 1), 3); // 1+2
            assert_eq!(ft.prefix_sum(&token, 4), 15); // 1+2+3+4+5

            assert_eq!(ft.range_sum(&token, 1, 4), 9); // 2+3+4
        });
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn test_fenwick_tree_oob_add() {
         GhostToken::new(|mut token| {
            let mut ft = BrandedFenwickTree::<i32>::new();
            ft.add(&mut token, 0, 1);
        });
    }
}
