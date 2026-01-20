//! `BrandedBinaryHeap` — a priority queue implemented with a binary heap.
//!
//! This implementation uses `BrandedVec` as the backing storage.
//! It supports standard max-heap operations with token-gated access.

use crate::collections::vec::BrandedVec;
use crate::collections::ZeroCopyOps;
use crate::GhostToken;
use core::cmp::Ord;
use core::fmt;

/// A priority queue implemented with a binary heap.
///
/// This structure guarantees that the top element is always the greatest element.
/// Access to the elements is controlled by a `GhostToken`.
pub struct BrandedBinaryHeap<'brand, T> {
    data: BrandedVec<'brand, T>,
}

impl<'brand, T: Ord> BrandedBinaryHeap<'brand, T> {
    /// Creates an empty binary heap.
    pub fn new() -> Self {
        Self {
            data: BrandedVec::new(),
        }
    }

    /// Creates an empty binary heap with a specific capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: BrandedVec::with_capacity(capacity),
        }
    }

    /// Returns the number of elements in the heap.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the heap is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns the capacity of the heap.
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// Pushes an item onto the binary heap.
    pub fn push(&mut self, token: &mut GhostToken<'brand>, item: T) {
        self.data.push(item);
        self.sift_up(token, self.data.len() - 1);
    }

    /// Pops the greatest item from the binary heap.
    pub fn pop(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }
        let last_idx = self.data.len() - 1;
        self.data.swap(0, last_idx);
        let item = self.data.pop()?;
        if !self.data.is_empty() {
            self.sift_down(token, 0);
        }
        Some(item)
    }

    /// Returns a reference to the greatest item in the binary heap.
    pub fn peek<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        self.data.get(token, 0)
    }

    /// Clears the binary heap.
    pub fn clear(&mut self) {
        // Inefficient but safe clear without modifying BrandedVec API
        while let Some(_) = self.data.pop() {}
    }

    fn sift_up(&mut self, token: &mut GhostToken<'brand>, mut node: usize) {
        while node > 0 {
            let parent = (node - 1) / 2;
            if self.less(token, parent, node) {
                self.data.swap(parent, node);
                node = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, token: &mut GhostToken<'brand>, mut node: usize) {
        let len = self.data.len();
        loop {
            let left = 2 * node + 1;
            if left >= len {
                break;
            }
            let right = left + 1;
            let mut greater = left;
            if right < len && self.less(token, left, right) {
                greater = right;
            }

            if self.less(token, node, greater) {
                self.data.swap(node, greater);
                node = greater;
            } else {
                break;
            }
        }
    }

    // Helper to compare two elements in the heap
    fn less(&self, token: &GhostToken<'brand>, a: usize, b: usize) -> bool {
        let val_a = self.data.borrow(token, a);
        let val_b = self.data.borrow(token, b);
        val_a < val_b
    }
}

impl<'brand, T> BrandedBinaryHeap<'brand, T> {
    /// Iterates over all elements in the heap in arbitrary order.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> core::slice::Iter<'a, T> {
        self.data.iter(token)
    }
}

impl<'brand, T> ZeroCopyOps<'brand, T> for BrandedBinaryHeap<'brand, T> {
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.data.find_ref(token, f)
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.data.any_ref(token, f)
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.data.all_ref(token, f)
    }
}

impl<'brand, T: Ord> Default for BrandedBinaryHeap<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T: fmt::Debug + Ord> fmt::Debug for BrandedBinaryHeap<'brand, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrandedBinaryHeap")
            .field("len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_binary_heap_basic() {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            heap.push(&mut token, 1);
            heap.push(&mut token, 5);
            heap.push(&mut token, 2);
            heap.push(&mut token, 10);

            assert_eq!(heap.peek(&token), Some(&10));
            assert_eq!(heap.pop(&mut token), Some(10));
            assert_eq!(heap.peek(&token), Some(&5));
            assert_eq!(heap.pop(&mut token), Some(5));
            assert_eq!(heap.pop(&mut token), Some(2));
            assert_eq!(heap.pop(&mut token), Some(1));
            assert_eq!(heap.pop(&mut token), None);
        });
    }

    #[test]
    fn test_binary_heap_order() {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            let data = vec![1, 10, 5, 2, 8, 3, 7];
            for &x in &data {
                heap.push(&mut token, x);
            }

            let mut result = Vec::new();
            while let Some(x) = heap.pop(&mut token) {
                result.push(x);
            }

            let mut expected = data;
            expected.sort();
            expected.reverse();

            assert_eq!(result, expected);
        });
    }

    #[test]
    fn test_iter_and_zero_copy() {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            heap.push(&mut token, 1);
            heap.push(&mut token, 3);
            heap.push(&mut token, 2);

            // Test iter (order is arbitrary but all elements should be present)
            let count = heap.iter(&token).count();
            assert_eq!(count, 3);

            // Test zero copy ops
            assert_eq!(heap.find_ref(&token, |&x| x == 2), Some(&2));
            assert!(heap.any_ref(&token, |&x| x == 3));
            assert!(heap.all_ref(&token, |&x| x > 0));
        });
    }
}
