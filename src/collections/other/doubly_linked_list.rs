//! `BrandedDoublyLinkedList` — a token-gated doubly linked list.
//!
//! This implementation uses a `BrandedVec` as the backing storage (arena) for nodes,
//! allowing safe index-based pointers with the `GhostCell` pattern.
//! It supports O(1) insertion and removal at arbitrary positions via Cursors.
//!
//! # Optimization
//! This implementation uses a Structure-of-Arrays (SoA) layout:
//! - `links`: Stores structure (prev/next pointers). Optimizes cache usage for structural operations.
//! - `values`: Stores element data. Only accessed when needed.

use crate::GhostToken;
use crate::collections::vec::BrandedVec;
use crate::collections::ZeroCopyOps;
use core::fmt;
use core::mem::MaybeUninit;

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
        // SAFETY: Internal indices are guaranteed to be valid and synchronized.
        match unsafe { self.list.links.get_unchecked(self.token, idx) } {
            LinkSlot::Occupied { next, .. } => {
                self.current = *next;
                // SAFETY: If link slot is occupied, value slot is initialized.
                unsafe {
                    let cell = self.list.values.get_unchecked(self.token, idx);
                    Some(cell.assume_init_ref())
                }
            }
            LinkSlot::Free(_) => None, // Should not happen for valid list traversal
        }
    }
}

/// A slot in the links vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkSlot {
    Occupied { prev: Option<usize>, next: Option<usize> },
    Free(Option<usize>), // Next free slot index
}

/// A doubly linked list with token-gated access.
pub struct BrandedDoublyLinkedList<'brand, T> {
    /// Structure storage: small footprint for cache efficiency during traversal/reordering.
    links: BrandedVec<'brand, LinkSlot>,
    /// Value storage: only accessed when reading/writing values.
    /// Uses MaybeUninit because free slots contain uninitialized data.
    values: BrandedVec<'brand, MaybeUninit<T>>,

    head: Option<usize>,
    tail: Option<usize>,
    free_head: Option<usize>,
    len: usize,
}

impl<'brand, T> BrandedDoublyLinkedList<'brand, T> {
    /// Creates a new empty doubly linked list.
    pub fn new() -> Self {
        Self {
            links: BrandedVec::new(),
            values: BrandedVec::new(),
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
        while self.pop_front(token).is_some() {}
    }

    /// Allocates a new node slot, potentially reusing a free one.
    fn alloc(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
        if let Some(free_idx) = self.free_head {
            // Reuse free slot
            let slot = self.links.borrow_mut(token, free_idx);
            let next_free = match slot {
                LinkSlot::Free(next) => *next,
                _ => panic!("Corrupted free list"),
            };
            *slot = LinkSlot::Occupied { prev: None, next: None };

            // Initialize value
            // SAFETY: Slot was free, so value was uninitialized. We write a valid value.
            let val_slot = self.values.borrow_mut(token, free_idx);
            *val_slot = MaybeUninit::new(value);

            self.free_head = next_free;
            free_idx
        } else {
            // Push new slot
            let idx = self.links.len();
            self.links.push(LinkSlot::Occupied { prev: None, next: None });
            self.values.push(MaybeUninit::new(value));
            idx
        }
    }

    /// Frees a node slot.
    ///
    /// This puts the slot onto the free list and drops the value.
    /// Caller must update links pointing to this node before freeing.
    fn free(&mut self, token: &mut GhostToken<'brand>, idx: usize) {
        let slot = self.links.borrow_mut(token, idx);
        *slot = LinkSlot::Free(self.free_head);

        // Drop the value
        let val_slot = self.values.borrow_mut(token, idx);
        unsafe { val_slot.assume_init_drop() };

        self.free_head = Some(idx);
    }

    /// Pushes an element to the front of the list.
    pub fn push_front(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
        let new_idx = self.alloc(token, value);
        let old_head = self.head;

        if let Some(head_idx) = old_head {
            if let LinkSlot::Occupied { prev, .. } = self.links.borrow_mut(token, head_idx) {
                *prev = Some(new_idx);
            }
        } else {
            self.tail = Some(new_idx);
        }

        if let LinkSlot::Occupied { next, .. } = self.links.borrow_mut(token, new_idx) {
            *next = old_head;
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
            if let LinkSlot::Occupied { next, .. } = self.links.borrow_mut(token, tail_idx) {
                *next = Some(new_idx);
            }
        } else {
            self.head = Some(new_idx);
        }

        if let LinkSlot::Occupied { prev, .. } = self.links.borrow_mut(token, new_idx) {
            *prev = old_tail;
        }

        self.tail = Some(new_idx);
        self.len += 1;
        new_idx
    }

    /// Pops an element from the front of the list.
    pub fn pop_front(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        let head_idx = self.head?;

        // Read links
        let (next_idx, _) = match self.links.borrow(token, head_idx) {
            LinkSlot::Occupied { next, prev } => (*next, *prev),
            _ => panic!("Corrupted list: head points to free slot"),
        };

        // Extract value before freeing
        // SAFETY: We checked it's Occupied.
        let value = unsafe {
            let val_slot = self.values.borrow(token, head_idx);
            val_slot.assume_init_read()
        };

        // We manually handle "freeing" logic here to avoid double drop
        // Actually alloc/free manage MaybeUninit, so free() just drops it.
        // But we want to return the value.
        // So we should NOT call free(), but implement the link update part of free manually
        // and put it in free list, WITHOUT dropping value (since we moved it).

        let slot = self.links.borrow_mut(token, head_idx);
        *slot = LinkSlot::Free(self.free_head);
        self.free_head = Some(head_idx);

        if let Some(next) = next_idx {
             if let LinkSlot::Occupied { prev, .. } = self.links.borrow_mut(token, next) {
                 *prev = None;
             }
             self.head = Some(next);
        } else {
             self.head = None;
             self.tail = None;
        }

        self.len -= 1;
        Some(value)
    }

    /// Pops an element from the back of the list.
    pub fn pop_back(&mut self, token: &mut GhostToken<'brand>) -> Option<T> {
        let tail_idx = self.tail?;

        let (prev_idx, _) = match self.links.borrow(token, tail_idx) {
            LinkSlot::Occupied { prev, next } => (*prev, *next),
            _ => panic!("Corrupted list: tail points to free slot"),
        };

        // Extract value
        let value = unsafe {
            let val_slot = self.values.borrow(token, tail_idx);
            val_slot.assume_init_read()
        };

        // Update free list manually to avoid drop
        let slot = self.links.borrow_mut(token, tail_idx);
        *slot = LinkSlot::Free(self.free_head);
        self.free_head = Some(tail_idx);

        if let Some(prev) = prev_idx {
            if let LinkSlot::Occupied { next, .. } = self.links.borrow_mut(token, prev) {
                *next = None;
            }
            self.tail = Some(prev);
        } else {
            self.head = None;
            self.tail = None;
        }

        self.len -= 1;
        Some(value)
    }

    /// Returns a reference to the front element.
    pub fn front<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        let head_idx = self.head?;
        match self.links.borrow(token, head_idx) {
            LinkSlot::Occupied { .. } => {
                unsafe { Some(self.values.borrow(token, head_idx).assume_init_ref()) }
            }
            _ => None,
        }
    }

    /// Returns a reference to the back element.
    pub fn back<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        let tail_idx = self.tail?;
        match self.links.borrow(token, tail_idx) {
            LinkSlot::Occupied { .. } => {
                unsafe { Some(self.values.borrow(token, tail_idx).assume_init_ref()) }
            }
            _ => None,
        }
    }

    /// Returns a reference to the element at the given index.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> Option<&'a T> {
        // We must check if slot is occupied
        match self.links.get(token, index) {
            Some(LinkSlot::Occupied { .. }) => {
                unsafe { Some(self.values.get_unchecked(token, index).assume_init_ref()) }
            }
            _ => None,
        }
    }

    /// Returns a mutable reference to the element at the given index.
    pub fn get_mut<'a>(&'a mut self, token: &'a mut GhostToken<'brand>, index: usize) -> Option<&'a mut T> {
        match self.links.get(token, index) {
            Some(LinkSlot::Occupied { .. }) => {
                unsafe { Some(self.values.get_unchecked_mut(token, index).assume_init_mut()) }
            }
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
    /// This operation is optimized by only touching the `links` vector,
    /// avoiding cache pollution from loading `values`.
    ///
    /// # Panics
    /// Panics if `index` is not a valid node index.
    pub fn move_to_front(&mut self, token: &mut GhostToken<'brand>, index: usize) {
        if self.head == Some(index) {
            return;
        }

        // Verify index is valid and get neighbors
        let (prev_idx, next_idx) = if let LinkSlot::Occupied { prev, next } = self.links.borrow(token, index) {
            (*prev, *next)
        } else {
             panic!("Invalid index");
        };

        // Detach from current position
        if let Some(prev) = prev_idx {
            if let LinkSlot::Occupied { next, .. } = self.links.borrow_mut(token, prev) {
                *next = next_idx;
            }
        }

        if let Some(next) = next_idx {
            if let LinkSlot::Occupied { prev, .. } = self.links.borrow_mut(token, next) {
                *prev = prev_idx;
            }
        } else {
             self.tail = prev_idx;
        }

        // Attach to front
        let old_head = self.head;
        if let Some(head_idx) = old_head {
             if let LinkSlot::Occupied { prev, .. } = self.links.borrow_mut(token, head_idx) {
                 *prev = Some(index);
             }
        }

        if let LinkSlot::Occupied { prev, next } = self.links.borrow_mut(token, index) {
            *prev = None;
            *next = old_head;
        }

        self.head = Some(index);

        if self.tail.is_none() {
             self.tail = Some(index);
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

        let (prev_idx, next_idx) = if let LinkSlot::Occupied { prev, next } = self.links.borrow(token, index) {
            (*prev, *next)
        } else {
             panic!("Invalid index");
        };

        // Detach
        if let Some(prev) = prev_idx {
            if let LinkSlot::Occupied { next, .. } = self.links.borrow_mut(token, prev) {
                *next = next_idx;
            }
        } else {
            self.head = next_idx;
        }

        if let Some(next) = next_idx {
            if let LinkSlot::Occupied { prev, .. } = self.links.borrow_mut(token, next) {
                *prev = prev_idx;
            }
        }

        // Attach to back
        let old_tail = self.tail;
        if let Some(tail_idx) = old_tail {
             if let LinkSlot::Occupied { next, .. } = self.links.borrow_mut(token, tail_idx) {
                 *next = Some(index);
             }
        }

        if let LinkSlot::Occupied { prev, next } = self.links.borrow_mut(token, index) {
            *next = None;
            *prev = old_tail;
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

        let mut links = BrandedVec::with_capacity(len);
        let mut values = BrandedVec::with_capacity(len);

        for (i, item) in items.into_iter().enumerate() {
            let prev = if i == 0 { None } else { Some(i - 1) };
            let next = if i == len - 1 { None } else { Some(i + 1) };

            links.push(LinkSlot::Occupied { prev, next });
            values.push(MaybeUninit::new(item));
        }

        Self {
            links,
            values,
            head: Some(0),
            tail: Some(len - 1),
            free_head: None,
            len,
        }
    }
}

/// Consuming iterator for BrandedDoublyLinkedList.
pub struct IntoIter<T> {
    links: Vec<Option<LinkSlot>>,
    values: Vec<MaybeUninit<T>>,
    current: Option<usize>,
    len: usize,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.current?;
        if let Some(slot) = self.links.get_mut(idx).and_then(|s| s.take()) {
            match slot {
                LinkSlot::Occupied { next, .. } => {
                    self.current = next;
                    self.len -= 1;
                    // Read value
                    // SAFETY: Slot was Occupied, so value is init.
                    unsafe {
                        Some(self.values.get_unchecked(idx).assume_init_read())
                    }
                }
                LinkSlot::Free(_) => None,
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

impl<T> Drop for IntoIter<T> {
    fn drop(&mut self) {
        // Drop all remaining occupied values
        for (i, slot) in self.links.iter().enumerate() {
            if let Some(LinkSlot::Occupied { .. }) = slot {
                unsafe {
                    self.values.get_unchecked_mut(i).assume_init_drop();
                }
            }
        }
    }
}

impl<'brand, T> IntoIterator for BrandedDoublyLinkedList<'brand, T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        let len = self.len;
        let head = self.head;

        // Read fields to avoid Drop
        let links_vec = unsafe { core::ptr::read(&self.links) };
        let values_vec = unsafe { core::ptr::read(&self.values) };

        // Forget self to prevent Drop from running and double-dropping values
        core::mem::forget(self);

        let links: Vec<Option<LinkSlot>> = links_vec.into_iter().map(Some).collect();
        let values: Vec<MaybeUninit<T>> = values_vec.into_iter().collect();

        IntoIter {
            links,
            values,
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

impl<'brand, T> Drop for BrandedDoublyLinkedList<'brand, T> {
    fn drop(&mut self) {
        // We need to drop all occupied values in `values`.
        // Iterating through free list is hard if we want to find occupied.
        // Easier: iterate all slots and check links.

        // Wait, self.links is BrandedVec. accessing it needs token?
        // No, Drop for BrandedVec drops contents.
        // `links` drops LinkSlots (plain enums).
        // `values` drops MaybeUninit<T>. MaybeUninit drop does nothing!
        // We MUST drop T manually.

        // Problem: We don't have a token in Drop.
        // But we own the BrandedVecs.
        // BrandedVec internal `Vec` is accessible via crate?
        // `BrandedVec` field `inner` is `pub(crate)`.

        // We can inspect `links.inner` and `values.inner`.
        // `links.inner` is `Vec<GhostCell<LinkSlot>>`.
        // `values.inner` is `Vec<GhostCell<MaybeUninit<T>>>`.

        // GhostCell allows `into_inner`.
        // But we are in Drop of BrandedDoublyLinkedList.
        // We can't move out of fields in Drop.
        // But we can iterate references if we use unsafe or into_inner on fields?
        // No, fields are dropped after drop() returns.

        // Solution: Since we don't have token, we can't safe-borrow.
        // But we are dropping the structure, so we have exclusive access fundamentally.
        // We can use UnsafeCell::get_mut equivalent or transmute.
        // Since we are `BrandedDoublyLinkedList` dropping, no one else can have token access to our `brand`.
        // Actually, token lifetime `'brand` might outlive the list.
        // But if list is dropped, no one can access it.

        // We can iterate `links` and `values` internal vectors (via some helper or just trusting index sync).
        // `links` and `values` always have same length.

        // But `BrandedVec` doesn't expose iter without token easily?
        // We can rely on `BrandedVec` Drop behavior?
        // `values` contains `MaybeUninit<T>`. Drop of `MaybeUninit` is no-op.
        // So `BrandedVec` dropping `values` will NOT drop `T`.
        // We MUST manually drop.

        // We need to iterate 0..len.
        // We can use `BrandedVec::inner` (it is pub crate).

        let links_len = self.links.len();

        // We iterate indices.
        for i in 0..links_len {
            // Get mutable references to content via GhostCell::get_mut
            // This is safe because we are in Drop and own the list exclusively.
            let link_slot = self.links.inner[i].get_mut();
            let val_slot = self.values.inner[i].get_mut();

            if let LinkSlot::Occupied { .. } = link_slot {
                // Drop value
                unsafe { val_slot.assume_init_drop() };
            }
        }
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
        match unsafe { self.list.links.get_unchecked(token, idx) } {
            LinkSlot::Occupied { .. } => {
                unsafe { Some(self.list.values.get_unchecked(token, idx).assume_init_ref()) }
            }
            _ => None,
        }
    }

    /// Returns a mutable reference to the current element.
    pub fn current_mut<'b>(&'b mut self, token: &'b mut GhostToken<'brand>) -> Option<&'b mut T> {
        let idx = self.current?;
        match unsafe { self.list.links.get_unchecked_mut(token, idx) } {
            LinkSlot::Occupied { .. } => {
                unsafe { Some(self.list.values.get_unchecked_mut(token, idx).assume_init_mut()) }
            }
            _ => None,
        }
    }

    /// Moves the cursor to the next element.
    pub fn move_next(&mut self, token: &GhostToken<'brand>) {
        if let Some(curr_idx) = self.current {
            if let LinkSlot::Occupied { next, .. } = unsafe { self.list.links.get_unchecked(token, curr_idx) } {
                self.current = *next;
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
            if let LinkSlot::Occupied { prev, .. } = unsafe { self.list.links.get_unchecked(token, curr_idx) } {
                self.current = *prev;
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
            let next_idx = if let LinkSlot::Occupied { next, .. } = self.list.links.borrow(token, curr_idx) {
                *next
            } else {
                panic!("Corrupted list");
            };

            // Link new node
            if let LinkSlot::Occupied { prev, next } = self.list.links.borrow_mut(token, new_idx) {
                *prev = Some(curr_idx);
                *next = next_idx;
            }

            // Update current's next
            if let LinkSlot::Occupied { next, .. } = self.list.links.borrow_mut(token, curr_idx) {
                *next = Some(new_idx);
            }

            // Update next's prev or tail
            if let Some(next) = next_idx {
                if let LinkSlot::Occupied { prev, .. } = self.list.links.borrow_mut(token, next) {
                    *prev = Some(new_idx);
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
    pub fn insert_before(&mut self, token: &mut GhostToken<'brand>, value: T) -> usize {
         if let Some(curr_idx) = self.current {
            let new_idx = self.list.alloc(token, value);

            // Get current's prev
            let prev_idx = if let LinkSlot::Occupied { prev, .. } = self.list.links.borrow(token, curr_idx) {
                *prev
            } else {
                panic!("Corrupted list");
            };

            // Link new node
            if let LinkSlot::Occupied { prev, next } = self.list.links.borrow_mut(token, new_idx) {
                *prev = prev_idx;
                *next = Some(curr_idx);
            }

            // Update current's prev
            if let LinkSlot::Occupied { prev, .. } = self.list.links.borrow_mut(token, curr_idx) {
                *prev = Some(new_idx);
            }

            // Update prev's next or head
            if let Some(prev) = prev_idx {
                if let LinkSlot::Occupied { next, .. } = self.list.links.borrow_mut(token, prev) {
                    *next = Some(new_idx);
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

        let (prev_idx, next_idx) = if let LinkSlot::Occupied { prev, next } = self.list.links.borrow(token, curr_idx) {
            (*prev, *next)
        } else {
            panic!("Corrupted list");
        };

        // Update prev node or head
        if let Some(prev) = prev_idx {
            if let LinkSlot::Occupied { next, .. } = self.list.links.borrow_mut(token, prev) {
                *next = next_idx;
            }
        } else {
            self.list.head = next_idx;
        }

        // Update next node or tail
        if let Some(next) = next_idx {
            if let LinkSlot::Occupied { prev, .. } = self.list.links.borrow_mut(token, next) {
                *prev = prev_idx;
            }
        } else {
            self.list.tail = prev_idx;
        }

        // Extract value
        let value = unsafe {
            let val_slot = self.list.values.borrow(token, curr_idx);
            val_slot.assume_init_read()
        };

        // Free slot
        let slot = self.list.links.borrow_mut(token, curr_idx);
        *slot = LinkSlot::Free(self.list.free_head);
        self.list.free_head = Some(curr_idx);

        self.list.len -= 1;
        self.current = next_idx;

        Some(value)
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
