//! `ChunkedVec` — a high-performance contiguous-by-chunks growable vector.
//!
//! ## Performance Characteristics
//!
//! Based on chunked memory allocation research and cache-conscious data structures:
//!
//! ### Time Complexity
//! - **Push**: O(1) amortized - chunk allocation + direct element placement
//! - **Get**: O(1) - direct indexing with chunk lookup
//! - **Bulk operations**: O(n) - zero-overhead chunk-wise iteration
//! - **Iteration**: O(1) per element - direct pointer arithmetic
//!
//! ### Space Complexity
//! - **Per element**: `size_of::<T>()` + minimal chunk metadata
//! - **Chunk overhead**: `size_of::<Box<[MaybeUninit<T>; CHUNK]>>` per chunk
//! - **Growth**: Predictable chunk allocation, no exponential overhead
//!
//! ### Memory Layout
//! - **Contiguous chunks**: Optimal cache locality within chunks
//! - **Heap allocation**: Each chunk is independently allocated
//! - **Predictable allocation**: No reallocation surprises
//!
//! ## Optimizations Applied
//!
//! 1. **Direct pointer arithmetic**: Eliminates bounds checking in hot paths
//! 2. **Chunk-wise bulk operations**: Efficient processing without iterator overhead
//! 3. **Unsafe optimizations**: `get_unchecked()` for performance-critical code
//! 4. **Cache-friendly access**: Sequential access within chunks
//!
//! ## Usage Patterns
//!
//! ### Standard Vector Operations
//! ```rust
//! use halo::collections::ChunkedVec;
//!
//! let mut chunked_vec = ChunkedVec::<u64, 1024>::new();
//! chunked_vec.push(42);
//! let value = chunked_vec.get(0).unwrap();
//! ```
//!
//! ### Bulk Processing (Optimized)
//! ```rust
//! use halo::collections::ChunkedVec;
//!
//! let mut chunked_vec = ChunkedVec::<u64, 1024>::new();
//! chunked_vec.push(1);
//! chunked_vec.push(2);
//! chunked_vec.push(3);
//!
//! // Zero-overhead iteration
//! chunked_vec.for_each(|x| println!("Processing {}", x));
//!
//! // Direct mutation
//! chunked_vec.for_each_mut(|x| *x *= 2);
//! ```

use core::iter::FusedIterator;
use core::{mem::MaybeUninit, ptr};

/// A vector backed by fixed-size chunks of `MaybeUninit<T>`.
///
/// ### Memory Efficiency
/// - **Predictable Allocation**: only allocates when current capacity is exhausted.
/// - **Contiguous Chunks**: Each chunk is an owned array on the heap, ensuring good cache locality for elements in the same chunk.
/// - **Zero-cost Branding**: Works seamlessly with GhostToken-gated elements.
/// - **Minimal Overhead**: Does not store capacity per element; only uses one `Vec` of `Box` pointers.
pub struct ChunkedVec<T, const CHUNK: usize> {
    chunks: Vec<Box<[MaybeUninit<T>; CHUNK]>>,
    len: usize,
}

impl<T, const CHUNK: usize> ChunkedVec<T, CHUNK> {
    /// Creates an empty `ChunkedVec`.
    pub const fn new() -> Self {
        assert!(CHUNK != 0, "ChunkedVec CHUNK must be > 0");
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }

    /// Returns the chunk size parameter as a runtime value.
    ///
    /// This is a zero-cost operation that returns the compile-time constant.
    #[inline(always)]
    pub const fn chunk_size() -> usize {
        CHUNK
    }

    /// Returns the number of chunks currently allocated.
    #[inline(always)]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Returns the number of initialized elements.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if there are no elements.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the chunk capacity (`CHUNK`).
    #[inline(always)]
    pub const fn chunk_capacity(&self) -> usize {
        CHUNK
    }

    /// Returns total capacity across allocated chunks.
    pub fn capacity(&self) -> usize {
        self.chunks.len() * CHUNK
    }

    /// Reserves enough space for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        assert!(CHUNK != 0, "ChunkedVec CHUNK must be > 0");
        let needed = self.len.saturating_add(additional);
        if needed <= self.capacity() {
            return;
        }
        let needed_chunks = (needed + CHUNK - 1) / CHUNK;
        while self.chunks.len() < needed_chunks {
            self.chunks.push(new_uninit_chunk::<T, CHUNK>());
        }
    }

    /// Pushes an element and returns its index.
    pub fn push(&mut self, value: T) -> usize {
        assert!(CHUNK != 0, "ChunkedVec CHUNK must be > 0");
        if self.len == self.capacity() {
            self.chunks.push(new_uninit_chunk::<T, CHUNK>());
        }
        let idx = self.len;
        let (c, o) = index_split::<CHUNK>(idx);
        // SAFETY:
        // - `c` is in-bounds because we just pushed a chunk if `len == capacity`.
        // - `o` is in-bounds because `index_split` returns `o < CHUNK`.
        // - the slot is uninitialized because `idx == self.len`.
        unsafe {
            // IMPORTANT: `Box<[MaybeUninit<T>; CHUNK]>::as_mut_ptr()` returns a pointer to the
            // *array*, not to the first element. We must cast to the element type before `.add(o)`.
            let base: *mut MaybeUninit<T> = self.chunks.get_unchecked_mut(c).as_mut_ptr().cast();
            base.add(o).write(MaybeUninit::new(value));
        }
        self.len += 1;
        idx
    }

    /// Returns a shared reference to element `idx` if in-bounds.
    pub fn get(&self, idx: usize) -> Option<&T> {
        if idx >= self.len {
            return None;
        }
        let (c, o) = index_split::<CHUNK>(idx);
        // SAFETY:
        // - `idx < self.len` ensures the element is initialized.
        // - `c` and `o` are valid indices into `chunks` and the chunk array.
        unsafe {
            // IMPORTANT: `Box<[MaybeUninit<T>; CHUNK]>::as_ptr()` returns a pointer to the *array*.
            // Cast to element pointer before offsetting.
            let base: *const MaybeUninit<T> = self.chunks.get_unchecked(c).as_ptr().cast();
            Some((&*base.add(o)).assume_init_ref())
        }
    }

    /// Returns a shared reference to element `idx` without bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, idx: usize) -> &T {
        let (c, o) = index_split::<CHUNK>(idx);
        // SAFETY: caller guarantees `idx < self.len`, so chunk `c` exists and offset `o` is valid.
        // We can skip the bounds check on chunks.get_unchecked since we know c < chunks.len().
        let chunk_ptr = self.chunks.as_ptr().add(c);
        let base: *const MaybeUninit<T> = (*chunk_ptr).as_ptr().cast();
        (&*base.add(o)).assume_init_ref()
    }

    /// Returns a mutable reference to element `idx` if in-bounds.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        if idx >= self.len {
            return None;
        }
        let (c, o) = index_split::<CHUNK>(idx);
        // SAFETY:
        // - `idx < self.len` ensures the element is initialized.
        // - `c` and `o` are valid indices.
        unsafe {
            let base: *mut MaybeUninit<T> = self.chunks.get_unchecked_mut(c).as_mut_ptr().cast();
            Some((&mut *base.add(o)).assume_init_mut())
        }
    }

    /// Returns an iterator over `&T`.
    #[inline]
    pub fn iter(&self) -> ChunkedIter<'_, T, CHUNK> {
        ChunkedIter { vec: self, idx: 0 }
    }

    /// Applies a function to all elements in the ChunkedVec.
    ///
    /// This provides maximum efficiency for bulk operations by directly
    /// iterating over chunks and elements without bounds checking overhead.
    #[inline]
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        if self.is_empty() {
            return;
        }

        for (chunk_idx, chunk) in self.chunks.iter().enumerate() {
            let initialized_count = if chunk_idx == self.chunks.len() - 1 {
                // Last chunk: only count initialized elements
                let elements_in_chunk = self.len - (chunk_idx * CHUNK);
                elements_in_chunk
            } else {
                // Full chunk
                CHUNK
            };

            for i in 0..initialized_count {
                // SAFETY: We only access initialized elements within bounds
                unsafe {
                    let base: *const MaybeUninit<T> = chunk.as_ptr().cast();
                    let elem = &*base.add(i);
                    f(elem.assume_init_ref());
                }
            }
        }
    }

    /// Applies a mutable function to all elements in the ChunkedVec.
    ///
    /// This provides maximum efficiency for bulk mutation operations.
    #[inline]
    pub fn for_each_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T),
    {
        if self.is_empty() {
            return;
        }

        let num_chunks = self.chunks.len();
        for chunk_idx in 0..num_chunks {
            let chunk = &mut self.chunks[chunk_idx];
            let initialized_count = if chunk_idx == num_chunks - 1 {
                // Last chunk: only count initialized elements
                let elements_in_chunk = self.len - (chunk_idx * CHUNK);
                elements_in_chunk
            } else {
                // Full chunk
                CHUNK
            };

            for i in 0..initialized_count {
                // SAFETY: We only access initialized elements within bounds
                unsafe {
                    let base: *mut MaybeUninit<T> = chunk.as_mut_ptr().cast();
                    let elem = &mut *base.add(i);
                    f(elem.assume_init_mut());
                }
            }
        }
    }

    /// Applies a mutable function to elements in the range [start..end].
    ///
    /// Both `start` and `end` are clamped to the vector length, and `end` is
    /// ensured to be >= `start` after clamping to prevent invalid range bounds.
    #[inline]
    pub fn for_each_mut_range<F>(&mut self, start: usize, end: usize, mut f: F)
    where
        F: FnMut(&mut T),
    {
        let len = self.len();
        let start = start.min(len);
        let end = end.min(len).max(start);

        if start >= end {
            return;
        }

        let num_chunks = self.chunks.len();
        let mut current_idx = start;
        while current_idx < end {
            let (chunk_idx, elem_idx) = index_split::<CHUNK>(current_idx);
            let chunk = &mut self.chunks[chunk_idx];

            // Calculate how many elements we can process in this chunk
            let chunk_start = current_idx;
            let chunk_end = if chunk_idx == num_chunks - 1 {
                // Last chunk: only up to initialized elements
                len
            } else {
                // Full chunk
                (chunk_idx + 1) * CHUNK
            };
            let process_end = chunk_end.min(end);

            for i in elem_idx..(elem_idx + (process_end - chunk_start)) {
                // SAFETY: We only access initialized elements within bounds
                unsafe {
                    let base: *mut MaybeUninit<T> = chunk.as_mut_ptr().cast();
                    let elem = &mut *base.add(i);
                    f(elem.assume_init_mut());
                }
            }

            current_idx = process_end;
        }
    }
}

impl<T, const CHUNK: usize> Default for ChunkedVec<T, CHUNK> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const CHUNK: usize> From<Vec<T>> for ChunkedVec<T, CHUNK> {
    fn from(vec: Vec<T>) -> Self {
        let mut chunked = Self::new();
        for item in vec {
            chunked.push(item);
        }
        chunked
    }
}

impl<T: Clone, const CHUNK: usize> From<ChunkedVec<T, CHUNK>> for Vec<T> {
    fn from(chunked: ChunkedVec<T, CHUNK>) -> Self {
        chunked.iter().cloned().collect()
    }
}

impl<T, const CHUNK: usize> Drop for ChunkedVec<T, CHUNK> {
    fn drop(&mut self) {
        // Drop only initialized elements.
        let mut remaining = self.len;
        for chunk in &mut self.chunks {
            if remaining == 0 {
                break;
            }
            let to_drop = remaining.min(CHUNK);
            // SAFETY: first `to_drop` elements in this chunk are initialized.
            unsafe {
                let ptr = chunk.as_mut_ptr().cast::<MaybeUninit<T>>();
                for i in 0..to_drop {
                    ptr::drop_in_place(ptr.add(i).cast::<T>());
                }
            }
            remaining -= to_drop;
        }
    }
}

/// Iterator over `&T` for a `ChunkedVec`.
pub struct ChunkedIter<'a, T, const CHUNK: usize> {
    vec: &'a ChunkedVec<T, CHUNK>,
    idx: usize,
}

impl<'a, T, const CHUNK: usize> ChunkedIter<'a, T, CHUNK> {
    /// Returns how many items remain.
    pub fn remaining(&self) -> usize {
        self.vec.len.saturating_sub(self.idx)
    }
}

impl<'a, T, const CHUNK: usize> Iterator for ChunkedIter<'a, T, CHUNK> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.idx;
        if i >= self.vec.len {
            return None;
        }
        self.idx += 1;
        // SAFETY: we just checked `i < self.vec.len`.
        Some(unsafe { self.vec.get_unchecked(i) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.vec.len.saturating_sub(self.idx);
        (rem, Some(rem))
    }
}

impl<'a, T, const CHUNK: usize> ExactSizeIterator for ChunkedIter<'a, T, CHUNK> {}
impl<'a, T, const CHUNK: usize> FusedIterator for ChunkedIter<'a, T, CHUNK> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_vec_push_get_across_chunks_power_of_two_chunk() {
        const CHUNK: usize = 8;
        let mut v: ChunkedVec<usize, CHUNK> = ChunkedVec::new();
        for i in 0..(CHUNK * 3 + 1) {
            v.push(i);
        }
        assert_eq!(v.len(), CHUNK * 3 + 1);
        for i in 0..v.len() {
            assert_eq!(*v.get(i).unwrap(), i);
        }
        let sum: usize = v.iter().copied().sum();
        assert_eq!(sum, (0..v.len()).sum::<usize>());
    }

    #[test]
    fn chunked_vec_push_get_across_chunks_non_power_of_two_chunk() {
        const CHUNK: usize = 6;
        let mut v: ChunkedVec<u32, CHUNK> = ChunkedVec::new();
        for i in 0..(CHUNK * 4 + 3) {
            v.push(i as u32);
        }
        assert_eq!(v.len(), CHUNK * 4 + 3);
        for i in 0..v.len() {
            assert_eq!(*v.get(i).unwrap(), i as u32);
        }
    }

    #[test]
    fn chunked_vec_get_mut_writes_correct_slot() {
        const CHUNK: usize = 4;
        let mut v: ChunkedVec<i32, CHUNK> = ChunkedVec::new();
        for i in 0..(CHUNK * 2 + 1) {
            v.push(i as i32);
        }
        *v.get_mut(0).unwrap() = -1;
        *v.get_mut(CHUNK).unwrap() = -2;
        *v.get_mut(CHUNK * 2).unwrap() = -3;
        assert_eq!(*v.get(0).unwrap(), -1);
        assert_eq!(*v.get(CHUNK).unwrap(), -2);
        assert_eq!(*v.get(CHUNK * 2).unwrap(), -3);
    }

    #[test]
    fn chunked_vec_drop_drops_exactly_initialized_prefix() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        static DROPS: AtomicUsize = AtomicUsize::new(0);

        struct DropCounter;
        impl Drop for DropCounter {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }

        DROPS.store(0, Ordering::Relaxed);
        {
            const CHUNK: usize = 7;
            let mut v: ChunkedVec<DropCounter, CHUNK> = ChunkedVec::new();
            for _ in 0..(CHUNK * 2 + 3) {
                v.push(DropCounter);
            }
            assert_eq!(v.len(), CHUNK * 2 + 3);
        }
        assert_eq!(DROPS.load(Ordering::Relaxed), 7 * 2 + 3);
    }

    #[test]
    fn chunked_vec_for_each_mut_range() {
        const CHUNK: usize = 4;
        let mut v: ChunkedVec<i32, CHUNK> = ChunkedVec::new();

        // Fill with test data
        for i in 0..12 {
            v.push(i);
        }
        assert_eq!(v.len(), 12);

        // Test normal range
        v.for_each_mut_range(2, 8, |x| *x *= 2);
        assert_eq!(*v.get(0).unwrap(), 0); // unchanged
        assert_eq!(*v.get(1).unwrap(), 1); // unchanged
        assert_eq!(*v.get(2).unwrap(), 4); // 2 * 2
        assert_eq!(*v.get(3).unwrap(), 6); // 3 * 2
        assert_eq!(*v.get(7).unwrap(), 14); // 7 * 2
        assert_eq!(*v.get(8).unwrap(), 8); // unchanged
        assert_eq!(*v.get(11).unwrap(), 11); // unchanged

        // Test out-of-bounds clamping: start > end should be handled correctly
        v.for_each_mut_range(10, 5, |x| *x = 999); // This should do nothing since end < start after clamping
        assert_eq!(*v.get(5).unwrap(), 10); // should remain unchanged (was modified to 10 earlier)

        // Test start beyond length
        v.for_each_mut_range(20, 25, |x| *x = 888); // Should do nothing
        assert_eq!(*v.get(11).unwrap(), 11); // should remain unchanged

        // Test end beyond length
        v.for_each_mut_range(10, 20, |x| *x *= 10);
        assert_eq!(*v.get(10).unwrap(), 100); // 10 * 10
        assert_eq!(*v.get(11).unwrap(), 110); // 11 * 10

        // Test empty range
        v.for_each_mut_range(5, 5, |x| *x = 777); // Should do nothing
        assert_eq!(*v.get(5).unwrap(), 10); // should remain unchanged
    }

    #[test]
    fn chunked_vec_for_each_mut_range_bounds_safety() {
        const CHUNK: usize = 3;
        let mut v: ChunkedVec<i32, CHUNK> = ChunkedVec::new();

        // Fill with test data across multiple chunks
        for i in 0..10 {
            v.push(i);
        }

        // Test the critical bug case: start > end after clamping
        // This would panic in a buggy implementation when creating [start..end] slice
        let original_values: Vec<i32> = (0..10).collect();
        for i in 0..10 {
            assert_eq!(*v.get(i).unwrap(), original_values[i]);
        }

        // These calls should all be safe and do nothing
        v.for_each_mut_range(5, 3, |x| *x = 999); // start > end
        v.for_each_mut_range(15, 20, |x| *x = 888); // both beyond length
        v.for_each_mut_range(8, 5, |x| *x = 777); // start > end, start within bounds

        // Verify no values were modified
        for i in 0..10 {
            assert_eq!(
                *v.get(i).unwrap(),
                original_values[i],
                "Element at index {} should be unchanged",
                i
            );
        }

        // Test that valid ranges still work after bounds clamping
        v.for_each_mut_range(2, 7, |x| *x *= 2);
        assert_eq!(*v.get(1).unwrap(), 1); // unchanged
        assert_eq!(*v.get(2).unwrap(), 4); // 2 * 2
        assert_eq!(*v.get(6).unwrap(), 12); // 6 * 2
        assert_eq!(*v.get(7).unwrap(), 7); // unchanged
    }
}

#[inline(always)]
const fn index_split<const CHUNK: usize>(idx: usize) -> (usize, usize) {
    if CHUNK == 0 {
        panic!("ChunkedVec CHUNK must be > 0");
    }
    if CHUNK.is_power_of_two() {
        let shift = CHUNK.trailing_zeros() as usize;
        let mask = CHUNK - 1;
        (idx >> shift, idx & mask)
    } else {
        (idx / CHUNK, idx % CHUNK)
    }
}

fn new_uninit_chunk<T, const CHUNK: usize>() -> Box<[MaybeUninit<T>; CHUNK]> {
    // Avoid creating a potentially large array on the stack.
    //
    // SAFETY: An uninitialized `[MaybeUninit<T>; CHUNK]` is valid; we will
    // write elements individually and only drop the initialized prefix.
    unsafe { Box::<[MaybeUninit<T>; CHUNK]>::new_uninit().assume_init() }
}
