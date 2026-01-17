//! `BrandedBinaryHeap` — a priority queue with token-gated access.
//!
//! This implementation uses a `Vec` of `GhostCell`s as the backing storage,
//! maintaining the max-heap property. It allows safe interior mutability via
//! the `GhostCell` paradigm, while ensuring the heap invariant is preserved
//! by requiring the token for operations that observe or modify the order.

use crate::{GhostCell, GhostToken};
use core::ops::{Deref, DerefMut};

/// A priority queue implemented with a binary heap.
///
/// This will be a max-heap.
pub struct BrandedBinaryHeap<'brand, T> {
    inner: Vec<GhostCell<'brand, T>>,
}

impl<'brand, T> BrandedBinaryHeap<'brand, T> {
    /// Creates an empty binary heap.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Creates an empty binary heap with a specific capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    /// Returns the number of elements in the heap.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the heap is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Drops all items from the heap.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Returns a reference to the greatest item in the heap, or `None` if it is empty.
    pub fn peek<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        self.inner.get(0).map(|c| c.borrow(token))
    }

    /// Returns the capacity of the heap.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }
}

impl<'brand, T: Ord> BrandedBinaryHeap<'brand, T> {
    /// Pushes an item onto the binary heap.
    pub fn push(&mut self, token: &GhostToken<'brand>, item: T) {
        self.inner.push(GhostCell::new(item));
        self.sift_up(token, self.inner.len() - 1);
    }

    /// Removes the greatest item from the binary heap and returns it, or `None` if it is empty.
    pub fn pop(&mut self, token: &GhostToken<'brand>) -> Option<T> {
        if self.inner.is_empty() {
            None
        } else {
            // Swap the first element with the last
            let last_idx = self.inner.len() - 1;
            self.inner.swap(0, last_idx);

            // Remove the last element (which was the first)
            let item = self.inner.pop().unwrap().into_inner();

            // Restore heap property if there are elements left
            if !self.inner.is_empty() {
                self.sift_down(token, 0);
            }

            Some(item)
        }
    }

    /// Returns a mutable reference to the greatest item in the binary heap, or `None` if it is empty.
    ///
    /// Note: If the item is modified, the heap property might be violated. The returned `PeekMut` guard
    /// will automatically sift the heap when dropped to restore the property.
    pub fn peek_mut<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> Option<PeekMut<'a, 'brand, T>> {
        if self.inner.is_empty() {
            None
        } else {
            Some(PeekMut {
                heap: self,
                token,
                sift: false,
            })
        }
    }

    fn sift_up(&mut self, token: &GhostToken<'brand>, mut node: usize) {
        while node > 0 {
            let parent = (node - 1) / 2;
            if self.inner[node].borrow(token) <= self.inner[parent].borrow(token) {
                break;
            }
            self.inner.swap(node, parent);
            node = parent;
        }
    }

    fn sift_down(&mut self, token: &GhostToken<'brand>, mut node: usize) {
        let len = self.inner.len();
        loop {
            let left = 2 * node + 1;
            let right = 2 * node + 2;
            let mut largest = node;

            if left < len && self.inner[left].borrow(token) > self.inner[largest].borrow(token) {
                largest = left;
            }

            if right < len && self.inner[right].borrow(token) > self.inner[largest].borrow(token) {
                largest = right;
            }

            if largest != node {
                self.inner.swap(node, largest);
                node = largest;
            } else {
                break;
            }
        }
    }
}

impl<'brand, T> Default for BrandedBinaryHeap<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Structure wrapping a mutable reference to the greatest item on a `BrandedBinaryHeap`.
///
/// This `struct` is created by the [`peek_mut`] method on [`BrandedBinaryHeap`].
///
/// [`peek_mut`]: BrandedBinaryHeap::peek_mut
pub struct PeekMut<'a, 'brand, T>
where
    T: Ord,
{
    heap: &'a mut BrandedBinaryHeap<'brand, T>,
    token: &'a mut GhostToken<'brand>,
    sift: bool,
}

impl<'a, 'brand, T: Ord> Drop for PeekMut<'a, 'brand, T> {
    fn drop(&mut self) {
        if self.sift {
            self.heap.sift_down(self.token, 0);
        }
    }
}

impl<'a, 'brand, T: Ord> Deref for PeekMut<'a, 'brand, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.heap.inner[0].borrow(self.token)
    }
}

impl<'a, 'brand, T: Ord> DerefMut for PeekMut<'a, 'brand, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.sift = true;
        self.heap.inner[0].borrow_mut(self.token)
    }
}

/// Pop the top element from the heap if it leaks (via `PeekMut::pop`).
impl<'a, 'brand, T: Ord> PeekMut<'a, 'brand, T> {
    /// Removes the peeked value from the heap and returns it.
    pub fn pop(mut self) -> T {
        let value = self.heap.pop(self.token).unwrap();
        self.sift = false; // Prevent double sifting
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_binary_heap_push_pop() {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            heap.push(&token, 3);
            heap.push(&token, 5);
            heap.push(&token, 1);
            heap.push(&token, 10);
            heap.push(&token, 2);

            assert_eq!(heap.len(), 5);
            assert_eq!(heap.peek(&token), Some(&10));

            assert_eq!(heap.pop(&token), Some(10));
            assert_eq!(heap.pop(&token), Some(5));
            assert_eq!(heap.pop(&token), Some(3));
            assert_eq!(heap.pop(&token), Some(2));
            assert_eq!(heap.pop(&token), Some(1));
            assert_eq!(heap.pop(&token), None);
        });
    }

    #[test]
    fn test_binary_heap_peek_mut() {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            heap.push(&token, 3);
            heap.push(&token, 5);
            heap.push(&token, 10);

            // Peek mut and change value to be smaller
            {
                let mut top = heap.peek_mut(&mut token).unwrap();
                assert_eq!(*top, 10);
                *top = 2;
                // Drop will sift down
            }

            // Now heap should be 5, 3, 2 (max heap)
            assert_eq!(heap.pop(&token), Some(5));
            assert_eq!(heap.pop(&token), Some(3));
            assert_eq!(heap.pop(&token), Some(2));
        });
    }

    #[test]
    fn test_binary_heap_peek_mut_pop() {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            heap.push(&token, 3);
            heap.push(&token, 10);

            {
                let top = heap.peek_mut(&mut token).unwrap();
                assert_eq!(top.pop(), 10);
            }

            assert_eq!(heap.pop(&token), Some(3));
        });
    }

    #[test]
    fn test_binary_heap_capacity() {
        GhostToken::new(|token| {
            let mut heap = BrandedBinaryHeap::with_capacity(10);
            assert_eq!(heap.capacity(), 10);

            heap.push(&token, 1);
            heap.reserve(20);
            assert!(heap.capacity() >= 21);
        });
    }
}
