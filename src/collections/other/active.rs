//! Active wrappers for `other` collections.

use super::deque::BrandedDequeIter;
use super::doubly_linked_list::{BrandedDoublyLinkedListIter, BrandedDoublyLinkedListIterMut};
use super::{BrandedBinaryHeap, BrandedDeque, BrandedDoublyLinkedList, BrandedFenwickTree};
use crate::GhostToken;
use core::cmp::Ord;
use core::ops::{AddAssign, SubAssign};

/// A wrapper around a mutable reference to a `BrandedDoublyLinkedList` and a mutable reference to a `GhostToken`.
pub struct ActiveDoublyLinkedList<'a, 'brand, T> {
    list: &'a mut BrandedDoublyLinkedList<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveDoublyLinkedList<'a, 'brand, T> {
    /// Creates a new active list handle.
    pub fn new(
        list: &'a mut BrandedDoublyLinkedList<'brand, T>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { list, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Clears the list.
    pub fn clear(&mut self) {
        self.list.clear(self.token);
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) -> usize {
        self.list.push_front(self.token, value)
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) -> usize {
        self.list.push_back(self.token, value)
    }

    /// Pops an element from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.list.pop_front(self.token)
    }

    /// Pops an element from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.list.pop_back(self.token)
    }

    /// Returns a shared reference to the front element.
    pub fn front(&self) -> Option<&T> {
        self.list.front(self.token)
    }

    /// Returns a shared reference to the back element.
    pub fn back(&self) -> Option<&T> {
        self.list.back(self.token)
    }

    /// Returns a shared reference to the element at the given index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.list.get(self.token, index)
    }

    /// Returns a mutable reference to the element at the given index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.list.get_mut(self.token, index)
    }

    /// Iterates over the list elements.
    pub fn iter(&self) -> BrandedDoublyLinkedListIter<'_, 'brand, T> {
        self.list.iter(self.token)
    }

    /// Iterates over the list elements mutably.
    pub fn iter_mut(&mut self) -> BrandedDoublyLinkedListIterMut<'_, 'brand, T> {
        self.list.iter_mut(self.token)
    }

    /// Moves the node at `index` to the front.
    pub fn move_to_front(&mut self, index: usize) {
        self.list.move_to_front(self.token, index)
    }

    /// Moves the node at `index` to the back.
    pub fn move_to_back(&mut self, index: usize) {
        self.list.move_to_back(self.token, index)
    }
}

/// Extension trait to easily create ActiveDoublyLinkedList from BrandedDoublyLinkedList.
pub trait ActivateDoublyLinkedList<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveDoublyLinkedList<'a, 'brand, T>;
}

impl<'brand, T> ActivateDoublyLinkedList<'brand, T> for BrandedDoublyLinkedList<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveDoublyLinkedList<'a, 'brand, T> {
        ActiveDoublyLinkedList::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedBinaryHeap` and a mutable reference to a `GhostToken`.
pub struct ActiveBinaryHeap<'a, 'brand, T> {
    heap: &'a mut BrandedBinaryHeap<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T: Ord> ActiveBinaryHeap<'a, 'brand, T> {
    /// Creates a new active heap handle.
    pub fn new(
        heap: &'a mut BrandedBinaryHeap<'brand, T>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { heap, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Returns the capacity.
    pub fn capacity(&self) -> usize {
        self.heap.capacity()
    }

    /// Pushes an item.
    pub fn push(&mut self, item: T) {
        self.heap.push(self.token, item)
    }

    /// Pops the greatest item.
    pub fn pop(&mut self) -> Option<T> {
        self.heap.pop(self.token)
    }

    /// Returns a reference to the greatest item.
    pub fn peek(&self) -> Option<&T> {
        self.heap.peek(self.token)
    }

    /// Clears the heap.
    pub fn clear(&mut self) {
        self.heap.clear()
    }

    /// Iterates over elements.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.heap.iter(self.token)
    }
}

/// Extension trait to easily create ActiveBinaryHeap from BrandedBinaryHeap.
pub trait ActivateBinaryHeap<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveBinaryHeap<'a, 'brand, T>;
}

impl<'brand, T: Ord> ActivateBinaryHeap<'brand, T> for BrandedBinaryHeap<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveBinaryHeap<'a, 'brand, T> {
        ActiveBinaryHeap::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedDeque` (fixed size ring buffer) and a mutable reference to a `GhostToken`.
pub struct ActiveDeque<'a, 'brand, T, const CAPACITY: usize> {
    deque: &'a mut BrandedDeque<'brand, T, CAPACITY>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T, const CAPACITY: usize> ActiveDeque<'a, 'brand, T, CAPACITY> {
    /// Creates a new active deque handle.
    pub fn new(
        deque: &'a mut BrandedDeque<'brand, T, CAPACITY>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { deque, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.deque.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }

    /// Returns `true` if full.
    pub fn is_full(&self) -> bool {
        self.deque.is_full()
    }

    /// Clears the deque.
    pub fn clear(&mut self) {
        self.deque.clear();
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) -> Option<()> {
        self.deque.push_back(value)
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) -> Option<()> {
        self.deque.push_front(value)
    }

    /// Pops from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.deque.pop_back().map(|c| c.into_inner())
    }

    /// Pops from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.deque.pop_front().map(|c| c.into_inner())
    }

    /// Returns the front element.
    pub fn front(&self) -> Option<&T> {
        self.deque.front(self.token)
    }

    /// Returns the back element.
    pub fn back(&self) -> Option<&T> {
        self.deque.back(self.token)
    }

    /// Returns a shared reference to the element at `idx`.
    pub fn get(&self, idx: usize) -> Option<&T> {
        self.deque.get(self.token, idx)
    }

    /// Returns a mutable reference to the element at `idx`.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        self.deque.get_mut(self.token, idx)
    }

    /// Iterates over elements.
    pub fn iter(&self) -> BrandedDequeIter<'_, 'brand, T, CAPACITY> {
        self.deque.iter(self.token)
    }

    /// Bulk operation.
    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(&T),
    {
        self.deque.for_each(self.token, f)
    }

    /// Bulk mutation.
    pub fn for_each_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut T),
    {
        self.deque.for_each_mut(self.token, f)
    }
}

/// Extension trait to easily create ActiveDeque from BrandedDeque.
pub trait ActivateDeque<'brand, T, const CAPACITY: usize> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveDeque<'a, 'brand, T, CAPACITY>;
}

impl<'brand, T, const CAPACITY: usize> ActivateDeque<'brand, T, CAPACITY>
    for BrandedDeque<'brand, T, CAPACITY>
{
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveDeque<'a, 'brand, T, CAPACITY> {
        ActiveDeque::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedFenwickTree` and a mutable reference to a `GhostToken`.
pub struct ActiveFenwickTree<'a, 'brand, T> {
    tree: &'a mut BrandedFenwickTree<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveFenwickTree<'a, 'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    /// Creates a new active Fenwick Tree handle.
    pub fn new(
        tree: &'a mut BrandedFenwickTree<'brand, T>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { tree, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// Adds `delta` to the element at `index`.
    pub fn add(&mut self, index: usize, delta: T) {
        self.tree.add(self.token, index, delta)
    }

    /// Computes prefix sum.
    pub fn prefix_sum(&self, index: usize) -> T {
        self.tree.prefix_sum(self.token, index)
    }

    /// Computes range sum.
    pub fn range_sum(&self, start: usize, end: usize) -> T {
        self.tree.range_sum(self.token, start, end)
    }

    /// Pushes a new value.
    pub fn push(&mut self, val: T) {
        self.tree.push(self.token, val)
    }

    /// Clears the tree.
    pub fn clear(&mut self) {
        self.tree.clear()
    }
}

/// Extension trait to easily create ActiveFenwickTree.
pub trait ActivateFenwickTree<'brand, T> {
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveFenwickTree<'a, 'brand, T>;
}

impl<'brand, T> ActivateFenwickTree<'brand, T> for BrandedFenwickTree<'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveFenwickTree<'a, 'brand, T> {
        ActiveFenwickTree::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_fenwick_tree() {
        GhostToken::new(|mut token| {
            let mut ft = BrandedFenwickTree::<i64>::new();
            let mut active = ft.activate(&mut token);

            for _ in 0..5 {
                active.push(0);
            }

            active.add(0, 10);
            active.add(2, 20);

            assert_eq!(active.prefix_sum(0), 10);
            assert_eq!(active.prefix_sum(2), 30);
            assert_eq!(active.range_sum(1, 3), 20);
        });
    }
}
