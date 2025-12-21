//! `ChunkedVec` — a contiguous-by-chunks growable vector.
//!
//! Goals:
//! - predictable allocation behavior (allocate in fixed-size chunks)
//! - good cache locality within each chunk
//! - minimal per-element overhead (stores `T` in `MaybeUninit<T>`)
//!
//! This is a building block for graph/arena layouts (CSR edges, chunked DFS stacks, etc.).

use core::{
    mem::MaybeUninit,
    ptr,
};

/// A vector backed by fixed-size chunks of `MaybeUninit<T>`.
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

    /// Returns the number of initialized elements.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if there are no elements.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the chunk capacity (`CHUNK`).
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
        // SAFETY: chunk exists; offset in-bounds; slot uninitialized.
        unsafe {
            self.chunks
                .get_unchecked_mut(c)
                .as_mut_ptr()
                .add(o)
                .write(MaybeUninit::new(value));
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
        // SAFETY: idx < len => element initialized.
        unsafe {
            let slot = self.chunks.get_unchecked(c).as_ptr().add(o);
            Some((&*slot).assume_init_ref())
        }
    }

    /// Returns a shared reference to element `idx` without bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, idx: usize) -> &T {
        let (c, o) = index_split::<CHUNK>(idx);
        let slot = self.chunks.get_unchecked(c).as_ptr().add(o);
        (&*slot).assume_init_ref()
    }

    /// Returns a mutable reference to element `idx` if in-bounds.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        if idx >= self.len {
            return None;
        }
        let (c, o) = index_split::<CHUNK>(idx);
        // SAFETY: idx < len => element initialized.
        unsafe {
            let slot = self.chunks.get_unchecked_mut(c).as_mut_ptr().add(o);
            Some((&mut *slot).assume_init_mut())
        }
    }

    /// Returns an iterator over `&T`.
    pub fn iter(&self) -> ChunkedIter<'_, T, CHUNK> {
        ChunkedIter {
            vec: self,
            idx: 0,
        }
    }
}

impl<T, const CHUNK: usize> Default for ChunkedVec<T, CHUNK> {
    fn default() -> Self {
        Self::new()
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
        self.vec.get(i)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.vec.len.saturating_sub(self.idx);
        (rem, Some(rem))
    }
}

impl<'a, T, const CHUNK: usize> ExactSizeIterator for ChunkedIter<'a, T, CHUNK> {}

#[inline(always)]
fn index_split<const CHUNK: usize>(idx: usize) -> (usize, usize) {
    debug_assert!(CHUNK != 0);
    if CHUNK != 0 && CHUNK.is_power_of_two() {
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


