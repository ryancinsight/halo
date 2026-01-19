//! `BrandedVecDeque` — a growable ring buffer deque with token-gated access.
//!
//! This provides a growable double-ended queue using the `GhostCell` pattern.
//! It is implemented from scratch using a `Vec` of `MaybeUninit` cells to allow
//! efficient ring buffer management and zero-cost token access.

use core::mem::MaybeUninit;
use crate::{GhostCell, GhostToken};
use crate::collections::ZeroCopyOps;
use std::vec::Vec;
use std::ptr;

/// A double-ended queue of token-gated elements.
pub struct BrandedVecDeque<'brand, T> {
    /// The backing storage. Elements are stored in a ring buffer fashion.
    /// We use MaybeUninit to manage initialization manually.
    buffer: Vec<MaybeUninit<GhostCell<'brand, T>>>,
    /// The index of the first element.
    head: usize,
    /// The number of elements in the deque.
    len: usize,
}

impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Creates an empty deque.
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            head: 0,
            len: 0,
        }
    }

    /// Creates an empty deque with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        // SAFETY: MaybeUninit allows uninitialized memory.
        unsafe { buffer.set_len(capacity); }
        Self {
            buffer,
            head: 0,
            len: 0,
        }
    }

    /// Number of elements.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Current capacity.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Returns the index of the tail (where the next element will be pushed).
    #[inline(always)]
    fn tail(&self) -> usize {
        if self.capacity() == 0 {
            0
        } else {
            (self.head + self.len) % self.capacity()
        }
    }

    /// Internal helper to grow the buffer.
    fn grow(&mut self) {
        let old_cap = self.capacity();
        // If empty, just reserve.
        if old_cap == 0 {
            self.buffer.reserve(4);
            // SAFETY: MaybeUninit allows uninitialized memory.
            unsafe { self.buffer.set_len(self.buffer.capacity()); }
            return;
        }

        let new_cap = old_cap * 2;
        self.buffer.reserve(new_cap - old_cap);

        // Safety: We are extending with uninitialized memory which is valid for MaybeUninit.
        // We set the vector length to the new capacity so we can index into it.
        unsafe {
            self.buffer.set_len(self.buffer.capacity());
        }

        let cap = self.buffer.len(); // Actual new capacity

        // Rearrange elements if needed (unwrap the ring)
        if self.head + self.len > old_cap {
            let head_len = old_cap - self.head;
            let new_head = cap - head_len;

            unsafe {
                let ptr = self.buffer.as_mut_ptr();
                ptr::copy_nonoverlapping(
                    ptr.add(self.head),
                    ptr.add(new_head),
                    head_len
                );
            }
            self.head = new_head;
        }
    }

    /// Ensure capacity for one more element.
    fn ensure_capacity(&mut self) {
        if self.len == self.capacity() {
            // First time setup if capacity is 0 but buffer len is 0.
            if self.buffer.len() < self.buffer.capacity() {
                unsafe { self.buffer.set_len(self.buffer.capacity()); }
            }

            if self.len == self.buffer.len() {
                self.grow();
                // Ensure len is set after grow (grow sets it)
                if self.buffer.len() < self.buffer.capacity() {
                     unsafe { self.buffer.set_len(self.buffer.capacity()); }
                }
            }
        }
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) {
        self.ensure_capacity();
        let tail = self.tail();
        unsafe {
            let ptr = self.buffer.get_unchecked_mut(tail).as_mut_ptr();
            ptr.write(GhostCell::new(value));
        }
        self.len += 1;
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) {
        self.ensure_capacity();
        self.head = if self.head == 0 { self.capacity() - 1 } else { self.head - 1 };
        unsafe {
            let ptr = self.buffer.get_unchecked_mut(self.head).as_mut_ptr();
            ptr.write(GhostCell::new(value));
        }
        self.len += 1;
    }

    /// Pops from the back.
    pub fn pop_back(&mut self) -> Option<GhostCell<'brand, T>> {
        if self.is_empty() {
            return None;
        }
        let tail = if self.tail() == 0 { self.capacity() - 1 } else { self.tail() - 1 };
        self.len -= 1;
        unsafe {
            let ptr = self.buffer.get_unchecked(tail).as_ptr();
            Some(ptr::read(ptr))
        }
    }

    /// Pops from the front.
    pub fn pop_front(&mut self) -> Option<GhostCell<'brand, T>> {
        if self.is_empty() {
            return None;
        }
        let head = self.head;
        self.head = (self.head + 1) % self.capacity();
        self.len -= 1;
        unsafe {
            let ptr = self.buffer.get_unchecked(head).as_ptr();
            Some(ptr::read(ptr))
        }
    }

    /// Clears the deque.
    pub fn clear(&mut self) {
        while self.pop_front().is_some() {}
    }

    /// Returns a shared reference to the element at `idx`, if in bounds.
    #[inline]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        if idx >= self.len {
            return None;
        }
        let actual_idx = (self.head + idx) % self.capacity();
        unsafe {
            let ptr = self.buffer.get_unchecked(actual_idx).as_ptr();
            Some((&*ptr).borrow(token))
        }
    }

    /// Returns an exclusive reference to the element at `idx`, if in bounds.
    #[inline]
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> Option<&'a mut T> {
        if idx >= self.len {
            return None;
        }
        let actual_idx = (self.head + idx) % self.capacity();
        unsafe {
            let ptr = self.buffer.get_unchecked(actual_idx).as_ptr();
            // We need mutable pointer to GhostCell
            let cell_ptr = ptr as *mut GhostCell<'brand, T>;
            Some((&mut *cell_ptr).borrow_mut(token))
        }
    }

    /// Returns a shared reference to the front element.
    #[inline]
    pub fn front<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        self.get(token, 0)
    }

    /// Returns a shared reference to the back element.
    #[inline]
    pub fn back<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        if self.len == 0 { None } else { self.get(token, self.len - 1) }
    }

    /// Exclusive iteration via callback.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        for i in 0..self.len {
             // Optimize: avoid repeatedly calling get_mut which calculates index
             let actual_idx = (self.head + i) % self.capacity();
             unsafe {
                let ptr = self.buffer.get_unchecked(actual_idx).as_ptr() as *mut GhostCell<'brand, T>;
                let item = (&mut *ptr).borrow_mut(token);
                f(item);
             }
        }
    }

    /// Shared iteration via callback.
    pub fn for_each(&self, token: &GhostToken<'brand>, mut f: impl FnMut(&T)) {
        for i in 0..self.len {
             let actual_idx = (self.head + i) % self.capacity();
             unsafe {
                let ptr = self.buffer.get_unchecked(actual_idx).as_ptr();
                let item = (&*ptr).borrow(token);
                f(item);
             }
        }
    }

    /// Returns a pair of slices representing the deque contents.
    pub fn as_slices<'a>(&'a self, _token: &'a GhostToken<'brand>) -> (&'a [T], &'a [T]) {
        if self.len == 0 || self.capacity() == 0 {
             return (&[], &[]);
        }
        let tail = self.tail();
        let (s1, s2) = if self.head < tail {
            // Contiguous
            (&self.buffer[self.head..tail], &self.buffer[0..0]) // Empty second slice
        } else {
            // Wrapped (or full where tail == head)
            (&self.buffer[self.head..], &self.buffer[0..tail])
        };

        unsafe {
            (
                std::slice::from_raw_parts(s1.as_ptr() as *const T, s1.len()),
                std::slice::from_raw_parts(s2.as_ptr() as *const T, s2.len()),
            )
        }
    }

    /// Returns a pair of mutable slices representing the deque contents.
    pub fn as_mut_slices<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> (&'a mut [T], &'a mut [T]) {
        if self.len == 0 || self.capacity() == 0 {
             return (&mut [], &mut []);
        }
        // Note: We need unsafe access to the buffer via &self
        let ptr = self.buffer.as_ptr() as *mut MaybeUninit<GhostCell<'brand, T>>;
        let cap = self.buffer.capacity();

        // Reconstruct slices manually based on head/tail logic
        // We know we hold &mut GhostToken, so we have exclusive access to contents.
        // We hold &self, so buffer is stable.

        let tail = self.tail();

        unsafe {
            if self.head < tail {
                 let s1_ptr = ptr.add(self.head);
                 let s1_len = tail - self.head;
                 (
                     std::slice::from_raw_parts_mut(s1_ptr as *mut T, s1_len),
                     std::slice::from_raw_parts_mut(ptr as *mut T, 0)
                 )
            } else {
                 let s1_ptr = ptr.add(self.head);
                 let s1_len = cap - self.head;
                 let s2_ptr = ptr;
                 let s2_len = tail;
                 (
                     std::slice::from_raw_parts_mut(s1_ptr as *mut T, s1_len),
                     std::slice::from_raw_parts_mut(s2_ptr as *mut T, s2_len)
                 )
            }
        }
    }

    /// Iterates over the elements.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a T> + 'a {
        let (s1, s2) = self.as_slices(token);
        s1.iter().chain(s2.iter())
    }

    /// Iterates over the elements (mutable).
    /// Note: This consumes `&mut token`. If you need to mutate in a loop,
    /// consider `for_each_mut` or `as_mut_slices`.
    pub fn iter_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> impl Iterator<Item = &'a mut T> + 'a {
        let (s1, s2) = self.as_mut_slices(token);
        s1.iter_mut().chain(s2.iter_mut())
    }
}

impl<'brand, T> Default for BrandedVecDeque<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T> Drop for BrandedVecDeque<'brand, T> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<'brand, T> ZeroCopyOps<'brand, T> for BrandedVecDeque<'brand, T> {
    #[inline(always)]
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).find(|&item| f(item))
    }

    #[inline(always)]
    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).any(|item| f(item))
    }

    #[inline(always)]
    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).all(|item| f(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;
    use std::rc::Rc;
    use std::cell::RefCell;

    #[test]
    fn branded_vec_deque_basic() {
        GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::new();
            dq.push_back(10);
            dq.push_front(20);
            assert_eq!(dq.len(), 2);
            assert_eq!(*dq.get(&token, 0).unwrap(), 20);
            assert_eq!(*dq.get(&token, 1).unwrap(), 10);

            *dq.get_mut(&mut token, 0).unwrap() += 5;
            assert_eq!(*dq.get(&token, 0).unwrap(), 25);
        });
    }

    #[test]
    fn branded_vec_deque_growth() {
        GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::new();
            for i in 0..100 {
                dq.push_back(i);
            }
            assert_eq!(dq.len(), 100);
            for i in 0..100 {
                assert_eq!(*dq.get(&token, i).unwrap(), i);
            }
        });
    }

    #[test]
    fn branded_vec_deque_wrap_growth() {
         GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::with_capacity(4);
            dq.push_back(1);
            dq.push_back(2);
            dq.push_back(3);
            dq.pop_front(); // Remove 1. head=1, len=2.
            dq.push_back(4);
            dq.push_back(5); // Should trigger growth?
            // cap=4. elements: [5, 2, 3, 4] (wrapped if implemented that way)
            // If we push one more:
            dq.push_back(6);

            // Should be sorted 2,3,4,5,6
            assert_eq!(dq.len(), 5);
            let vec: Vec<_> = dq.iter(&token).copied().collect();
            assert_eq!(vec, vec![2, 3, 4, 5, 6]);
        });
    }

    #[test]
    fn branded_vec_deque_drop() {
        struct Dropper(Rc<RefCell<i32>>);
        impl Drop for Dropper {
            fn drop(&mut self) {
                *self.0.borrow_mut() += 1;
            }
        }

        let counter = Rc::new(RefCell::new(0));
        {
            GhostToken::new(|mut _token| {
                let mut dq = BrandedVecDeque::new();
                dq.push_back(Dropper(counter.clone()));
                dq.push_back(Dropper(counter.clone()));
                dq.pop_front(); // One drops here
            });
            // Second drops here
        }
        assert_eq!(*counter.borrow(), 2);
    }

    #[test]
    fn branded_vec_deque_slices() {
        GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::with_capacity(4);
            dq.push_back(1);
            dq.push_back(2);
            dq.push_back(3);
            dq.push_back(4);
            // Full contiguous: [1,2,3,4]
            let (s1, s2) = dq.as_slices(&token);
            assert_eq!(s1, &[1, 2, 3, 4]);
            assert!(s2.is_empty());

            dq.pop_front(); // [_, 2, 3, 4]
            dq.push_back(5); // [5, 2, 3, 4] (wrapped)

            let (s1, s2) = dq.as_slices(&token);
            // head is at index 1 (val 2). tail is at index 1 (next val).
            // elements: 2, 3, 4, 5
            // s1: buffer[1..4] -> [2, 3, 4]
            // s2: buffer[0..1] -> [5]
            assert_eq!(s1, &[2, 3, 4]);
            assert_eq!(s2, &[5]);
        });
    }
}

impl<'brand, T> FromIterator<T> for BrandedVecDeque<'brand, T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut buffer: Vec<MaybeUninit<GhostCell<'brand, T>>> = Vec::new();
        for item in iter {
             buffer.push(MaybeUninit::new(GhostCell::new(item)));
        }
        let len = buffer.len();
        Self {
            buffer,
            head: 0,
            len,
        }
    }
}

impl<'brand, T> IntoIterator for BrandedVecDeque<'brand, T> {
    type Item = T;
    type IntoIter = IntoIter<'brand, T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { deque: self }
    }
}

pub struct IntoIter<'brand, T> {
    deque: BrandedVecDeque<'brand, T>,
}

impl<'brand, T> Iterator for IntoIter<'brand, T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.deque.pop_front().map(|cell| cell.into_inner())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.deque.len(), Some(self.deque.len()))
    }
}

impl<'brand, T> ExactSizeIterator for IntoIter<'brand, T> {}

impl<'brand, T> Extend<T> for BrandedVecDeque<'brand, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item);
        }
    }
}

impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Creates a draining iterator that removes the specified range in the deque
    /// and yields the removed items.
    ///
    /// Note: This simple implementation collects into a Vec and returns its iterator.
    /// This is not as efficient as a lazy iterator but satisfies the API.
    pub fn drain<R>(&mut self, range: R) -> std::vec::IntoIter<T>
    where
        R: std::ops::RangeBounds<usize>,
    {
        // Simple inefficient implementation:
        // 1. Identify indices to remove.
        // 2. Extract them.
        // 3. Close the gap.

        let len = self.len();
        let start = match range.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            std::ops::Bound::Included(&n) => n + 1,
            std::ops::Bound::Excluded(&n) => n,
            std::ops::Bound::Unbounded => len,
        };

        if start >= end || start >= len {
            return Vec::new().into_iter();
        }
        let end = std::cmp::min(end, len);
        let count = end - start;

        // We will rotate the deque so that the range is at the front or back?
        // Or just move elements one by one?
        // Since we need to return an iterator, we can extract to a Vec.

        // This is tricky to do efficiently in-place without a custom iterator.
        // I'll implement a naive remove-in-loop approach.
        // But removing from middle is O(N). Doing it 'count' times is O(count * N).
        // Since this is "drain", users expect linear time total.

        // Optimization:
        // 1. Rotate so start is at 0? No, `head` moves.
        // 2. Move elements from 0..start to tail?

        // Let's rely on standard logic:
        // Move elements after `end` to `start`.
        // The elements at `start..end` are "overwritten" or moved out.

        // Because of ring buffer, this is complex.
        // Fallback:
        // Reconstruct the deque.
        // Create new deque.
        // Push 0..start.
        // Push end..len.
        // Return start..end.

        let mut new_dq = Self::with_capacity(self.capacity());
        let mut drain_items = Vec::with_capacity(count);

        // We can iterate efficiently using index
        for i in 0..len {
            // We need to extract the element.
            // But we can't easily extract from random index without moving.
            // But we are rebuilding, so we can pop_front everything.
            let item = self.pop_front().unwrap().into_inner();
            if i >= start && i < end {
                drain_items.push(item);
            } else {
                new_dq.push_back(item);
            }
        }

        *self = new_dq;
        drain_items.into_iter()
    }
}
