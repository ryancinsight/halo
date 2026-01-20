//! `BrandedVecDeque` — a growable ring buffer deque with token-gated access.
//!
//! This provides a growable double-ended queue using the `GhostCell` pattern.
//! It is implemented from scratch using raw allocation to allow efficient ring
//! buffer management and zero-cost token access, avoiding `std::vec::Vec`.

use crate::collections::ZeroCopyOps;
use crate::{GhostCell, GhostToken};
use core::mem::{self, MaybeUninit};
use core::ptr::{self, NonNull};
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};

/// A double-ended queue of token-gated elements.
pub struct BrandedVecDeque<'brand, T> {
    /// Pointer to the allocated memory.
    ptr: NonNull<GhostCell<'brand, T>>,
    /// Capacity of the allocation.
    cap: usize,
    /// The index of the first element.
    head: usize,
    /// The number of elements in the deque.
    len: usize,
}

// Safety: BrandedVecDeque owns the memory and the data.
unsafe impl<'brand, T: Send> Send for BrandedVecDeque<'brand, T> {}
unsafe impl<'brand, T: Sync> Sync for BrandedVecDeque<'brand, T> {}

impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Creates an empty deque.
    pub fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            cap: 0,
            head: 0,
            len: 0,
        }
    }

    /// Creates an empty deque with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::new();
        }
        let layout = Layout::array::<GhostCell<'brand, T>>(capacity).unwrap();
        // Ensure layout size > 0 if capacity > 0 (T could be ZST)
        let ptr = if layout.size() > 0 {
            unsafe {
                let p = alloc(layout);
                if p.is_null() {
                    handle_alloc_error(layout);
                }
                NonNull::new_unchecked(p as *mut GhostCell<'brand, T>)
            }
        } else {
            NonNull::dangling()
        };

        Self {
            ptr,
            cap: capacity,
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
        self.cap
    }

    /// Returns the index of the tail (where the next element will be pushed).
    #[inline(always)]
    fn tail(&self) -> usize {
        if self.cap == 0 {
            0
        } else {
            (self.head + self.len) % self.cap
        }
    }

    /// Internal helper to grow the buffer.
    fn grow(&mut self) {
        let old_cap = self.cap;
        let new_cap = if old_cap == 0 { 4 } else { old_cap * 2 };

        let old_layout = if old_cap > 0 {
            Some(Layout::array::<GhostCell<'brand, T>>(old_cap).unwrap())
        } else {
            None
        };

        let new_layout = Layout::array::<GhostCell<'brand, T>>(new_cap).unwrap();

        // ZST check
        if new_layout.size() == 0 {
            self.cap = usize::MAX;
            return;
        }

        unsafe {
            let new_ptr = alloc(new_layout);
            if new_ptr.is_null() {
                handle_alloc_error(new_layout);
            }
            let new_ptr = new_ptr as *mut GhostCell<'brand, T>;

            // Copy elements to new buffer
            // We align head to 0 in the new buffer for simplicity
            if old_cap > 0 {
                let ptr = self.ptr.as_ptr();
                let head = self.head;
                let len = self.len;

                // Amount of data from head to end of allocation
                let upper_len = old_cap - head;
                // Actual size of first chunk
                let head_len = if len <= upper_len { len } else { upper_len };
                // Size of second chunk
                let tail_len = len - head_len;

                // Copy head part
                ptr::copy_nonoverlapping(ptr.add(head), new_ptr, head_len);

                // Copy tail part if any
                if tail_len > 0 {
                    ptr::copy_nonoverlapping(ptr, new_ptr.add(head_len), tail_len);
                }

                // Deallocate old
                if let Some(layout) = old_layout {
                    dealloc(ptr as *mut u8, layout);
                }
            }

            self.ptr = NonNull::new_unchecked(new_ptr);
            self.cap = new_cap;
            self.head = 0;
        }
    }

    /// Ensure capacity for one more element.
    fn ensure_capacity(&mut self) {
        if self.len == self.cap {
            self.grow();
        }
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) {
        self.ensure_capacity();
        let tail = self.tail();
        unsafe {
            let ptr = self.ptr.as_ptr().add(tail);
            ptr.write(GhostCell::new(value));
        }
        self.len += 1;
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) {
        self.ensure_capacity();
        self.head = if self.head == 0 {
            self.cap - 1
        } else {
            self.head - 1
        };
        unsafe {
            let ptr = self.ptr.as_ptr().add(self.head);
            ptr.write(GhostCell::new(value));
        }
        self.len += 1;
    }

    /// Pops from the back.
    pub fn pop_back(&mut self) -> Option<GhostCell<'brand, T>> {
        if self.is_empty() {
            return None;
        }
        let tail_idx = if self.tail() == 0 {
            self.cap - 1
        } else {
            self.tail() - 1
        };
        self.len -= 1;
        unsafe {
            let ptr = self.ptr.as_ptr().add(tail_idx);
            Some(ptr::read(ptr))
        }
    }

    /// Pops from the front.
    pub fn pop_front(&mut self) -> Option<GhostCell<'brand, T>> {
        if self.is_empty() {
            return None;
        }
        let head_idx = self.head;
        self.head = (self.head + 1) % self.cap;
        self.len -= 1;
        unsafe {
            let ptr = self.ptr.as_ptr().add(head_idx);
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
        let actual_idx = (self.head + idx) % self.cap;
        unsafe {
            let ptr = self.ptr.as_ptr().add(actual_idx);
            Some((&*ptr).borrow(token))
        }
    }

    /// Returns an exclusive reference to the element at `idx`, if in bounds.
    #[inline]
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        if idx >= self.len {
            return None;
        }
        let actual_idx = (self.head + idx) % self.cap;
        unsafe {
            let ptr = self.ptr.as_ptr().add(actual_idx);
            Some((&mut *ptr).borrow_mut(token))
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
        if self.len == 0 {
            None
        } else {
            self.get(token, self.len - 1)
        }
    }

    /// Exclusive iteration via callback.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        for i in 0..self.len {
            let actual_idx = (self.head + i) % self.cap;
            unsafe {
                let ptr = self.ptr.as_ptr().add(actual_idx);
                let item = (&mut *ptr).borrow_mut(token);
                f(item);
            }
        }
    }

    /// Shared iteration via callback.
    pub fn for_each(&self, token: &GhostToken<'brand>, mut f: impl FnMut(&T)) {
        for i in 0..self.len {
            let actual_idx = (self.head + i) % self.cap;
            unsafe {
                let ptr = self.ptr.as_ptr().add(actual_idx);
                let item = (&*ptr).borrow(token);
                f(item);
            }
        }
    }

    /// Returns a pair of slices representing the deque contents.
    pub fn as_slices<'a>(&'a self, _token: &'a GhostToken<'brand>) -> (&'a [T], &'a [T]) {
        if self.len == 0 {
            return (&[], &[]);
        }
        let ptr = self.ptr.as_ptr();
        let head = self.head;
        let cap = self.cap;

        unsafe {
            if head + self.len <= cap {
                // Contiguous
                let s1 = std::slice::from_raw_parts(ptr.add(head) as *const T, self.len);
                (s1, &[])
            } else {
                // Wrapped
                let len1 = cap - head;
                let len2 = self.len - len1;
                let s1 = std::slice::from_raw_parts(ptr.add(head) as *const T, len1);
                let s2 = std::slice::from_raw_parts(ptr as *const T, len2);
                (s1, s2)
            }
        }
    }

    /// Returns a pair of mutable slices representing the deque contents.
    pub fn as_mut_slices<'a>(
        &'a self,
        _token: &'a mut GhostToken<'brand>,
    ) -> (&'a mut [T], &'a mut [T]) {
        if self.len == 0 {
            return (&mut [], &mut []);
        }
        let ptr = self.ptr.as_ptr();
        let head = self.head;
        let cap = self.cap;

        unsafe {
            if head + self.len <= cap {
                let s1 = std::slice::from_raw_parts_mut(ptr.add(head) as *mut T, self.len);
                (s1, &mut [])
            } else {
                let len1 = cap - head;
                let len2 = self.len - len1;
                let s1 = std::slice::from_raw_parts_mut(ptr.add(head) as *mut T, len1);
                let s2 = std::slice::from_raw_parts_mut(ptr as *mut T, len2);
                (s1, s2)
            }
        }
    }

    /// Iterates over the elements.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a T> + 'a {
        let (s1, s2) = self.as_slices(token);
        s1.iter().chain(s2.iter())
    }

    /// Iterates over the elements (mutable).
    pub fn iter_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
    ) -> impl Iterator<Item = &'a mut T> + 'a {
        let (s1, s2) = self.as_mut_slices(token);
        s1.iter_mut().chain(s2.iter_mut())
    }

    /// Rotates the deque `k` steps to the left.
    ///
    /// This corresponds to `rotate_left` on `slice` or `VecDeque`.
    /// Elements are shifted such that the element at `k` becomes the first element.
    ///
    /// # Panics
    /// Panics if `k > len`.
    pub fn rotate_left(&mut self, k: usize) {
        assert!(k <= self.len, "rotation amount too large");
        if k == 0 || k == self.len {
            return;
        }

        if self.len == self.cap {
            self.head = (self.head + k) % self.cap;
        } else {
            // Physical movement required for non-full deque
            // We can move k elements from front to back
            for _ in 0..k {
                if let Some(val) = self.pop_front() {
                    self.push_back(val.into_inner());
                }
            }
        }
    }

    /// Rotates the deque `k` steps to the right.
    ///
    /// # Panics
    /// Panics if `k > len`.
    pub fn rotate_right(&mut self, k: usize) {
        assert!(k <= self.len, "rotation amount too large");
        if k == 0 || k == self.len {
            return;
        }

        if self.len == self.cap {
            self.head = (self.head + self.cap - k) % self.cap;
        } else {
            // Physical movement required
            for _ in 0..k {
                if let Some(val) = self.pop_back() {
                    self.push_front(val.into_inner());
                }
            }
        }
    }

    /// Rearranges the internal storage so that the deque contents are contiguous.
    ///
    /// Returns a mutable slice to the contiguous contents.
    /// This may require allocating a new buffer if the deque is wrapped.
    pub fn make_contiguous(&mut self) -> &mut [GhostCell<'brand, T>] {
        if self.len == 0 {
            return &mut [];
        }

        if mem::size_of::<T>() == 0 {
            // For ZSTs, elements don't occupy memory, so they are always "contiguous"
            // (or rather, location doesn't matter). Just return a slice.
            return unsafe {
                std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
            };
        }

        let ptr = self.ptr.as_ptr();
        let head = self.head;
        let tail = self.tail();
        let cap = self.cap;

        unsafe {
            if tail <= head {
                // Wrapped case.
                // The logical contents are [head..cap] followed by [0..tail].
                // We want to make them contiguous.
                // The safest way is to allocate a new buffer and copy the parts in order.

                let head_len = cap - head;
                let tail_len = tail;

                // Allocate new buffer
                let new_cap = self.cap;
                let new_layout = Layout::array::<GhostCell<'brand, T>>(new_cap).unwrap();
                // Check done at start of function, but strictly layout size > 0 here.
                let new_ptr = alloc(new_layout) as *mut GhostCell<'brand, T>;
                if new_ptr.is_null() { handle_alloc_error(new_layout); }

                // Copy head part to start of new buffer
                ptr::copy_nonoverlapping(ptr.add(head), new_ptr, head_len);

                // Copy tail part to follow head part
                if tail_len > 0 {
                    ptr::copy_nonoverlapping(ptr, new_ptr.add(head_len), tail_len);
                }

                // Deallocate old buffer
                let old_layout = Layout::array::<GhostCell<'brand, T>>(self.cap).unwrap();
                dealloc(ptr as *mut u8, old_layout);

                // Update pointers
                self.ptr = NonNull::new_unchecked(new_ptr);
                self.head = 0;
            }

            // Now contiguous.
            std::slice::from_raw_parts_mut(self.ptr.as_ptr().add(self.head), self.len)
        }
    }
}

impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Creates a splicing iterator that replaces the specified range in the deque
    /// with the given `replace_with` iterator and yields the removed items.
    ///
    /// `replace_with` does not need to be the same length as the removed range.
    ///
    /// # logic
    /// This method rotates the deque to bring the range to the front, yields elements,
    /// pushes new elements to the back, and rotates again to restore order.
    pub fn splice<R, I>(
        &mut self,
        range: R,
        replace_with: I,
    ) -> Splice<'_, 'brand, T, I::IntoIter>
    where
        R: std::ops::RangeBounds<usize>,
        I: IntoIterator<Item = T>,
    {
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

        let start = std::cmp::min(start, len);
        let end = std::cmp::min(end, len);

        let count = end.saturating_sub(start);
        let suffix_len = len - end;

        // Choose rotation strategy to bring the range to the front
        // Cost 1: start (prefix len)
        // Cost 2: suffix_len + count
        if start <= suffix_len + count {
            self.rotate_left(start);
        } else {
            self.rotate_right(suffix_len + count);
        }

        Splice {
            deque: self,
            remaining: count,
            replacement: replace_with.into_iter(),
            suffix_len,
        }
    }
}

/// A splicing iterator for `BrandedVecDeque`.
pub struct Splice<'a, 'brand, T, I>
where
    I: Iterator<Item = T>,
{
    deque: &'a mut BrandedVecDeque<'brand, T>,
    remaining: usize,
    replacement: I,
    suffix_len: usize,
}

impl<'a, 'brand, T, I> Iterator for Splice<'a, 'brand, T, I>
where
    I: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.deque.pop_front().map(GhostCell::into_inner)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, 'brand, T, I> ExactSizeIterator for Splice<'a, 'brand, T, I> where I: Iterator<Item = T> {}

impl<'a, 'brand, T, I> Drop for Splice<'a, 'brand, T, I>
where
    I: Iterator<Item = T>,
{
    fn drop(&mut self) {
        while self.remaining > 0 {
            self.remaining -= 1;
            self.deque.pop_front();
        }

        for item in self.replacement.by_ref() {
            self.deque.push_back(item);
        }

        if self.suffix_len > 0 {
            self.deque.rotate_left(self.suffix_len);
        }
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
        if self.cap > 0 && mem::size_of::<T>() > 0 {
            unsafe {
                let layout = Layout::array::<GhostCell<'brand, T>>(self.cap).unwrap();
                dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
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

impl<'brand, T> FromIterator<T> for BrandedVecDeque<'brand, T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let (lower, _upper) = iter.size_hint();
        let mut deque = Self::with_capacity(lower);
        deque.extend(iter);
        deque
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
    pub fn drain<R>(&mut self, range: R) -> Drain<'_, 'brand, T>
    where
        R: std::ops::RangeBounds<usize>,
    {
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

        let start = std::cmp::min(start, len);
        let end = std::cmp::min(end, len);

        if start >= end {
            return Drain {
                deque: self,
                remaining: 0,
                cleanup_is_left: false,
                cleanup_len: 0,
            };
        }

        let count = end - start;
        let suffix_len = len - end;

        // Choose rotation strategy to bring the drain range to the front
        // Cost 1: start (prefix len)
        // Cost 2: suffix_len + count
        let (cleanup_is_left, cleanup_len) = if start <= suffix_len + count {
            // Strategy 1: Rotate prefix to back
            self.rotate_left(start);
            // Cleanup: Rotate right start (prefix len)
            (false, start)
        } else {
            // Strategy 2: Rotate suffix+range to front
            self.rotate_right(suffix_len + count);
            // Cleanup: Rotate left suffix_len
            (true, suffix_len)
        };

        Drain {
            deque: self,
            remaining: count,
            cleanup_is_left,
            cleanup_len,
        }
    }
}

/// A draining iterator for `BrandedVecDeque`.
pub struct Drain<'a, 'brand, T> {
    deque: &'a mut BrandedVecDeque<'brand, T>,
    remaining: usize,
    cleanup_is_left: bool,
    cleanup_len: usize,
}

impl<'a, 'brand, T> Iterator for Drain<'a, 'brand, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        // Always pop from front as we rotated the range to the front
        self.deque.pop_front().map(GhostCell::into_inner)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<'a, 'brand, T> ExactSizeIterator for Drain<'a, 'brand, T> {}

impl<'a, 'brand, T> Drop for Drain<'a, 'brand, T> {
    fn drop(&mut self) {
        // Exhaust remaining
        while self.remaining > 0 {
            self.remaining -= 1;
            self.deque.pop_front();
        }

        // Cleanup rotation
        if self.cleanup_len > 0 {
            if self.cleanup_is_left {
                self.deque.rotate_left(self.cleanup_len);
            } else {
                self.deque.rotate_right(self.cleanup_len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;
    use std::cell::RefCell;
    use std::rc::Rc;

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
