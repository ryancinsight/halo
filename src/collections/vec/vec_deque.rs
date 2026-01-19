//! `BrandedVecDeque` — a growable ring buffer deque with token-gated access.
//!
//! This provides a growable double-ended queue using the `GhostCell` pattern.
//! It is implemented from scratch using raw allocation to allow efficient ring
//! buffer management and zero-cost token access, avoiding `std::vec::Vec`.

use core::mem::{self, MaybeUninit};
use core::ptr::{self, NonNull};
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use crate::{GhostCell, GhostToken};
use crate::collections::ZeroCopyOps;

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
        self.head = if self.head == 0 { self.cap - 1 } else { self.head - 1 };
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
        let tail_idx = if self.tail() == 0 { self.cap - 1 } else { self.tail() - 1 };
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
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> Option<&'a mut T> {
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
        if self.len == 0 { None } else { self.get(token, self.len - 1) }
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
    pub fn as_mut_slices<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> (&'a mut [T], &'a mut [T]) {
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
    ///
    /// The iterator yields the elements in the range. When the iterator is dropped,
    /// the remaining elements in the deque are shifted to close the gap.
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

        if start >= end || start >= len {
            // Empty range or out of bounds (effectively empty if start >= len)
            return Drain {
                deque: self,
                start,
                count: 0,
                iter_pos: 0,
                drained: 0,
            };
        }
        let end = std::cmp::min(end, len);
        let count = end - start;

        Drain {
            deque: self,
            start,
            count,
            iter_pos: 0,
            drained: 0,
        }
    }
}

/// A draining iterator for `BrandedVecDeque`.
pub struct Drain<'a, 'brand, T> {
    deque: &'a mut BrandedVecDeque<'brand, T>,
    /// Index of the start of the drain range (logical index)
    start: usize,
    /// Number of elements to drain
    count: usize,
    /// Current iteration position relative to `start`
    iter_pos: usize,
    /// Number of elements actually drained (yielded or dropped)
    drained: usize,
}

impl<'a, 'brand, T> Iterator for Drain<'a, 'brand, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iter_pos < self.count {
            // Get element at start + iter_pos
            // This is just a read + logical remove (but we don't shift yet)
            // We use `ptr::read` to take the value out.
            // The slot becomes logically uninitialized until we shift.

            let idx = self.start + self.iter_pos;
            let actual_idx = (self.deque.head + idx) % self.deque.cap;

            unsafe {
                let ptr = self.deque.ptr.as_ptr().add(actual_idx);
                let cell = ptr::read(ptr);
                self.iter_pos += 1;
                self.drained += 1; // Mark as drained
                Some(cell.into_inner())
            }
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.count - self.iter_pos;
        (remaining, Some(remaining))
    }
}

impl<'a, 'brand, T> Drop for Drain<'a, 'brand, T> {
    fn drop(&mut self) {
        // Drop remaining elements in the range
        while self.iter_pos < self.count {
             let idx = self.start + self.iter_pos;
             let actual_idx = (self.deque.head + idx) % self.deque.cap;
             unsafe {
                 let ptr = self.deque.ptr.as_ptr().add(actual_idx);
                 ptr::drop_in_place(ptr);
             }
             self.iter_pos += 1;
             // self.drained does not need increment here as we handle total range
        }

        // Shift elements to close the gap
        // The gap is `[start, start + count)`.
        // We need to move elements after `start + count` to `start`.
        // Or move elements before `start` to `start + count`?
        // std::VecDeque chooses the smaller move.

        let start = self.start;
        let count = self.count;
        let deque_len = self.deque.len;
        let tail_len = deque_len - (start + count);
        let head_len = start;

        // We removed `count` elements.
        // New length will be deque_len - count.

        if head_len < tail_len {
            // Move head part forward (towards gap)
            // Gap is at `start`. We move `0..start` to `count..start+count`.
            // Wait, we move `0..start` to `0+count..start+count`?
            // No, we shift `0..start` RIGHT by `count` positions?
            // No, we removed `count`. We want to close the gap.
            // If we shift head, we shift it RIGHT? No, that opens a gap at 0.
            // Wait.
            // Old state: [H...H] [G...G] [T...T]
            //             0..start  start..end  end..len
            // We want:   [H...H] [T...T]
            //
            // If we move T to left: [H...H] [T...T] ...
            // If we move H to right: ... [H...H] [T...T]
            //
            // If we move H to right: head moves right by `count`.

            // Move 0..start to 0+count..start+count
            // Since ring buffer, we use `wrap_copy`.

            unsafe {
                self.deque.wrap_copy(self.deque.head, self.deque.head + count, head_len);
                self.deque.head = (self.deque.head + count) % self.deque.cap;
            }
        } else {
            // Move tail part left
            // Move start+count..len to start..len-count
            let src_logical = start + count;
            let dst_logical = start;
            let len = tail_len;

            // Calculate actual indices
            // We need `wrap_copy`. But logical indices are easier.
            // Implementation of `wrap_copy` is needed.
            // Wait, I don't have `wrap_copy`. I need to implement it or inline it.

            // Inline logic for moving tail left
            unsafe {
                // We want to copy from `src_logical` to `dst_logical` for `len` items.
                // We iterate? No, bulk copy.
                // Since it's ring buffer, we might have 1 or 2 chunks.
                // It's effectively `copy_overlapping` (memmove) logic but with wrap.

                // Simplified: use a helper for logical range copy.
                // But logical indices are relative to `self.deque.head`.

                self.deque.copy_range(src_logical, dst_logical, len);
            }
        }

        self.deque.len -= count;
    }
}

impl<'brand, T> BrandedVecDeque<'brand, T> {
    /// Copies `len` elements from logical index `src` to logical index `dst`.
    /// Handles wrapping.
    unsafe fn copy_range(&mut self, src: usize, dst: usize, len: usize) {
        if len == 0 { return; }

        // Convert to physical indices
        let head = self.head;
        let cap = self.cap;
        let ptr = self.ptr.as_ptr();

        // We can't easily do a single copy if wrapped.
        // We effectively perform `ptr::copy` (memmove).
        // Since it's a ring buffer, src and dst ranges might wrap.
        // And they might overlap.

        // To handle overlap correctly with wrapping is tricky.
        // But `ptr::copy` handles overlap. We just need to handle wrapping.
        // A simple way is to copy element by element if we don't want to optimize yet?
        // No, we want performance.

        // Since we are inside `drain`, we know the gap size.
        // But `copy_range` is generic.

        // Let's implement element-wise copy for simplicity and correctness first?
        // Or handle the 4 cases of wrapping (src wrapped, dst wrapped, etc).

        // Actually, std::VecDeque implementation is complex.
        // Given constraints, maybe element-wise loop is safer than buggy memcpy logic for now,
        // and still O(N) (just higher constant).
        // But we want "optimizing performance".

        // Let's try to do it right.
        // We can construct slices for src range and dst range.
        // But `as_mut_slices` gives whole deque.
        // We want specific ranges.

        // We can do it in two passes max (contiguous or wrapped).
        // If src wraps, we have 2 src chunks.
        // If dst wraps, we have 2 dst chunks.
        // This is generic copy on ring buffer.

        // Let's implement `wrap_copy` properly.
        // We assume `src` and `dst` are logical indices relative to `head`.

        // But wait, `copy_range` is hard.
        // Let's look at `wrap_copy` in logic above:
        // `self.deque.wrap_copy(self.deque.head, self.deque.head + count, head_len);`
        // Here arguments are physical indices (modulo cap logic handled inside or outside?).
        // `self.deque.head` is physical. `count` is offset.
        // So `wrap_copy` should take physical indices (unwrapped) or handle wrap?
        // "logical index" usually means 0..len.

        // Let's assume `copy_range` takes logical indices 0..len.

        for i in 0..len {
            let s_idx = (head + src + i) % cap;
            let d_idx = (head + dst + i) % cap;
            let s_ptr = ptr.add(s_idx);
            let d_ptr = ptr.add(d_idx);
            ptr::copy(s_ptr, d_ptr, 1);
        }
    }

    // Helper for shifting head right
    unsafe fn wrap_copy(&mut self, src_physical: usize, dst_physical: usize, len: usize) {
         let cap = self.cap;
         let ptr = self.ptr.as_ptr();

         // This is used for shifting head.
         // We copy `len` items from `src_physical` to `dst_physical`.
         // Both indices might wrap conceptually, but `src_physical` is `head`.
         // `dst_physical` is `head + count`.

         // Use backward copy to be safe?
         // When shifting head right, we move 0->count, 1->count+1...
         // If we iterate forward, we overwrite if count < len.
         // We should use `ptr::copy` which handles overlap.

         for i in (0..len).rev() {
             let s_idx = (src_physical + i) % cap;
             let d_idx = (dst_physical + i) % cap;
             ptr::copy(ptr.add(s_idx), ptr.add(d_idx), 1);
         }
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

    #[test]
    fn branded_vec_deque_drain() {
        GhostToken::new(|mut token| {
            let mut dq = BrandedVecDeque::new();
            for i in 0..10 {
                dq.push_back(i);
            }

            // Drain middle [3, 4, 5, 6]
            let drained: Vec<_> = dq.drain(3..7).collect();
            assert_eq!(drained, vec![3, 4, 5, 6]);

            // Remaining: [0, 1, 2, 7, 8, 9]
            let remaining: Vec<_> = dq.iter(&token).copied().collect();
            assert_eq!(remaining, vec![0, 1, 2, 7, 8, 9]);
            assert_eq!(dq.len(), 6);
        });
    }

    #[test]
    fn branded_vec_deque_drain_wrap() {
        GhostToken::new(|mut token| {
            // Force wrap: cap 8
            let mut dq = BrandedVecDeque::with_capacity(8);
            for i in 0..5 { dq.push_back(i); } // [0,1,2,3,4]
            for _ in 0..2 { dq.pop_front(); } // [_,_,2,3,4] head=2
            for i in 5..8 { dq.push_back(i); } // [5,6,7,_,_,2,3,4] wrapped

            // Current logical: [2, 3, 4, 5, 6, 7]
            // Drain [3, 4, 5] -> indices 1..4 (exclusive)

            let drained: Vec<_> = dq.drain(1..4).collect();
            assert_eq!(drained, vec![3, 4, 5]);

            // Remaining: [2, 6, 7]
            let remaining: Vec<_> = dq.iter(&token).copied().collect();
            assert_eq!(remaining, vec![2, 6, 7]);
        });
    }
}
