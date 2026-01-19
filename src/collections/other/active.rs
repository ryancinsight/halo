//! Active wrappers for other collections.
//!
//! These wrappers bundle the collection with a mutable reference to the `GhostToken`,
//! reducing token redundancy in API calls.

use crate::GhostToken;
use super::{BrandedSkipList, BrandedDoublyLinkedList, BrandedBinaryHeap, BrandedCowStrings, BrandedSlotMap, SlotKey};
use super::doubly_linked_list::{CursorMut, BrandedDoublyLinkedListIter, BrandedDoublyLinkedListIterMut};
use crate::collections::BrandedCollection;
use std::borrow::Cow;
use std::hash::{Hash, BuildHasher};

/// Active wrapper for `BrandedSkipList`.
pub struct ActiveSkipList<'a, 'brand, K, V> {
    list: &'a mut BrandedSkipList<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, V> ActiveSkipList<'a, 'brand, K, V> {
    pub fn new(list: &'a mut BrandedSkipList<'brand, K, V>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { list, token }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

impl<'a, 'brand, K, V> ActiveSkipList<'a, 'brand, K, V>
where K: Ord
{
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where K: std::borrow::Borrow<Q>, Q: Ord
    {
        self.list.get(self.token, key)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where K: std::borrow::Borrow<Q>, Q: Ord
    {
        self.list.get_mut(self.token, key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.list.insert(self.token, key, value)
    }

    pub fn iter(&self) -> super::skip_list::Iter<'_, 'brand, K, V> {
        self.list.iter(self.token)
    }
}

pub trait ActivateSkipList<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveSkipList<'a, 'brand, K, V>;
}

impl<'brand, K, V> ActivateSkipList<'brand, K, V> for BrandedSkipList<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveSkipList<'a, 'brand, K, V> {
        ActiveSkipList::new(self, token)
    }
}

/// Active wrapper for `BrandedDoublyLinkedList`.
pub struct ActiveDoublyLinkedList<'a, 'brand, T> {
    list: &'a mut BrandedDoublyLinkedList<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveDoublyLinkedList<'a, 'brand, T> {
    pub fn new(list: &'a mut BrandedDoublyLinkedList<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { list, token }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn push_front(&mut self, value: T) -> usize {
        self.list.push_front(self.token, value)
    }

    pub fn push_back(&mut self, value: T) -> usize {
        self.list.push_back(self.token, value)
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.list.pop_front(self.token)
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.list.pop_back(self.token)
    }

    pub fn front(&self) -> Option<&T> {
        self.list.front(self.token)
    }

    pub fn back(&self) -> Option<&T> {
        self.list.back(self.token)
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.list.get(self.token, index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.list.get_mut(self.token, index)
    }

    pub fn iter(&self) -> BrandedDoublyLinkedListIter<'_, 'brand, T> {
        self.list.iter(self.token)
    }

    pub fn iter_mut(&mut self) -> BrandedDoublyLinkedListIterMut<'_, 'brand, T> {
        self.list.iter_mut(self.token)
    }

    pub fn move_to_front(&mut self, index: usize) {
        self.list.move_to_front(self.token, index)
    }

    pub fn move_to_back(&mut self, index: usize) {
        self.list.move_to_back(self.token, index)
    }

    pub fn cursor_front(&'a mut self) -> ActiveCursorMut<'a, 'brand, T> {
        let cursor = self.list.cursor_front();
        ActiveCursorMut {
            cursor,
            token: self.token
        }
    }

    pub fn cursor_back(&'a mut self) -> ActiveCursorMut<'a, 'brand, T> {
        let cursor = self.list.cursor_back();
        ActiveCursorMut {
            cursor,
            token: self.token
        }
    }
}

pub struct ActiveCursorMut<'a, 'brand, T> {
    cursor: CursorMut<'a, 'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveCursorMut<'a, 'brand, T> {
    pub fn index(&self) -> Option<usize> {
        self.cursor.index()
    }

    pub fn current(&self) -> Option<&T> {
        self.cursor.current(self.token)
    }

    pub fn current_mut(&mut self) -> Option<&mut T> {
        self.cursor.current_mut(self.token)
    }

    pub fn move_next(&mut self) {
        self.cursor.move_next(self.token)
    }

    pub fn move_prev(&mut self) {
        self.cursor.move_prev(self.token)
    }

    pub fn insert_after(&mut self, value: T) -> usize {
        self.cursor.insert_after(self.token, value)
    }

    pub fn insert_before(&mut self, value: T) -> usize {
        self.cursor.insert_before(self.token, value)
    }

    pub fn remove_current(&mut self) -> Option<T> {
        self.cursor.remove_current(self.token)
    }
}

pub trait ActivateDoublyLinkedList<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveDoublyLinkedList<'a, 'brand, T>;
}

impl<'brand, T> ActivateDoublyLinkedList<'brand, T> for BrandedDoublyLinkedList<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveDoublyLinkedList<'a, 'brand, T> {
        ActiveDoublyLinkedList::new(self, token)
    }
}

/// Active wrapper for `BrandedBinaryHeap`.
pub struct ActiveBinaryHeap<'a, 'brand, T> {
    heap: &'a mut BrandedBinaryHeap<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveBinaryHeap<'a, 'brand, T> {
    pub fn new(heap: &'a mut BrandedBinaryHeap<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { heap, token }
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.heap.iter(self.token)
    }
}

impl<'a, 'brand, T: Ord> ActiveBinaryHeap<'a, 'brand, T> {
    pub fn clear(&mut self) {
        self.heap.clear()
    }

    pub fn push(&mut self, item: T) {
        self.heap.push(self.token, item)
    }

    pub fn pop(&mut self) -> Option<T> {
        self.heap.pop(self.token)
    }

    pub fn peek(&self) -> Option<&T> {
        self.heap.peek(self.token)
    }
}

pub trait ActivateBinaryHeap<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveBinaryHeap<'a, 'brand, T>;
}

impl<'brand, T> ActivateBinaryHeap<'brand, T> for BrandedBinaryHeap<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveBinaryHeap<'a, 'brand, T> {
        ActiveBinaryHeap::new(self, token)
    }
}

/// Active wrapper for `BrandedCowStrings`.
pub struct ActiveCowStrings<'a, 'brand> {
    strings: &'a mut BrandedCowStrings<'brand>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand> ActiveCowStrings<'a, 'brand> {
    pub fn new(strings: &'a mut BrandedCowStrings<'brand>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { strings, token }
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    pub fn insert(&mut self, s: Cow<'brand, str>) -> usize {
        self.strings.insert(self.token, s)
    }

    pub fn insert_borrowed(&mut self, s: &'brand str) -> usize {
        self.strings.insert_borrowed(self.token, s)
    }

    pub fn insert_owned(&mut self, s: String) -> usize {
        self.strings.insert_owned(self.token, s)
    }

    pub fn get(&self, idx: usize) -> Option<&Cow<'brand, str>> {
        self.strings.get(self.token, idx)
    }

    pub fn get_by_value(&self, value: &str) -> Option<&Cow<'brand, str>> {
        self.strings.get_by_value(self.token, value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Cow<'brand, str>> {
        self.strings.iter(self.token)
    }
}

pub trait ActivateCowStrings<'brand> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveCowStrings<'a, 'brand>;
}

impl<'brand> ActivateCowStrings<'brand> for BrandedCowStrings<'brand> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveCowStrings<'a, 'brand> {
        ActiveCowStrings::new(self, token)
    }
}

/// Active wrapper for `BrandedSlotMap`.
pub struct ActiveSlotMap<'a, 'brand, T> {
    map: &'a mut BrandedSlotMap<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveSlotMap<'a, 'brand, T> {
    pub fn new(map: &'a mut BrandedSlotMap<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { map, token }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn insert(&mut self, value: T) -> SlotKey<'brand> {
        self.map.insert(self.token, value)
    }

    pub fn get(&self, key: SlotKey<'brand>) -> Option<&T> {
        self.map.get(self.token, key)
    }

    pub fn get_mut(&mut self, key: SlotKey<'brand>) -> Option<&mut T> {
        self.map.get_mut(self.token, key)
    }

    pub fn remove(&mut self, key: SlotKey<'brand>) -> Option<T> {
        self.map.remove(self.token, key)
    }

    pub fn contains_key(&self, key: SlotKey<'brand>) -> bool {
        self.map.contains_key(self.token, key)
    }

    pub fn clear(&mut self) {
        self.map.clear(self.token)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SlotKey<'brand>, &T)> {
        self.map.iter(self.token)
    }
}

pub trait ActivateSlotMap<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveSlotMap<'a, 'brand, T>;
}

impl<'brand, T> ActivateSlotMap<'brand, T> for BrandedSlotMap<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveSlotMap<'a, 'brand, T> {
        ActiveSlotMap::new(self, token)
    }
}
