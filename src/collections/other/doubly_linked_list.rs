//! `BrandedDoublyLinkedList` — a token-gated doubly linked list.
//!
//! This implementation uses a `BrandedVec` as the backing storage (arena) for nodes,
//! allowing safe index-based pointers with the `GhostCell` pattern.
//! It supports O(1) insertion and removal at arbitrary positions via Cursors.

use crate::GhostToken;
use crate::collections::vec::BrandedVec;
use crate::collections::ZeroCopyOps;
use core::fmt;

/// Zero-cost iterator for BrandedDoublyLinkedList.
pub struct BrandedDoublyLinkedListIter<'a, 'brand, T> {
    list: &'a BrandedDoublyLinkedList<'brand, T>,
    current: Option<usize>,
    token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, T> Iterator for BrandedDoublyLinkedListIter<'a, 'brand, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.current?;
        // SAFETY: Internal indices are guaranteed to be valid.
        match unsafe { self.list.storage.get_unchecked(self.token, idx) } {
            Slot::Occupied(node) => {
                self.current = node.next;
                Some(&node.value)
            }
            _ => None, // Should not happen for valid list
        }
    }
}

/// A node in the doubly linked list.
#[derive(Debug, Clone)]
struct Node<T> {
    value: T,
    prev: Option<usize>,
    next: Option<usize>,
}

/// A slot in the backing storage, either occupied by a node or free.
#[derive(Debug, Clone)]
enum Slot<T> {
    Occupied(Node<T>),
    Free(Option<usize>), // Next free slot index
}

/// A doubly linked list with token-gated access.
pub struct BrandedDoublyLinkedList<'brand, T> {
    storage: BrandedVec<'brand, Slot<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    free_head: Option<usize>,
    len: usize,
}

impl<'brand, T> BrandedDoublyLinkedList<'brand, T> {
    /// Creates a new empty doubly linked list.
    pub fn new() -> Self {
        Self {
            storage: BrandedVec::new(),
            head: None,
            tail: None,
            free_head: None,
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
    pub fn clear(&mut self, token: &mut GhostToken<'brand>) {
        // We can just reset everything, effectively dropping all elements
        // when the storage vector is cleared.
        // However, BrandedVec doesn't expose clear() directly on inner.
        // But we can rebuild it or iterate and pop.
        while self.pop_front(token).is_some() {}
    }

    /// Allocates a new node slot, potentially reusing a free one.
    fn alloc(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
        if let Some(free_idx) = self.free_head {
            // Reuse free slot
            let slot = self.storage.borrow_mut(token, free_idx);
            let next_free = match slot {
                Slot::Free(next) => *next,
                _ => panic!("Corrupted free list"),
            };
            *slot = Slot::Occupied(Node {
                value,
                prev: None,
                next: None,
            });
            self.free_head = next_free;
            free_idx
        } else {
            // Push new slot
            let idx = self.storage.len();
            self.storage.push(Slot::Occupied(Node {
                value,
                prev: None,
                next: None,
            }));
            idx
        }
    }

    /// Frees a node slot.
    fn free(&mut self, token: &mut GhostToken<'brand>, idx: usize) {
        let slot = self.storage.borrow_mut(token, idx);
        // We don't need the value anymore, it's already moved out or dropped.
        // But we need to be careful not to drop uninitialized memory if we used ptr::read.
        // Here we assume the slot currently contains a valid Occupied node that we are overwriting.
        *slot = Slot::Free(self.free_head);
        self.free_head = Some(idx);
    }

    /// Pushes an element to the front of the list.
    pub fn push_front(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
        let new_idx = self.alloc(token, value);
        let old_head = self.head;

        if let Some(head_idx) = old_head {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, head_idx) {
                node.prev = Some(new_idx);
            }
        } else {
            self.tail = Some(new_idx);
        }

        if let Slot::Occupied(node) = self.storage.borrow_mut(token, new_idx) {
            node.next = old_head;
        }

        self.head = Some(new_idx);
        self.len += 1;
        new_idx
    }

    /// Pushes an element to the back of the list.
    pub fn push_back(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
        let new_idx = self.alloc(token, value);
        let old_tail = self.tail;

        if let Some(tail_idx) = old_tail {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, tail_idx) {
                node.next = Some(new_idx);
            }
        } else {
            self.head = Some(new_idx);
        }

        if let Slot::Occupied(node) = self.storage.borrow_mut(token, new_idx) {
            node.prev = old_tail;
        }

        self.tail = Some(new_idx);
        self.len += 1;
        new_idx
    }

    /// Pops an element from the front of the list.
    pub fn pop_front(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        let head_idx = self.head?;
        // SAFETY: head index is managed internally and valid.
        let head_slot = unsafe { self.storage.get_unchecked_mut(token, head_idx) };

        // Extract value and replace with Free marker placeholder
        // (We will fix the Free marker's next pointer in free())
        let node = match std::mem::replace(head_slot, Slot::Free(None)) {
             Slot::Occupied(node) => node,
             _ => panic!("Corrupted list: head points to free slot"),
        };

        let next_idx = node.next;

        // Fix up the free list
        *head_slot = Slot::Free(self.free_head);
        self.free_head = Some(head_idx);

        if let Some(next) = next_idx {
             if let Slot::Occupied(node) = self.storage.borrow_mut(token, next) {
                 node.prev = None;
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
    pub fn pop_back(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        let tail_idx = self.tail?;
        // SAFETY: tail index is managed internally and valid.
        let tail_slot = unsafe { self.storage.get_unchecked_mut(token, tail_idx) };

        let node = match std::mem::replace(tail_slot, Slot::Free(None)) {
             Slot::Occupied(node) => node,
             _ => panic!("Corrupted list: tail points to free slot"),
        };

        let prev_idx = node.prev;

        *tail_slot = Slot::Free(self.free_head);
        self.free_head = Some(tail_idx);

        if let Some(prev) = prev_idx {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, prev) {
                node.next = None;
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
    pub fn front<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        let head_idx = self.head?;
        // SAFETY: head index is managed internally and valid.
        match unsafe { self.storage.get_unchecked(token, head_idx) } {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a reference to the back element.
    pub fn back<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        let tail_idx = self.tail?;
        // SAFETY: tail index is managed internally and valid.
        match unsafe { self.storage.get_unchecked(token, tail_idx) } {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a reference to the element at the given index.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> Option<&'a T> {
        match self.storage.borrow(token, index) {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a mutable reference to the element at the given index.
    pub fn get_mut<'a>(&'a mut self, token: &'a mut GhostToken<'brand>, index: usize) -> Option<&'a mut T> {
        match self.storage.borrow_mut(token, index) {
            Slot::Occupied(node) => Some(&mut node.value),
            _ => None,
        }
    }

    /// Iterates over the list elements.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> BrandedDoublyLinkedListIter<'a, 'brand, T> {
        BrandedDoublyLinkedListIter {
            list: self,
            current: self.head,
            token,
        }
    }

    /// Moves the node at `index` to the front of the list.
    ///
    /// # Panics
    /// Panics if `index` is not a valid node index.
    pub fn move_to_front(&mut self, token: &mut GhostToken<'brand>, index: usize) {
        if self.head == Some(index) {
            return;
        }

        // Verify index is valid and get neighbors
        let (prev_idx, next_idx) = if let Slot::Occupied(node) = self.storage.borrow(token, index) {
            (node.prev, node.next)
        } else {
             panic!("Invalid index");
        };

        // Detach from current position
        if let Some(prev) = prev_idx {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, prev) {
                node.next = next_idx;
            }
        } else {
            // If prev is None, we are at head. But we checked head == Some(index) above.
            // This case should be unreachable if invariants hold, unless the list is corrupted.
            // Or maybe head is somehow not index but prev is None? That implies head IS index.
            // So we can assume unreachable! or just ignore.
        }

        if let Some(next) = next_idx {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, next) {
                node.prev = prev_idx;
            }
        } else {
             self.tail = prev_idx;
        }

        // Attach to front
        let old_head = self.head;
        if let Some(head_idx) = old_head {
             if let Slot::Occupied(node) = self.storage.borrow_mut(token, head_idx) {
                 node.prev = Some(index);
             }
        }

        if let Slot::Occupied(node) = self.storage.borrow_mut(token, index) {
            node.prev = None;
            node.next = old_head;
        }

        self.head = Some(index);
        // If list was empty before (it wasn't, we had `index`), or had 1 element...
        if self.tail.is_none() {
            // Should not happen if we are moving an existing element
             self.tail = Some(index);
        } else if self.tail == Some(index) && next_idx.is_none() {
             // We were tail, and now we are head.
             // If we were the ONLY element, head==tail==index, caught by early return.
             // If we were tail of >1 list, we detached. new tail is prev_idx.
             // We are now head.
        }
    }

    /// Moves the node at `index` to the back of the list.
    ///
    /// # Panics
    /// Panics if `index` is not a valid node index.
    pub fn move_to_back(&mut self, token: &mut GhostToken<'brand>, index: usize) {
        if self.tail == Some(index) {
            return;
        }

        // Verify index is valid and get neighbors
        let (prev_idx, next_idx) = if let Slot::Occupied(node) = self.storage.borrow(token, index) {
            (node.prev, node.next)
        } else {
             panic!("Invalid index");
        };

        // Detach from current position
        if let Some(prev) = prev_idx {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, prev) {
                node.next = next_idx;
            }
        } else {
            self.head = next_idx;
        }

        if let Some(next) = next_idx {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, next) {
                node.prev = prev_idx;
            }
        }

        // Attach to back
        let old_tail = self.tail;
        if let Some(tail_idx) = old_tail {
             if let Slot::Occupied(node) = self.storage.borrow_mut(token, tail_idx) {
                 node.next = Some(index);
             }
        }

        if let Slot::Occupied(node) = self.storage.borrow_mut(token, index) {
            node.next = None;
            node.prev = old_tail;
        }

        self.tail = Some(index);
        if self.head.is_none() {
            self.head = Some(index);
        }
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

impl<'brand, T> FromIterator<T> for BrandedDoublyLinkedList<'brand, T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let items: Vec<T> = iter.into_iter().collect();
        let len = items.len();

        if len == 0 {
            return Self::new();
        }

        let mut storage = BrandedVec::with_capacity(len);

        for (i, item) in items.into_iter().enumerate() {
            let prev = if i == 0 { None } else { Some(i - 1) };
            let next = if i == len - 1 { None } else { Some(i + 1) };

            let node = Node {
                value: item,
                prev,
                next,
            };

            storage.push(Slot::Occupied(node));
        }

        Self {
            storage,
            head: Some(0),
            tail: Some(len - 1),
            free_head: None,
            len,
        }
    }
}

/// Consuming iterator for BrandedDoublyLinkedList.
pub struct IntoIter<T> {
    slots: Vec<Option<Slot<T>>>,
    current: Option<usize>,
    len: usize,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.current?;
        if let Some(slot) = self.slots.get_mut(idx).and_then(|s| s.take()) {
            match slot {
                Slot::Occupied(node) => {
                    self.current = node.next;
                    self.len -= 1;
                    Some(node.value)
                }
                Slot::Free(_) => {
                    // Should not happen if following valid links
                    None
                }
            }
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<T> ExactSizeIterator for IntoIter<T> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<'brand, T> IntoIterator for BrandedDoublyLinkedList<'brand, T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        let len = self.len;
        let head = self.head;
        // BrandedVec::into_iter() returns Iterator<Item = Slot<T>>
        let slots: Vec<Option<Slot<T>>> = self.storage.into_iter().map(Some).collect();

        IntoIter {
            slots,
            current: head,
            len,
        }
    }
}

impl<'brand, T> ZeroCopyOps<'brand, T> for BrandedDoublyLinkedList<'brand, T> {
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).find(|&item| f(item))
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).any(|item| f(item))
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
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
    pub fn current<'b>(&'b self, token: &'b GhostToken<'brand>) -> Option<&'b T> {
        let idx = self.current?;
        // SAFETY: cursor index is managed internally and valid.
        match unsafe { self.list.storage.get_unchecked(token, idx) } {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a mutable reference to the current element.
    pub fn current_mut<'b>(&'b mut self, token: &'b mut GhostToken<'brand>) -> Option<&'b mut T> {
        let idx = self.current?;
        // SAFETY: cursor index is managed internally and valid.
        match unsafe { self.list.storage.get_unchecked_mut(token, idx) } {
            Slot::Occupied(node) => Some(&mut node.value),
            _ => None,
        }
    }

    /// Moves the cursor to the next element.
    pub fn move_next(&mut self, token: &GhostToken<'brand>) {
        if let Some(curr_idx) = self.current {
            // SAFETY: cursor index is valid.
            if let Slot::Occupied(node) = unsafe { self.list.storage.get_unchecked(token, curr_idx) } {
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
    pub fn move_prev(&mut self, token: &GhostToken<'brand>) {
        if let Some(curr_idx) = self.current {
            // SAFETY: cursor index is valid.
            if let Slot::Occupied(node) = unsafe { self.list.storage.get_unchecked(token, curr_idx) } {
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
    pub fn insert_after(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
        if let Some(curr_idx) = self.current {
            let new_idx = self.list.alloc(token, value);

            // Get current's next
            let next_idx = if let Slot::Occupied(node) = self.list.storage.borrow(token, curr_idx) {
                node.next
            } else {
                panic!("Corrupted list");
            };

            // Link new node
            if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, new_idx) {
                node.prev = Some(curr_idx);
                node.next = next_idx;
            }

            // Update current's next
            if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, curr_idx) {
                node.next = Some(new_idx);
            }

            // Update next's prev or tail
            if let Some(next) = next_idx {
                if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, next) {
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
             // If cursor is detached but list is not empty (e.g. at end), push back?
             // Usually insert_after on None cursor implies push_front/back depending on convention.
             // Here if current is None, we assume it's "before head" or "after tail"?
             // The implementation says:
             // if list is empty, push_back.
             // If not empty, what?
             // It seems original implementation handled list empty.
             // We can just return push_back result.
             // But wait, if cursor is None, we can't insert "after" it.
             // I'll stick to original logic: if empty, push back.
             // If not empty and current is None, it probably does nothing or panics?
             // Original: "else if self.list.is_empty() { ... }"
             // So if not empty and None, it does nothing?
             // I should probably return Option<usize> or just usize (and panic/return 0 if nothing happened).
             // But alloc returns usize.
             // If nothing happens, I can't return a valid index.
             // I'll change logic to return Option<usize> for inserts on cursor?
             // But the request was to return usize.
             // Let's assume valid state.
             panic!("Cannot insert after None cursor on non-empty list");
        }
    }

    /// Inserts a new element before the current element.
    pub fn insert_before(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
         if let Some(curr_idx) = self.current {
            let new_idx = self.list.alloc(token, value);

            // Get current's prev
            let prev_idx = if let Slot::Occupied(node) = self.list.storage.borrow(token, curr_idx) {
                node.prev
            } else {
                panic!("Corrupted list");
            };

            // Link new node
            if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, new_idx) {
                node.prev = prev_idx;
                node.next = Some(curr_idx);
            }

            // Update current's prev
            if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, curr_idx) {
                node.prev = Some(new_idx);
            }

            // Update prev's next or head
            if let Some(prev) = prev_idx {
                if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, prev) {
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
    pub fn remove_current(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        let curr_idx = self.current?;

        let (prev_idx, next_idx) = if let Slot::Occupied(node) = self.list.storage.borrow(token, curr_idx) {
            (node.prev, node.next)
        } else {
            panic!("Corrupted list");
        };

        // Update prev node or head
        if let Some(prev) = prev_idx {
            if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, prev) {
                node.next = next_idx;
            }
        } else {
            self.list.head = next_idx;
        }

        // Update next node or tail
        if let Some(next) = next_idx {
            if let Slot::Occupied(node) = self.list.storage.borrow_mut(token, next) {
                node.prev = prev_idx;
            }
        } else {
            self.list.tail = prev_idx;
        }

        // Extract value and free slot
        let slot = self.list.storage.borrow_mut(token, curr_idx);
        let node = match std::mem::replace(slot, Slot::Free(None)) {
             Slot::Occupied(node) => node,
             _ => panic!("Corrupted list"),
        };

        *slot = Slot::Free(self.list.free_head);
        self.list.free_head = Some(curr_idx);

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
