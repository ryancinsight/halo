//! `BrandedDoublyLinkedList` — a token-gated doubly linked list.
//!
//! This implementation uses a `BrandedVec` as the backing storage (arena) for nodes,
//! allowing safe index-based pointers with the `GhostCell` pattern.
//! It supports O(1) insertion and removal at arbitrary positions via Cursors.

use crate::GhostToken;
use crate::collections::vec::BrandedVec;
use core::fmt;

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
    ///
    /// Returns the index of the newly created node.
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
    ///
    /// Returns the index of the newly created node.
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

    /// Returns a reference to the element at the given index.
    ///
    /// Returns `None` if the index is invalid or the slot is free.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        match self.storage.get(token, idx) {
            Some(Slot::Occupied(node)) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a mutable reference to the element at the given index.
    ///
    /// Returns `None` if the index is invalid or the slot is free.
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        match self.storage.get_mut(token, idx) {
            Some(Slot::Occupied(node)) => Some(&mut node.value),
            _ => None,
        }
    }

    /// Moves the node at `idx` to the front of the list.
    ///
    /// # Panics
    /// Panics if `idx` does not point to a valid occupied node.
    pub fn move_to_front(&mut self, token: &mut GhostToken<'brand>, idx: usize) {
        if self.head == Some(idx) {
            return;
        }

        // Detach from current position
        let (prev, next) = {
            let node = match self.storage.borrow_mut(token, idx) {
                Slot::Occupied(node) => node,
                _ => panic!("Node not found at index {}", idx),
            };
            (node.prev, node.next)
        };

        // Update neighbors
        if let Some(p) = prev {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, p) {
                node.next = next;
            }
        } else {
            // idx was head (handled by early return, but for completeness)
            self.head = next;
        }

        if let Some(n) = next {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, n) {
                node.prev = prev;
            }
        } else {
            // idx was tail
            self.tail = prev;
        }

        // Attach to front
        let old_head = self.head;
        if let Some(h) = old_head {
            if let Slot::Occupied(node) = self.storage.borrow_mut(token, h) {
                node.prev = Some(idx);
            }
        } else {
            // List was empty (impossible if idx was in it) or became empty
            self.tail = Some(idx);
        }

        if let Slot::Occupied(node) = self.storage.borrow_mut(token, idx) {
            node.prev = None;
            node.next = old_head;
        }

        self.head = Some(idx);
    }

    /// Pops an element from the front of the list.
    pub fn pop_front(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        let head_idx = self.head?;
        let head_slot = self.storage.borrow_mut(token, head_idx);

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
        let tail_slot = self.storage.borrow_mut(token, tail_idx);

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
        match self.storage.borrow(token, head_idx) {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a reference to the back element.
    pub fn back<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        let tail_idx = self.tail?;
        match self.storage.borrow(token, tail_idx) {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
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
        match self.list.storage.borrow(token, idx) {
            Slot::Occupied(node) => Some(&node.value),
            _ => None,
        }
    }

    /// Returns a mutable reference to the current element.
    pub fn current_mut<'b>(&'b mut self, token: &'b mut GhostToken<'brand>) -> Option<&'b mut T> {
        let idx = self.current?;
        match self.list.storage.borrow_mut(token, idx) {
            Slot::Occupied(node) => Some(&mut node.value),
            _ => None,
        }
    }

    /// Moves the cursor to the next element.
    pub fn move_next(&mut self, token: &GhostToken<'brand>) {
        if let Some(curr_idx) = self.current {
            if let Slot::Occupied(node) = self.list.storage.borrow(token, curr_idx) {
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
            if let Slot::Occupied(node) = self.list.storage.borrow(token, curr_idx) {
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
    pub fn insert_after(&mut self, token: &mut GhostToken<'brand>, value: T) {
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
        } else if self.list.is_empty() {
            self.list.push_back(token, value);
            self.current = self.list.head;
        }
    }

    /// Inserts a new element before the current element.
    pub fn insert_before(&mut self, token: &mut GhostToken<'brand>, value: T) {
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
        } else if self.list.is_empty() {
            self.list.push_front(token, value);
            self.current = self.list.head;
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

            list.push_back(&mut token, 1);
            list.push_back(&mut token, 2);
            list.push_front(&mut token, 0);

            assert_eq!(list.len(), 3);

            assert_eq!(list.pop_front(&mut token), Some(0));
            assert_eq!(list.pop_back(&mut token), Some(2));
            assert_eq!(list.pop_back(&mut token), Some(1));
            assert_eq!(list.pop_back(&mut token), None);
            assert!(list.is_empty());
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
}
