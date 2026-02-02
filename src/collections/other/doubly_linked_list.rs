//! `BrandedDoublyLinkedList` — a token-gated doubly linked list.
//!
//! This implementation uses a `BrandedPool` as the backing storage for nodes,
//! allowing safe index-based pointers with the `GhostCell` pattern.
//! It supports O(1) insertion and removal at arbitrary positions via Cursors.

use crate::alloc::pool::{PoolView, PoolViewMut};
use crate::alloc::BrandedPool;
use crate::collections::ZeroCopyOps;
// use crate::GhostCell;
// use crate::GhostToken;
use core::fmt;
use core::marker::PhantomData;

/// Internal node structure.
struct ListNode<T> {
    prev: Option<usize>,
    next: Option<usize>,
    value: T,
}
use crate::token::traits::{GhostBorrow, GhostBorrowMut};

/// Zero-cost iterator for BrandedDoublyLinkedList.
struct BrandedDoublyLinkedListIter<'a, 'brand, T, Token>
where
    Token: GhostBorrow<'brand>,
{
    view: PoolView<'a, ListNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    remaining: usize,
    _phantom: PhantomData<(&'a Token, &'brand ())>,
}

impl<'a, 'brand, T, Token> Iterator for BrandedDoublyLinkedListIter<'a, 'brand, T, Token>
where
    T: 'a,
    Token: GhostBorrow<'brand>,
{
    type Item = &'a T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let idx = self.head?;
        // Check occupancy (sanity check, though pool should be valid)
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

impl<'a, 'brand, T, Token> DoubleEndedIterator for BrandedDoublyLinkedListIter<'a, 'brand, T, Token>
where
    T: 'a,
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

impl<'a, 'brand, T, Token> std::iter::FusedIterator for BrandedDoublyLinkedListIter<'a, 'brand, T, Token>
where
    T: 'a,
    Token: GhostBorrow<'brand>,
{}

impl<'a, 'brand, T, Token> ExactSizeIterator for BrandedDoublyLinkedListIter<'a, 'brand, T, Token>
where
    T: 'a,
    Token: GhostBorrow<'brand>,
{}

/// Mutable iterator for BrandedDoublyLinkedList.
struct BrandedDoublyLinkedListIterMut<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    view: PoolViewMut<'a, ListNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    remaining: usize,
    _phantom: PhantomData<(&'a mut Token, &'brand ())>,
}

impl<'a, 'brand, T, Token> Iterator for BrandedDoublyLinkedListIterMut<'a, 'brand, T, Token>
where
    T: 'a,
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

impl<'a, 'brand, T, Token> DoubleEndedIterator for BrandedDoublyLinkedListIterMut<'a, 'brand, T, Token>
where
    T: 'a,
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

impl<'a, 'brand, T, Token> std::iter::FusedIterator for BrandedDoublyLinkedListIterMut<'a, 'brand, T, Token>
where
    T: 'a,
    Token: GhostBorrowMut<'brand>,
{}

impl<'a, 'brand, T, Token> ExactSizeIterator for BrandedDoublyLinkedListIterMut<'a, 'brand, T, Token>
where
    T: 'a,
    Token: GhostBorrowMut<'brand>,
{}

/// A doubly linked list with token-gated access.
pub struct BrandedDoublyLinkedList<'brand, T> {
    /// Pool storage for nodes.
    pool: BrandedPool<'brand, ListNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    len: usize,
}

impl<'brand, T> BrandedDoublyLinkedList<'brand, T> {
    /// Creates a new empty doubly linked list.
    pub fn new() -> Self {
        Self {
            pool: BrandedPool::new(),
            head: None,
            tail: None,
            len: 0,
        }
    }

    /// Returns the number of elements in the list.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Clears the list, removing all elements.
    pub fn clear<Token>(&mut self, token: &mut Token)
    where
        Token: GhostBorrowMut<'brand>,
    {
        while self.pop_front(token).is_some() {}
    }

    /// Pushes an element to the front of the list.
    pub fn push_front<Token>(&mut self, token: &mut Token, value: T) -> usize
    where
        Token: GhostBorrowMut<'brand>,
    {
        let node = ListNode {
            prev: None,
            next: self.head,
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

    /// Pushes an element to the back of the list.
    pub fn push_back<Token>(&mut self, token: &mut Token, value: T) -> usize
    where
        Token: GhostBorrowMut<'brand>,
    {
        let node = ListNode {
            prev: self.tail,
            next: None,
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

    /// Pops an element from the front of the list.
    pub fn pop_front<Token>(&mut self, token: &mut Token) -> Option<T>
    where
        Token: GhostBorrowMut<'brand>,
    {
        let head_idx = self.head?;

        // We use take to extract value and free the slot
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

    /// Pops an element from the back of the list.
    pub fn pop_back<Token>(&mut self, token: &mut Token) -> Option<T>
    where
        Token: GhostBorrowMut<'brand>,
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
        Token: GhostBorrow<'brand>,
    {
        let head_idx = self.head?;
        let node = self.pool.get(token, head_idx)?;
        Some(&node.value)
    }

    /// Returns a reference to the back element.
    pub fn back<'a, Token>(&'a self, token: &'a Token) -> Option<&'a T>
    where
        Token: GhostBorrow<'brand>,
    {
        let tail_idx = self.tail?;
        let node = self.pool.get(token, tail_idx)?;
        Some(&node.value)
    }

    /// Returns a reference to the element at the given index.
    pub fn get<'a, Token>(&'a self, token: &'a Token, index: usize) -> Option<&'a T>
    where
        Token: GhostBorrow<'brand>,
    {
        let node = self.pool.get(token, index)?;
        Some(&node.value)
    }

    /// Returns a mutable reference to the element at the given index.
    pub fn get_mut<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
        index: usize,
    ) -> Option<&'a mut T>
    where
        Token: GhostBorrowMut<'brand>,
    {
        let node = self.pool.get_mut(token, index)?;
        Some(&mut node.value)
    }

    /// Iterates over the list elements.
    pub fn iter<'a, Token>(
        &'a self,
        token: &'a Token,
    ) -> impl Iterator<Item = &'a T> + DoubleEndedIterator + ExactSizeIterator + std::iter::FusedIterator + 'a + use<'a, 'brand, T, Token>
    where
        Token: GhostBorrow<'brand>,
    {
        BrandedDoublyLinkedListIter::<_, Token> {
            view: self.pool.view(token),
            head: self.head,
            tail: self.tail,
            remaining: self.len,
            _phantom: PhantomData,
        }
    }

    /// Iterates over the list elements (mutable).
    pub fn iter_mut<'a, Token>(
        &'a self,
        token: &'a mut Token,
    ) -> impl Iterator<Item = &'a mut T> + DoubleEndedIterator + ExactSizeIterator + std::iter::FusedIterator + 'a + use<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        BrandedDoublyLinkedListIterMut::<_, Token> {
            view: self.pool.view_mut(token),
            head: self.head,
            tail: self.tail,
            remaining: self.len,
            _phantom: PhantomData,
        }
    }

    /// Moves an existing element to the front of the list.
    pub fn move_to_front<Token>(&mut self, token: &mut Token, index: usize)
    where
        Token: GhostBorrowMut<'brand>,
    {
        if self.head == Some(index) {
            return;
        }

        // Detach
        let (prev, next) = {
            let node = self
                .pool
                .get(token, index)
                .expect("Index out of bounds or invalid");
            (node.prev, node.next)
        };

        if let Some(prev_idx) = prev {
            if let Some(node) = self.pool.get_mut(token, prev_idx) {
                node.next = next;
            }
        } else {
            // It was head, but we checked that
        }

        if let Some(next_idx) = next {
            if let Some(node) = self.pool.get_mut(token, next_idx) {
                node.prev = prev;
            }
        } else {
            self.tail = prev;
        }

        // Attach to front
        let old_head = self.head;
        if let Some(head_idx) = old_head {
            if let Some(node) = self.pool.get_mut(token, head_idx) {
                node.prev = Some(index);
            }
        }

        if let Some(node) = self.pool.get_mut(token, index) {
            node.prev = None;
            node.next = old_head;
        }

        self.head = Some(index);
    }

    /// Moves an existing element to the back of the list.
    pub fn move_to_back<Token>(&mut self, token: &mut Token, index: usize)
    where
        Token: GhostBorrowMut<'brand>,
    {
        if self.tail == Some(index) {
            return;
        }

        // Detach
        let (prev, next) = {
            let node = self
                .pool
                .get(token, index)
                .expect("Index out of bounds or invalid");
            (node.prev, node.next)
        };

        if let Some(prev_idx) = prev {
            if let Some(node) = self.pool.get_mut(token, prev_idx) {
                node.next = next;
            }
        } else {
            self.head = next;
        }

        if let Some(next_idx) = next {
            if let Some(node) = self.pool.get_mut(token, next_idx) {
                node.prev = prev;
            }
        } else {
            // It was tail, but we checked that
        }

        // Attach to back
        let old_tail = self.tail;
        if let Some(tail_idx) = old_tail {
            if let Some(node) = self.pool.get_mut(token, tail_idx) {
                node.next = Some(index);
            }
        }

        if let Some(node) = self.pool.get_mut(token, index) {
            node.prev = old_tail;
            node.next = None;
        }

        self.tail = Some(index);
    }

    /// Creates a cursor at the front of the list.
    pub fn cursor_front<'a>(&'a mut self) -> CursorMut<'a, 'brand, T> {
        let head = self.head;
        CursorMut {
            list: self,
            current: head,
            index: 0,
        }
    }

    /// Creates a cursor at the back of the list.
    pub fn cursor_back<'a>(&'a mut self) -> CursorMut<'a, 'brand, T> {
        let tail = self.tail;
        let len = self.len;
        CursorMut {
            list: self,
            current: tail,
            index: if len > 0 { len - 1 } else { 0 },
        }
    }
}

impl<'brand, T> Default for BrandedDoublyLinkedList<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

// Implement Drop
impl<'brand, T> Drop for BrandedDoublyLinkedList<'brand, T> {
    fn drop(&mut self) {
        let mut current = self.head;
        while let Some(idx) = current {
            if let Some(node) = self.pool.get_mut_exclusive(idx) {
                current = node.next;
                // Node destructor will run when pool is dropped, but values need dropping now?
                // PoolSlot::Occupied(T) owns T.
                // When pool drops, all slots drop. T drops.
                // If we don't clear the list, T will be dropped when Pool drops.
                // BUT if Pool is shared and we drop list, we want to free nodes in the pool?
                // If Pool is OWNED by List (it is: `pool: BrandedPool`), then dropping pool drops all T.
                // So we don't need manual drop logic if `PoolSlot` handles drop.
                // `PoolSlot` is an enum with `Occupied(T)`. Drop of enum drops T.
                // So this Drop impl is actually redundant if we own the pool!

                // Wait, if we used `BrandedPool` as a shared structure (passed in), we would need to free.
                // But `BrandedDoublyLinkedList` OWNS `BrandedPool`.
                // So we can just let `BrandedPool` drop.
            } else {
                break;
            }
        }
    }
}

impl<'brand, T> ZeroCopyOps<'brand, T> for BrandedDoublyLinkedList<'brand, T> {
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

/// A mutable cursor for the linked list.
pub struct CursorMut<'a, 'brand, T> {
    list: &'a mut BrandedDoublyLinkedList<'brand, T>,
    current: Option<usize>,
    index: usize,
}

impl<'a, 'brand, T> CursorMut<'a, 'brand, T> {
    /// Returns the current element index.
    pub fn index(&self) -> Option<usize> {
        self.current
    }

    /// Returns a reference to the current element.
    pub fn current<'b, Token>(&'b self, token: &'b Token) -> Option<&'b T>
    where
        Token: GhostBorrow<'brand>,
    {
        let idx = self.current?;
        let node = self.list.pool.get(token, idx)?;
        Some(&node.value)
    }

    /// Returns a mutable reference to the current element.
    pub fn current_mut<'b, Token>(&'b mut self, token: &'b mut Token) -> Option<&'b mut T>
    where
        Token: GhostBorrowMut<'brand>,
    {
        let idx = self.current?;
        let node = self.list.pool.get_mut(token, idx)?;
        Some(&mut node.value)
    }

    /// Moves the cursor to the next element.
    pub fn move_next<Token>(&mut self, token: &Token)
    where
        Token: GhostBorrow<'brand>,
    {
        if let Some(curr_idx) = self.current {
            if let Some(node) = self.list.pool.get(token, curr_idx) {
                self.current = node.next;
                if self.current.is_some() {
                    self.index += 1;
                }
            }
        } else {
            self.current = self.list.head;
            self.index = 0;
        }
    }

    /// Moves the cursor to the previous element.
    pub fn move_prev<Token>(&mut self, token: &Token)
    where
        Token: GhostBorrow<'brand>,
    {
        if let Some(curr_idx) = self.current {
            if let Some(node) = self.list.pool.get(token, curr_idx) {
                self.current = node.prev;
                if self.current.is_some() {
                    self.index -= 1;
                }
            }
        } else {
            self.current = self.list.tail;
            self.index = self.list.len().saturating_sub(1);
        }
    }

    /// Inserts a new element after the current element.
    pub fn insert_after<Token>(&mut self, token: &mut Token, value: T) -> usize
    where
        Token: GhostBorrowMut<'brand>,
    {
        if let Some(curr_idx) = self.current {
            // Read next_idx
            let next_idx = self.list.pool.get(token, curr_idx).unwrap().next;

            let node = ListNode {
                prev: Some(curr_idx),
                next: next_idx,
                value,
            };
            let new_idx = self.list.pool.alloc(token, node);

            // Update current's next
            if let Some(node) = self.list.pool.get_mut(token, curr_idx) {
                node.next = Some(new_idx);
            }

            // Update next's prev or tail
            if let Some(next) = next_idx {
                if let Some(node) = self.list.pool.get_mut(token, next) {
                    node.prev = Some(new_idx);
                }
            } else {
                self.list.tail = Some(new_idx);
            }

            self.list.len += 1;
            new_idx
        } else if self.list.is_empty() {
            let new_idx = self.list.push_back(token, value);
            self.current = self.list.head;
            new_idx
        } else {
            panic!("Cannot insert after None cursor on non-empty list");
        }
    }

    /// Inserts a new element before the current element.
    pub fn insert_before<Token>(&mut self, token: &mut Token, value: T) -> usize
    where
        Token: GhostBorrowMut<'brand>,
    {
        if let Some(curr_idx) = self.current {
            // Read prev_idx
            let prev_idx = self.list.pool.get(token, curr_idx).unwrap().prev;

            let node = ListNode {
                prev: prev_idx,
                next: Some(curr_idx),
                value,
            };
            let new_idx = self.list.pool.alloc(token, node);

            // Update current's prev
            if let Some(node) = self.list.pool.get_mut(token, curr_idx) {
                node.prev = Some(new_idx);
            }

            // Update prev's next or head
            if let Some(prev) = prev_idx {
                if let Some(node) = self.list.pool.get_mut(token, prev) {
                    node.next = Some(new_idx);
                }
            } else {
                self.list.head = Some(new_idx);
            }

            self.list.len += 1;
            self.index += 1;
            new_idx
        } else if self.list.is_empty() {
            let new_idx = self.list.push_front(token, value);
            self.current = self.list.head;
            new_idx
        } else {
            panic!("Cannot insert before None cursor on non-empty list");
        }
    }

    /// Removes the current element. The cursor moves to the next element.
    pub fn remove_current<Token>(&mut self, token: &mut Token) -> Option<T>
    where
        Token: GhostBorrowMut<'brand>,
    {
        let curr_idx = self.current?;

        let node = unsafe { self.list.pool.take(token, curr_idx) };
        let prev_idx = node.prev;
        let next_idx = node.next;

        // Update prev node or head
        if let Some(prev) = prev_idx {
            if let Some(node) = self.list.pool.get_mut(token, prev) {
                node.next = next_idx;
            }
        } else {
            self.list.head = next_idx;
        }

        // Update next node or tail
        if let Some(next) = next_idx {
            if let Some(node) = self.list.pool.get_mut(token, next) {
                node.prev = prev_idx;
            }
        } else {
            self.list.tail = prev_idx;
        }

        self.list.len -= 1;
        self.current = next_idx;

        Some(node.value)
    }
}

impl<'brand, T: fmt::Debug> fmt::Debug for BrandedDoublyLinkedList<'brand, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrandedDoublyLinkedList")
            .field("len", &self.len)
            .field("head", &self.head)
            .field("tail", &self.tail)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_push_pop_basic() {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();

            let idx1 = list.push_back(&mut token, 1);
            let idx2 = list.push_back(&mut token, 2);
            let idx0 = list.push_front(&mut token, 0);

            assert_eq!(list.len(), 3);
            assert_eq!(idx1, 0); // First alloc
            assert_eq!(idx2, 1); // Second alloc
            assert_eq!(idx0, 2); // Third alloc

            assert_eq!(list.pop_front(&mut token), Some(0));
            assert_eq!(list.pop_back(&mut token), Some(2));
            assert_eq!(list.pop_back(&mut token), Some(1));
            assert_eq!(list.pop_back(&mut token), None);
            assert!(list.is_empty());
        });
    }

    #[test]
    fn test_move_to_front() {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();
            let idx1 = list.push_back(&mut token, 1); // Head
            let idx2 = list.push_back(&mut token, 2);
            let idx3 = list.push_back(&mut token, 3); // Tail

            // List: 1, 2, 3
            list.move_to_front(&mut token, idx2);
            // List: 2, 1, 3
            assert_eq!(list.front(&token), Some(&2));
            assert_eq!(list.back(&token), Some(&3));

            list.move_to_front(&mut token, idx3);
            // List: 3, 2, 1
            assert_eq!(list.front(&token), Some(&3));
            assert_eq!(list.back(&token), Some(&1));

            list.move_to_front(&mut token, idx3); // Already front
            assert_eq!(list.front(&token), Some(&3));
        });
    }

    #[test]
    fn test_cursor_navigation() {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();
            list.push_back(&mut token, 1);
            list.push_back(&mut token, 2);
            list.push_back(&mut token, 3);

            let mut cursor = list.cursor_front();
            assert_eq!(cursor.current(&token), Some(&1));

            cursor.move_next(&token);
            assert_eq!(cursor.current(&token), Some(&2));

            cursor.move_next(&token);
            assert_eq!(cursor.current(&token), Some(&3));

            cursor.move_next(&token);
            assert_eq!(cursor.current(&token), None);

            cursor.move_prev(&token);
            assert_eq!(cursor.current(&token), Some(&3));
        });
    }

    #[test]
    fn test_cursor_mutation() {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();
            list.push_back(&mut token, 1);
            list.push_back(&mut token, 3);

            let mut cursor = list.cursor_front();
            cursor.move_next(&token); // At 3

            cursor.insert_before(&mut token, 2);
            // List should be 1, 2, 3
            // Cursor is still at 3
            assert_eq!(cursor.current(&token), Some(&3));

            cursor.move_prev(&token); // At 2
            assert_eq!(cursor.current(&token), Some(&2));

            cursor.move_prev(&token); // At 1
            assert_eq!(cursor.current(&token), Some(&1));

            cursor.remove_current(&mut token); // Remove 1
                                               // List should be 2, 3
                                               // Cursor moves to next: 2
            assert_eq!(cursor.current(&token), Some(&2));
            assert_eq!(list.len(), 2);

            assert_eq!(list.pop_front(&mut token), Some(2));
            assert_eq!(list.pop_front(&mut token), Some(3));
        });
    }

    #[test]
    fn test_iter_and_zero_copy() {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();
            list.push_back(&mut token, 1);
            list.push_back(&mut token, 2);
            list.push_back(&mut token, 3);

            // Test iter
            let collected: Vec<i32> = list.iter(&token).copied().collect();
            assert_eq!(collected, vec![1, 2, 3]);

            // Test zero copy ops
            assert_eq!(list.find_ref(&token, |&x| x == 2), Some(&2));
            assert!(list.any_ref(&token, |&x| x == 3));
            assert!(list.all_ref(&token, |&x| x > 0));
        });
    }
}
