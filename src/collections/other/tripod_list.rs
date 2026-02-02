//! `TripodList` — a token-gated doubly linked list with parent pointers.
//!
//! This structure implements the "Tripod" pattern: nodes have three links (Prev, Next, Parent).
//! It is useful for representing hierarchical linear structures, such as children lists in a tree
//! or DOM-like structures where elements need to know their container.
//!
//! Backed by `BrandedPool` for zero-overhead, cache-friendly storage.

use crate::alloc::pool::{PoolView, PoolViewMut};
use crate::alloc::BrandedPool;
use crate::collections::ZeroCopyOps;
use crate::token::traits::{GhostBorrow, GhostBorrowMut};
// use crate::GhostCell;
// use core::fmt;
use core::marker::PhantomData;

/// Internal node structure with 3 legs (Tripod).
struct TripodNode<T> {
    prev: Option<usize>,
    next: Option<usize>,
    parent: Option<usize>,
    value: T,
}

/// Zero-cost iterator for TripodList.
struct TripodListIter<'a, 'brand, T, Token>
where
    Token: GhostBorrow<'brand>,
{
    view: PoolView<'a, TripodNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    remaining: usize,
    _marker: PhantomData<(&'a Token, &'brand ())>,
}

impl<'a, 'brand, T, Token> Iterator for TripodListIter<'a, 'brand, T, Token>
where
    Token: GhostBorrow<'brand>,
{
    type Item = &'a T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let idx = self.head?;
        if idx < self.view.storage.len() {
             unsafe {
                let node = &self.view.storage[idx].occupied;
                self.head = node.next;
                self.remaining -= 1;
                Some(&node.value)
            }
        } else {
            None
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, 'brand, T, Token> DoubleEndedIterator for TripodListIter<'a, 'brand, T, Token>
where
    Token: GhostBorrow<'brand>,
{
    #[inline(always)]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let idx = self.tail?;
        if idx < self.view.storage.len() {
             unsafe {
                let node = &self.view.storage[idx].occupied;
                self.tail = node.prev;
                self.remaining -= 1;
                Some(&node.value)
            }
        } else {
            None
        }
    }
}

impl<'a, 'brand, T, Token> std::iter::FusedIterator for TripodListIter<'a, 'brand, T, Token>
where
    Token: GhostBorrow<'brand>,
{}

impl<'a, 'brand, T, Token> ExactSizeIterator for TripodListIter<'a, 'brand, T, Token>
where
    Token: GhostBorrow<'brand>,
{}

/// Mutable iterator for TripodList.
struct TripodListIterMut<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    view: PoolViewMut<'a, TripodNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    remaining: usize,
    _marker: PhantomData<(&'a mut Token, &'brand ())>,
}

impl<'a, 'brand, T, Token> Iterator for TripodListIterMut<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    type Item = &'a mut T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let idx = self.head?;
        unsafe {
            if idx >= self.view.storage.len() {
                return None;
            }
            let ptr = self.view.storage.as_mut_ptr().add(idx);
            let node = &mut (*ptr).occupied;
            self.head = node.next;
            self.remaining -= 1;
            Some(&mut node.value)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, 'brand, T, Token> DoubleEndedIterator for TripodListIterMut<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    #[inline(always)]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let idx = self.tail?;
        unsafe {
            if idx >= self.view.storage.len() {
                return None;
            }
            let ptr = self.view.storage.as_mut_ptr().add(idx);
            let node = &mut (*ptr).occupied;
            self.tail = node.prev;
            self.remaining -= 1;
            Some(&mut node.value)
        }
    }
}

impl<'a, 'brand, T, Token> std::iter::FusedIterator for TripodListIterMut<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{}

impl<'a, 'brand, T, Token> ExactSizeIterator for TripodListIterMut<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{}

/// A doubly linked list where each node has a parent pointer.
pub struct TripodList<'brand, T> {
    pool: BrandedPool<'brand, TripodNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    len: usize,
    /// The default parent index for new nodes (optional).
    default_parent: Option<usize>,
}

impl<'brand, T> TripodList<'brand, T> {
    /// Creates a new empty TripodList.
    pub fn new() -> Self {
        Self {
            pool: BrandedPool::new(),
            head: None,
            tail: None,
            len: 0,
            default_parent: None,
        }
    }

    /// Sets a default parent index that will be assigned to all new nodes.
    pub fn set_default_parent(&mut self, parent: Option<usize>) {
        self.default_parent = parent;
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Pushes an element to the front.
    pub fn push_front<Token>(&mut self, token: &mut Token, value: T) -> usize
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        let node = TripodNode {
            prev: None,
            next: self.head,
            parent: self.default_parent,
            value,
        };

        let new_idx = self.pool.alloc(token, node);
        let old_head = self.head;

        if let Some(head_idx) = old_head {
            if let Some(node) = self.pool.get_mut(token, head_idx) {
                node.prev = Some(new_idx);
            }
        } else {
            self.tail = Some(new_idx);
        }

        self.head = Some(new_idx);
        self.len += 1;
        new_idx
    }

    /// Pushes an element to the back.
    pub fn push_back<Token>(&mut self, token: &mut Token, value: T) -> usize
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        let node = TripodNode {
            prev: self.tail,
            next: None,
            parent: self.default_parent,
            value,
        };

        let new_idx = self.pool.alloc(token, node);
        let old_tail = self.tail;

        if let Some(tail_idx) = old_tail {
            if let Some(node) = self.pool.get_mut(token, tail_idx) {
                node.next = Some(new_idx);
            }
        } else {
            self.head = Some(new_idx);
        }

        self.tail = Some(new_idx);
        self.len += 1;
        new_idx
    }

    /// Pops an element from the front.
    pub fn pop_front<Token>(&mut self, token: &mut Token) -> Option<T>
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        let head_idx = self.head?;
        let node = unsafe { self.pool.take(token, head_idx) };
        let next_idx = node.next;

        if let Some(next) = next_idx {
            if let Some(next_node) = self.pool.get_mut(token, next) {
                next_node.prev = None;
            }
            self.head = Some(next);
        } else {
            self.head = None;
            self.tail = None;
        }

        self.len -= 1;
        Some(node.value)
    }

    /// Pops an element from the back.
    pub fn pop_back<Token>(&mut self, token: &mut Token) -> Option<T>
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        let tail_idx = self.tail?;
        let node = unsafe { self.pool.take(token, tail_idx) };
        let prev_idx = node.prev;

        if let Some(prev) = prev_idx {
            if let Some(prev_node) = self.pool.get_mut(token, prev) {
                prev_node.next = None;
            }
            self.tail = Some(prev);
        } else {
            self.head = None;
            self.tail = None;
        }

        self.len -= 1;
        Some(node.value)
    }

    /// Returns a reference to the front element.
    pub fn front<'a, Token>(&'a self, token: &'a Token) -> Option<&'a T>
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        let head_idx = self.head?;
        self.pool.get(token, head_idx).map(|n| &n.value)
    }

    /// Returns a reference to the back element.
    pub fn back<'a, Token>(&'a self, token: &'a Token) -> Option<&'a T>
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        let tail_idx = self.tail?;
        self.pool.get(token, tail_idx).map(|n| &n.value)
    }

    /// Gets the parent index of a node at `index`.
    pub fn get_parent<Token>(&self, token: &Token, index: usize) -> Option<usize>
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        self.pool.get(token, index).and_then(|n| n.parent)
    }

    /// Sets the parent index of a node at `index`.
    pub fn set_parent<Token>(
        &mut self,
        token: &mut Token,
        index: usize,
        parent: Option<usize>,
    ) where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        if let Some(node) = self.pool.get_mut(token, index) {
            node.parent = parent;
        }
    }

    /// Iterates over the list.
    pub fn iter<'a, Token>(
        &'a self,
        token: &'a Token,
    ) -> impl Iterator<Item = &'a T> + DoubleEndedIterator + ExactSizeIterator + std::iter::FusedIterator + 'a + use<'a, 'brand, T, Token>
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        TripodListIter::<_, Token> {
            view: self.pool.view(token),
            head: self.head,
            tail: self.tail,
            remaining: self.len,
            _marker: PhantomData,
        }
    }

    /// Iterates over the list mutably.
    pub fn iter_mut<'a, Token>(
        &'a self,
        token: &'a mut Token,
    ) -> impl Iterator<Item = &'a mut T> + DoubleEndedIterator + ExactSizeIterator + std::iter::FusedIterator + 'a + use<'a, 'brand, T, Token>
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        TripodListIterMut::<_, Token> {
            view: self.pool.view_mut(token),
            head: self.head,
            tail: self.tail,
            remaining: self.len,
            _marker: PhantomData,
        }
    }

    /// Creates a cursor at the front.
    pub fn cursor_front<'a>(&'a mut self) -> TripodCursorMut<'a, 'brand, T> {
        let head = self.head;
        TripodCursorMut {
            list: self,
            current: head,
        }
    }
}

impl<'brand, T> Default for TripodList<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T> ZeroCopyOps<'brand, T> for TripodList<'brand, T> {
    fn find_ref<'a, F, Token>(&'a self, token: &'a Token, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        self.iter(token).find(|&item| f(item))
    }

    fn any_ref<F, Token>(&self, token: &Token, f: F) -> bool
    where
        F: Fn(&T) -> bool,
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        self.iter(token).any(|item| f(item))
    }

    fn all_ref<F, Token>(&self, token: &Token, f: F) -> bool
    where
        F: Fn(&T) -> bool,
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        self.iter(token).all(|item| f(item))
    }
}

/// A mutable cursor for TripodList.
pub struct TripodCursorMut<'a, 'brand, T> {
    list: &'a mut TripodList<'brand, T>,
    current: Option<usize>,
}

impl<'a, 'brand, T> TripodCursorMut<'a, 'brand, T> {
    /// Returns reference to current element.
    pub fn current<'b, Token>(&'b self, token: &'b Token) -> Option<&'b T>
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        let idx = self.current?;
        self.list.pool.get(token, idx).map(|n| &n.value)
    }

    /// Returns mutable reference to current element.
    pub fn current_mut<'b, Token>(&'b mut self, token: &'b mut Token) -> Option<&'b mut T>
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        let idx = self.current?;
        self.list.pool.get_mut(token, idx).map(|n| &mut n.value)
    }

    /// Moves to next element.
    pub fn move_next<Token>(&mut self, token: &Token)
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        if let Some(idx) = self.current {
            if let Some(node) = self.list.pool.get(token, idx) {
                self.current = node.next;
            }
        } else {
            self.current = self.list.head;
        }
    }

    /// Returns the parent of the current element.
    pub fn parent<Token>(&self, token: &Token) -> Option<usize>
    where
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        let idx = self.current?;
        self.list.pool.get(token, idx).and_then(|n| n.parent)
    }

    /// Sets the parent of the current element.
    pub fn set_parent<Token>(&mut self, token: &mut Token, parent: Option<usize>)
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        if let Some(idx) = self.current {
            if let Some(node) = self.list.pool.get_mut(token, idx) {
                node.parent = parent;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_tripod_basic() {
        GhostToken::new(|mut token| {
            let mut list = TripodList::new();
            list.push_back(&mut token, 1);
            list.push_back(&mut token, 2);
            assert_eq!(list.len(), 2);
            assert_eq!(list.pop_front(&mut token), Some(1));
            assert_eq!(list.pop_front(&mut token), Some(2));
        });
    }

    #[test]
    fn test_tripod_parent() {
        GhostToken::new(|mut token| {
            let mut list = TripodList::new();
            // Assume 999 is some valid parent index in another structure or purely symbolic
            list.set_default_parent(Some(999));

            let idx1 = list.push_back(&mut token, 10);
            let idx2 = list.push_back(&mut token, 20);

            assert_eq!(list.get_parent(&token, idx1), Some(999));
            assert_eq!(list.get_parent(&token, idx2), Some(999));

            // Change parent of node 2
            list.set_parent(&mut token, idx2, Some(888));
            assert_eq!(list.get_parent(&token, idx2), Some(888));
            assert_eq!(list.get_parent(&token, idx1), Some(999));
        });
    }

    #[test]
    fn test_cursor_parent() {
        GhostToken::new(|mut token| {
            let mut list = TripodList::new();
            list.push_back(&mut token, 1);
            let mut cursor = list.cursor_front();

            cursor.set_parent(&mut token, Some(123));
            assert_eq!(cursor.parent(&token), Some(123));
        });
    }
}
