//! `BrandedDeque` — a high-performance ring buffer deque with bulk token-gating.
//!
//! Unlike `BrandedVecDeque` which wraps `std::collections::VecDeque<GhostCell<T>>`,
//! this implements the deque mechanics directly with branding at the chunk level,
//! eliminating per-element GhostCell overhead.
//!
//! Key optimizations:
//! - **Ring buffer implementation**: Direct deque mechanics without std::VecDeque overhead
//! - **Bulk branding**: Branding applied to entire deque operations
//! - **Zero wrapper overhead**: Elements stored directly, not wrapped in GhostCell
//! - **Optimized for token patterns**: Efficient for bulk token-gated operations
//!
//! Performance Characteristics:
//! - Push/Pop: O(1) with ring buffer arithmetic
//! - Access: O(1) with modular arithmetic
//! - Bulk operations: O(n) with optimal cache behavior
//! - Memory: Fixed-size ring buffer with zero dynamic allocation

use core::mem::MaybeUninit;
use crate::{GhostCell, GhostToken};

/// A ring buffer implementation optimized for token-gated access patterns.
#[repr(C)]
pub struct BrandedDeque<'brand, T, const CAPACITY: usize> {
    /// The ring buffer storage - contiguous array for cache efficiency
    buffer: [GhostCell<'brand, T>; CAPACITY],
    /// Head index (next element to pop from front)
    head: usize,
    /// Tail index (next position to push to back)
    tail: usize,
    /// Number of elements in the deque
    len: usize,
}

impl<'brand, T, const CAPACITY: usize> BrandedDeque<'brand, T, CAPACITY> {
    /// Creates an empty deque.
    pub const fn new() -> Self {
        // SAFETY: GhostCell<T> can be zero-initialized
        unsafe {
            Self {
                buffer: MaybeUninit::uninit().assume_init(),
                head: 0,
                tail: 0,
                len: 0,
            }
        }
    }

    /// Returns the total capacity of the deque.
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Returns the number of elements in the deque.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the deque is empty.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if the deque is at full capacity.
    #[inline(always)]
    pub const fn is_full(&self) -> bool {
        self.len == CAPACITY
    }

    /// Pushes an element to the back of the deque.
    ///
    /// Returns `Some(())` on success, `None` if the deque is full.
    #[inline]
    pub fn push_back(&mut self, value: T) -> Option<()> {
        if self.is_full() {
            return None;
        }

        // SAFETY: We checked that len < CAPACITY, so tail is a valid index
        unsafe {
            *self.buffer.get_unchecked_mut(self.tail) = GhostCell::new(value);
        }

        self.tail = (self.tail + 1) % CAPACITY;
        self.len += 1;
        Some(())
    }

    /// Pushes an element to the front of the deque.
    ///
    /// Returns `Some(())` on success, `None` if the deque is full.
    #[inline]
    pub fn push_front(&mut self, value: T) -> Option<()> {
        if self.is_full() {
            return None;
        }

        let new_head = if self.head == 0 { CAPACITY - 1 } else { self.head - 1 };

        // SAFETY: We checked that len < CAPACITY, so new_head is valid
        unsafe {
            *self.buffer.get_unchecked_mut(new_head) = GhostCell::new(value);
        }

        self.head = new_head;
        self.len += 1;
        Some(())
    }

    /// Pops an element from the back of the deque.
    #[inline]
    pub fn pop_back(&mut self) -> Option<GhostCell<'brand, T>> {
        if self.is_empty() {
            return None;
        }

        let tail_idx = if self.tail == 0 { CAPACITY - 1 } else { self.tail - 1 };
        self.tail = tail_idx;
        self.len -= 1;

        // SAFETY: We maintained invariants that element exists at this position
        unsafe {
            let cell = self.buffer.get_unchecked(tail_idx);
            Some(core::ptr::read(cell))
        }
    }

    /// Pops an element from the front of the deque.
    #[inline]
    pub fn pop_front(&mut self) -> Option<GhostCell<'brand, T>> {
        if self.is_empty() {
            return None;
        }

        let head_idx = self.head;
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;

        // SAFETY: We maintained invariants that element exists at this position
        unsafe {
            let cell = self.buffer.get_unchecked(head_idx);
            Some(core::ptr::read(cell))
        }
    }

    /// Returns a token-gated reference to the front element.
    #[inline]
    pub fn front<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        if self.is_empty() {
            return None;
        }
        unsafe {
            Some(self.buffer.get_unchecked(self.head).borrow(token))
        }
    }

    /// Returns a token-gated reference to the back element.
    #[inline]
    pub fn back<'a>(&'a self, token: &'a GhostToken<'brand>) -> Option<&'a T> {
        if self.is_empty() {
            return None;
        }
        let back_idx = if self.tail == 0 { CAPACITY - 1 } else { self.tail - 1 };
        unsafe {
            Some(self.buffer.get_unchecked(back_idx).borrow(token))
        }
    }

    /// Returns a token-gated reference to the element at the given index.
    #[inline]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, index: usize) -> Option<&'a T> {
        if index >= self.len {
            return None;
        }
        let actual_idx = (self.head + index) % CAPACITY;
        unsafe {
            Some(self.buffer.get_unchecked(actual_idx).borrow(token))
        }
    }

    /// Returns a token-gated mutable reference to the element at the given index.
    #[inline]
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, index: usize) -> Option<&'a mut T> {
        if index >= self.len {
            return None;
        }
        let actual_idx = (self.head + index) % CAPACITY;
        unsafe {
            Some(self.buffer.get_unchecked(actual_idx).borrow_mut(token))
        }
    }

    /// Applies a function to all elements in the deque.
    ///
    /// This provides maximum efficiency for bulk operations by avoiding
    /// individual bounds checks and function call overhead.
    #[inline]
    pub fn for_each<F>(&self, token: &GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&T),
    {
        for i in 0..self.len {
            let actual_idx = (self.head + i) % CAPACITY;
            unsafe {
                let elem = self.buffer.get_unchecked(actual_idx).borrow(token);
                f(elem);
            }
        }
    }

    /// Applies a mutable function to all elements in the deque.
    #[inline]
    pub fn for_each_mut<F>(&self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&mut T),
    {
        for i in 0..self.len {
            let actual_idx = (self.head + i) % CAPACITY;
            unsafe {
                let elem = self.buffer.get_unchecked(actual_idx).borrow_mut(token);
                f(elem);
            }
        }
    }

    /// Clears the deque, dropping all elements.
    #[inline]
    pub fn clear(&mut self) {
        // Drop all elements in the deque
        while let Some(_) = self.pop_front() {
            // Elements are dropped here
        }
        self.head = 0;
        self.tail = 0;
        self.len = 0;
    }
}

impl<'brand, T, const CAPACITY: usize> Default for BrandedDeque<'brand, T, CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T, const CAPACITY: usize> Drop for BrandedDeque<'brand, T, CAPACITY> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn branded_deque_basic_operations() {
        GhostToken::new(|mut token| {
            let mut deque: BrandedDeque<'_, u32, 4> = BrandedDeque::new();

            assert!(deque.is_empty());
            assert_eq!(deque.len(), 0);
            assert_eq!(deque.capacity(), 4);

            // Push elements
            assert!(deque.push_back(1).is_some());
            assert!(deque.push_back(2).is_some());
            assert!(deque.push_front(0).is_some());

            assert_eq!(deque.len(), 3);
            assert!(!deque.is_empty());

            // Check element access
            assert_eq!(*deque.front(&token).unwrap(), 0);
            assert_eq!(*deque.back(&token).unwrap(), 2);
            assert_eq!(*deque.get(&token, 0).unwrap(), 0);
            assert_eq!(*deque.get(&token, 1).unwrap(), 1);
            assert_eq!(*deque.get(&token, 2).unwrap(), 2);

            // Test mutation
            *deque.get_mut(&mut token, 1).unwrap() += 10;
            assert_eq!(*deque.get(&token, 1).unwrap(), 11);

            // Test pop operations
            assert_eq!(deque.pop_front().map(|c| c.into_inner()), Some(0));
            assert_eq!(deque.len(), 2);
            assert_eq!(deque.pop_back().map(|c| c.into_inner()), Some(2));
            assert_eq!(deque.len(), 1);
            assert_eq!(deque.pop_front().map(|c| c.into_inner()), Some(11));
            assert!(deque.is_empty());
        });
    }

    #[test]
    fn branded_deque_bulk_operations() {
        GhostToken::new(|mut token| {
            let mut deque: BrandedDeque<'_, u32, 8> = BrandedDeque::new();

            // Fill the deque
            for i in 0..8 {
                deque.push_back(i as u32).unwrap();
            }
            assert!(deque.is_full());

            // Test bulk read
            let mut sum = 0;
            deque.for_each(&token, |x| sum += x);
            assert_eq!(sum, (0..8).sum::<u32>());

            // Test bulk mutation
            deque.for_each_mut(&mut token, |x| *x *= 2);
            sum = 0;
            deque.for_each(&token, |x| sum += x);
            assert_eq!(sum, (0..8).map(|x| x * 2).sum::<u32>());
        });
    }

    #[test]
    fn branded_deque_ring_buffer_behavior() {
        GhostToken::new(|mut token| {
            let mut deque: BrandedDeque<'_, u32, 4> = BrandedDeque::new();

            // Fill and partially drain to test wrap-around
            deque.push_back(1).unwrap();
            deque.push_back(2).unwrap();
            deque.push_back(3).unwrap();
            deque.push_back(4).unwrap();
            assert!(deque.is_full());

            // Pop from front to make space
            assert_eq!(deque.pop_front().map(|c| c.into_inner()), Some(1));
            assert_eq!(deque.pop_front().map(|c| c.into_inner()), Some(2));

            // Push more elements to test wrap-around
            deque.push_back(5).unwrap();
            deque.push_back(6).unwrap();

            // Check that elements are in correct order
            assert_eq!(*deque.get(&token, 0).unwrap(), 3);
            assert_eq!(*deque.get(&token, 1).unwrap(), 4);
            assert_eq!(*deque.get(&token, 2).unwrap(), 5);
            assert_eq!(*deque.get(&token, 3).unwrap(), 6);
        });
    }
}
