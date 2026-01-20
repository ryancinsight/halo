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

use crate::collections::{BrandedCollection, BrandedVec};
use crate::{GhostToken, GhostCell};
use core::ops::{AddAssign, SubAssign};
use std::iter::FromIterator;

/// A branded Fenwick Tree.
pub struct BrandedFenwickTree<'brand, T> {
    /// The tree is 1-indexed internally for easier bit manipulation.
    /// Index 0 is unused (dummy).
    tree: BrandedVec<'brand, T>,
}

impl<'brand, T> BrandedFenwickTree<'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    /// Creates a new empty Fenwick Tree.
    pub fn new() -> Self {
        let mut tree = BrandedVec::new();
        // Push dummy element at index 0
        tree.push(T::default());
        Self { tree }
    }

    /// Creates a new Fenwick Tree with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let mut tree = BrandedVec::with_capacity(capacity + 1);
        tree.push(T::default());
        Self { tree }
    }

    /// Returns the number of elements in the tree (excluding dummy).
    pub fn len(&self) -> usize {
        self.tree.len() - 1
    }

    /// Returns true if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adds `delta` to the element at `index`.
    /// `index` is 0-based.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds.
    pub fn add(&mut self, token: &mut GhostToken<'brand>, index: usize, delta: T) {
        let n = self.len();
        assert!(index < n, "Index out of bounds");

        // Convert to 1-based index
        let mut idx = index + 1;
        while idx <= n {
            unsafe {
                let cell = self.tree.get_unchecked_mut(token, idx);
                *cell += delta;
            }
            // idx += idx & -idx
            idx += idx & (!idx + 1); // logic to isolate last set bit
        }
    }

    /// Computes the prefix sum up to `index` (inclusive).
    /// `index` is 0-based.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds.
    pub fn prefix_sum(&self, token: &GhostToken<'brand>, index: usize) -> T {
        let n = self.len();
        if index >= n {
             panic!("Index out of bounds");
        }

        let mut sum = T::default();
        let mut idx = index + 1;

        while idx > 0 {
            unsafe {
                let cell = self.tree.get_unchecked(token, idx);
                sum += *cell;
            }
            // idx -= idx & -idx
            idx -= idx & (!idx + 1);
        }
        sum
    }

    /// Computes the sum of the range `[start, end)`.
    /// `start` is inclusive, `end` is exclusive.
    ///
    /// # Panics
    /// Panics if indices are out of bounds or `start > end`.
    pub fn range_sum(&self, token: &GhostToken<'brand>, start: usize, end: usize) -> T {
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
    pub fn push(&mut self, token: &mut GhostToken<'brand>, val: T) {
        // 1. Extend the tree with a 0 value.
        self.tree.push(T::default());
        // 2. Add the value to the new position.
        // This maintains the Fenwick invariant.
        let idx = self.len() - 1;
        self.add(token, idx, val);
    }

    /// Clears the tree.
    pub fn clear(&mut self) {
        self.tree.clear();
        self.tree.push(T::default());
    }

    /// Returns the capacity of the backing storage.
    pub fn capacity(&self) -> usize {
        self.tree.capacity() - 1
    }
}

impl<'brand, T> BrandedCollection<'brand> for BrandedFenwickTree<'brand, T> {
    fn is_empty(&self) -> bool {
        self.tree.len() <= 1
    }

    fn len(&self) -> usize {
        self.tree.len() - 1
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
        let mut tree = BrandedVec::with_capacity(lower + 1);

        // Push dummy
        tree.push(T::default());

        // Push elements (raw values first)
        for item in iter {
            tree.push(item);
        }

        // O(n) construction:
        // For each index i from 1 to n, add tree[i] to parent tree[i + (i & -i)]
        let len = tree.len();
        // We can't easily do this with BrandedVec safely without a token because we need to read/write multiple indices.
        // But FromIterator creates the structure, so we own it.
        // BrandedVec owns the cells. We can access them if we had a token?
        // No, FromIterator doesn't take a token.
        // But we are constructing it. The cells are fresh.
        // We can use `get_mut_exclusive` which `BrandedVec` provides!

        for i in 1..len {
            let parent = i + (i & (!i + 1)); // i + LSOne(i)
            if parent < len {
                // Read tree[i]
                let val = *tree.get_mut_exclusive(i).expect("index valid");
                // Add to tree[parent]
                *tree.get_mut_exclusive(parent).expect("parent valid") += val;
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
