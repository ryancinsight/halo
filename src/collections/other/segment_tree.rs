//! `BrandedSegmentTree` — a Segment Tree with token-gated access and sub-view capabilities.
//!
//! This implementation provides a generic Segment Tree backed by `BrandedVec`.
//! It supports efficient range queries and point updates.
//! It features hierarchical "sub-token" views (`BrandedSegmentTreeViewMut`) that allow
//! safe splitting of the tree into disjoint mutable regions for parallel processing.

use crate::collections::{BrandedCollection, BrandedVec};
use crate::{GhostCell, GhostToken};
use std::marker::PhantomData;
use std::mem::MaybeUninit;

/// A branded Segment Tree.
pub struct BrandedSegmentTree<'brand, T, F> {
    tree: BrandedVec<'brand, T>,
    n: usize,
    combinator: F,
    default_value: T,
}

/// A mutable view into a sub-tree of the Segment Tree.
pub struct BrandedSegmentTreeViewMut<'a, 'brand, T, F> {
    /// Pointer to the base of the vector storage (element 0).
    ptr: *mut GhostCell<'brand, T>,
    /// Index of the current node in the heap layout (0-based).
    node_idx: usize,
    /// The range covered by this node `[start, end)`.
    range_start: usize,
    /// The range covered by this node `[start, end)`.
    range_end: usize,
    /// Reference to the combinator function.
    combinator: &'a F,
    /// Reference to the default value (neutral element).
    default_value: &'a T,
    /// Marker for lifetime.
    _marker: PhantomData<&'a mut GhostCell<'brand, T>>,
}

unsafe impl<'a, 'brand, T: Send, F: Sync> Send for BrandedSegmentTreeViewMut<'a, 'brand, T, F> {}
unsafe impl<'a, 'brand, T: Sync, F: Sync> Sync for BrandedSegmentTreeViewMut<'a, 'brand, T, F> {}

impl<'brand, T, F> BrandedSegmentTree<'brand, T, F>
where
    T: Clone + PartialEq,
    F: Fn(&T, &T) -> T,
{
    /// Creates a new Segment Tree with size `n`, a `combinator` function, and a `default_value` (neutral element).
    pub fn new(n: usize, combinator: F, default_value: T) -> Self {
        // Size requirement: next power of 2 * 2. Or just 4*n safe bound.
        let size = 4 * n;
        let mut tree = BrandedVec::with_capacity(size);
        for _ in 0..size {
            tree.push(default_value.clone());
        }

        Self {
            tree,
            n,
            combinator,
            default_value,
        }
    }

    /// Builds the tree from an initial slice.
    pub fn build(&mut self, token: &mut GhostToken<'brand>, data: &[T]) {
        assert!(data.len() <= self.n);
        // Reset
        for i in 0..self.tree.len() {
            *self.tree.borrow_mut(token, i) = self.default_value.clone();
        }

        self.build_recursive(token, data, 0, 0, self.n);
    }

    fn build_recursive(
        &mut self,
        token: &mut GhostToken<'brand>,
        data: &[T],
        node: usize,
        start: usize,
        end: usize,
    ) {
        if start >= end {
            return;
        }
        if start == end - 1 {
            if start < data.len() {
                *self.tree.borrow_mut(token, node) = data[start].clone();
            }
            return;
        }

        let mid = start + (end - start) / 2;
        let left_child = 2 * node + 1;
        let right_child = 2 * node + 2;

        self.build_recursive(token, data, left_child, start, mid);
        self.build_recursive(token, data, right_child, mid, end);

        let left_val = self.tree.borrow(token, left_child).clone();
        let right_val = self.tree.borrow(token, right_child).clone();
        *self.tree.borrow_mut(token, node) = (self.combinator)(&left_val, &right_val);
    }

    /// Updates the value at `index` to `value`.
    pub fn update(&mut self, token: &mut GhostToken<'brand>, index: usize, value: T) {
        assert!(index < self.n);
        self.update_recursive(token, 0, 0, self.n, index, value);
    }

    fn update_recursive(
        &mut self,
        token: &mut GhostToken<'brand>,
        node: usize,
        start: usize,
        end: usize,
        idx: usize,
        val: T,
    ) {
        if start == end - 1 {
            *self.tree.borrow_mut(token, node) = val;
            return;
        }

        let mid = start + (end - start) / 2;
        let left_child = 2 * node + 1;
        let right_child = 2 * node + 2;

        if idx < mid {
            self.update_recursive(token, left_child, start, mid, idx, val);
        } else {
            self.update_recursive(token, right_child, mid, end, idx, val);
        }

        // Pull up
        let left_val = self.tree.borrow(token, left_child).clone();
        let right_val = self.tree.borrow(token, right_child).clone();
        *self.tree.borrow_mut(token, node) = (self.combinator)(&left_val, &right_val);
    }

    /// Queries the range `[q_start, q_end)`.
    pub fn query(&self, token: &GhostToken<'brand>, q_start: usize, q_end: usize) -> T {
        if q_start >= q_end || q_start >= self.n {
            return self.default_value.clone();
        }
        self.query_recursive(token, 0, 0, self.n, q_start, q_end)
    }

    fn query_recursive(
        &self,
        token: &GhostToken<'brand>,
        node: usize,
        start: usize,
        end: usize,
        q_start: usize,
        q_end: usize,
    ) -> T {
        if q_start <= start && end <= q_end {
            return self.tree.borrow(token, node).clone();
        }
        if end <= q_start || start >= q_end {
            return self.default_value.clone();
        }

        let mid = start + (end - start) / 2;
        let left_child = 2 * node + 1;
        let right_child = 2 * node + 2;

        let l_res = self.query_recursive(token, left_child, start, mid, q_start, q_end);
        let r_res = self.query_recursive(token, right_child, mid, end, q_start, q_end);

        (self.combinator)(&l_res, &r_res)
    }

    /// Repair the tree consistency after bulk updates via views.
    /// This recomputes all internal nodes based on the values in the leaves.
    /// It is an O(N) operation.
    pub fn repair(&mut self, token: &mut GhostToken<'brand>) {
        self.repair_recursive(token, 0, 0, self.n);
    }

    fn repair_recursive(
        &mut self,
        token: &mut GhostToken<'brand>,
        node: usize,
        start: usize,
        end: usize,
    ) {
        if start >= end || start == end - 1 {
            return;
        }
        let mid = start + (end - start) / 2;
        let left_child = 2 * node + 1;
        let right_child = 2 * node + 2;

        self.repair_recursive(token, left_child, start, mid);
        self.repair_recursive(token, right_child, mid, end);

        let left_val = self.tree.borrow(token, left_child).clone();
        let right_val = self.tree.borrow(token, right_child).clone();
        *self.tree.borrow_mut(token, node) = (self.combinator)(&left_val, &right_val);
    }

    /// Returns a mutable view of the root of the tree, allowing splitting.
    pub fn view_mut<'a>(&'a mut self) -> BrandedSegmentTreeViewMut<'a, 'brand, T, F> {
        BrandedSegmentTreeViewMut {
            ptr: self.tree.inner.as_mut_ptr(),
            node_idx: 0,
            range_start: 0,
            range_end: self.n,
            combinator: &self.combinator,
            default_value: &self.default_value,
            _marker: PhantomData,
        }
    }
}

impl<'a, 'brand, T, F> BrandedSegmentTreeViewMut<'a, 'brand, T, F>
where
    T: Clone,
    F: Fn(&T, &T) -> T,
{
    /// Returns the range covered by this view.
    pub fn range(&self) -> (usize, usize) {
        (self.range_start, self.range_end)
    }

    /// Splits the view into left and right children views.
    /// Returns `None` if this is a leaf node or empty.
    pub fn split(self) -> Option<(Self, Self)> {
        if self.range_start >= self.range_end || self.range_start == self.range_end - 1 {
            return None;
        }

        let mid = self.range_start + (self.range_end - self.range_start) / 2;
        let left_child_idx = 2 * self.node_idx + 1;
        let right_child_idx = 2 * self.node_idx + 2;

        unsafe {
            let left = Self {
                ptr: self.ptr,
                node_idx: left_child_idx,
                range_start: self.range_start,
                range_end: mid,
                combinator: self.combinator,
                default_value: self.default_value,
                _marker: PhantomData,
            };
            let right = Self {
                ptr: self.ptr,
                node_idx: right_child_idx,
                range_start: mid,
                range_end: self.range_end,
                combinator: self.combinator,
                default_value: self.default_value,
                _marker: PhantomData,
            };
            Some((left, right))
        }
    }

    /// Updates the value at `index` within this view's range.
    /// Returns `true` if the index was in range and updated.
    ///
    /// This method recursively traverses down *within this view's subtree*.
    /// It updates the path and re-combines values.
    ///
    /// Note: This mutates the underlying tree cells. Since `BrandedSegmentTreeViewMut` guarantees disjointness,
    /// it is safe to call this on disjoint views in parallel.
    pub fn update(&mut self, index: usize, value: T) -> bool {
        if index < self.range_start || index >= self.range_end {
            return false;
        }
        self.update_recursive(
            self.node_idx,
            self.range_start,
            self.range_end,
            index,
            value,
        );
        true
    }

    fn update_recursive(&mut self, node: usize, start: usize, end: usize, idx: usize, val: T) {
        if start == end - 1 {
            unsafe {
                let cell = &mut *self.ptr.add(node);
                *cell.get_mut() = val;
            }
            return;
        }

        let mid = start + (end - start) / 2;
        let left_child = 2 * node + 1;
        let right_child = 2 * node + 2;

        if idx < mid {
            self.update_recursive(left_child, start, mid, idx, val);
        } else {
            self.update_recursive(right_child, mid, end, idx, val);
        }

        // Pull up
        unsafe {
            let left_cell = &*self.ptr.add(left_child);
            // We need to read from the cell.
            // BrandedSegmentTreeViewMut grants MUTABLE access.
            // Reading is safe if we have exclusive access.
            // However, GhostCell doesn't expose `get` without token.
            // But we have `ptr` to `GhostCell`.
            // `GhostCell` is transparent wrapper around `UnsafeCell`.
            // We can get `&T` via `&mut GhostCell` -> `get_mut` -> `&mut T` -> `&T`.
            // But `left_cell` is `&GhostCell` here (from `ptr`). We need `&mut` to call `get_mut` without token.
            // Since `self` owns the *subtree* rooted at `node`, it owns `left_child` and `right_child`.
            // So we can mutably borrow them.

            let left_cell_mut = &mut *self.ptr.add(left_child);
            let left_val = left_cell_mut.get_mut(); // Safe, no token needed

            let right_cell_mut = &mut *self.ptr.add(right_child);
            let right_val = right_cell_mut.get_mut(); // Safe

            let new_val = (self.combinator)(left_val, right_val);

            let node_cell_mut = &mut *self.ptr.add(node);
            *node_cell_mut.get_mut() = new_val;
        }
    }
}

impl<'brand, T, F> BrandedCollection<'brand> for BrandedSegmentTree<'brand, T, F> {
    fn is_empty(&self) -> bool {
        self.n == 0
    }

    fn len(&self) -> usize {
        self.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_segment_tree_sum() {
        GhostToken::new(|mut token| {
            // Range Sum Query
            let mut st = BrandedSegmentTree::new(8, |a, b| a + b, 0);

            let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
            st.build(&mut token, &data);

            assert_eq!(st.query(&token, 0, 8), 36);
            assert_eq!(st.query(&token, 0, 4), 10); // 1+2+3+4
            assert_eq!(st.query(&token, 4, 8), 26); // 5+6+7+8

            st.update(&mut token, 0, 10); // 1 -> 10. Sum should increase by 9.
            assert_eq!(st.query(&token, 0, 8), 45);
        });
    }

    #[test]
    fn test_segment_tree_min() {
        GhostToken::new(|mut token| {
            // Range Minimum Query
            let mut st = BrandedSegmentTree::new(4, |a, b| std::cmp::min(*a, *b), i32::MAX);

            st.update(&mut token, 0, 10);
            st.update(&mut token, 1, 5);
            st.update(&mut token, 2, 20);
            st.update(&mut token, 3, 8);

            assert_eq!(st.query(&token, 0, 4), 5);
            assert_eq!(st.query(&token, 0, 2), 5);
            assert_eq!(st.query(&token, 2, 4), 8);
        });
    }

    #[test]
    fn test_view_mut_split() {
        GhostToken::new(|mut token| {
            let mut st = BrandedSegmentTree::new(4, |a, b| a + b, 0);

            // Build initial
            st.update(&mut token, 0, 1);
            st.update(&mut token, 1, 1);
            st.update(&mut token, 2, 1);
            st.update(&mut token, 3, 1);
            assert_eq!(st.query(&token, 0, 4), 4);

            {
                let mut view = st.view_mut();
                // Split root
                let (mut left, mut right) = view.split().unwrap();

                // Left covers [0, 2), Right covers [2, 4)
                assert_eq!(left.range(), (0, 2));
                assert_eq!(right.range(), (2, 4));

                // Update in parallel (logically)
                left.update(0, 10); // 1 -> 10
                right.update(3, 10); // 1 -> 10
            }

            st.repair(&mut token);

            // Verify
            assert_eq!(st.query(&token, 0, 4), 10 + 1 + 1 + 10);
        });
    }

    #[test]
    fn test_empty_segment_tree() {
        GhostToken::new(|mut token| {
            let mut st = BrandedSegmentTree::new(0, |a, b: &i32| a + b, 0);
            st.build(&mut token, &[]);
            st.repair(&mut token);
            assert_eq!(st.query(&token, 0, 0), 0);
        });
    }

    #[test]
    fn test_empty_view_split() {
        GhostToken::new(|mut token| {
            let mut st = BrandedSegmentTree::new(0, |a, b: &i32| a + b, 0);
            let view = st.view_mut();
            assert!(view.split().is_none());
        });
    }
}
