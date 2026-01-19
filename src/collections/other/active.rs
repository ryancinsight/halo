//! Active wrappers for `other` collections.

use crate::GhostToken;
use super::{BrandedDoublyLinkedList, BrandedBinaryHeap};
use super::doubly_linked_list::{BrandedDoublyLinkedListIter, BrandedDoublyLinkedListIterMut};
use core::cmp::Ord;

/// A wrapper around a mutable reference to a `BrandedDoublyLinkedList` and a mutable reference to a `GhostToken`.
pub struct ActiveDoublyLinkedList<'a, 'brand, T> {
    list: &'a mut BrandedDoublyLinkedList<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveDoublyLinkedList<'a, 'brand, T> {
    /// Creates a new active list handle.
    pub fn new(list: &'a mut BrandedDoublyLinkedList<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
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
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveDoublyLinkedList<'a, 'brand, T>;
}

impl<'brand, T> ActivateDoublyLinkedList<'brand, T> for BrandedDoublyLinkedList<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveDoublyLinkedList<'a, 'brand, T> {
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
    pub fn new(heap: &'a mut BrandedBinaryHeap<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
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
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveBinaryHeap<'a, 'brand, T>;
}

impl<'brand, T: Ord> ActivateBinaryHeap<'brand, T> for BrandedBinaryHeap<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveBinaryHeap<'a, 'brand, T> {
        ActiveBinaryHeap::new(self, token)
    }
}
