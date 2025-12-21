//! A lock-free Treiber stack for node indices.
//!
//! This is a classic MPMC stack:
//! - `head` is an atomic index (or `NONE`)
//! - `next[i]` is the atomic next pointer for node `i`
//!
//! Safety model:
//! - This implementation stores only indices, not references.
//! - Correctness relies on the caller ensuring each index is pushed at most once
//!   concurrently, or otherwise providing a safe reclamation strategy. For our
//!   intended graph traversal use (visited bitmap ensures single push), that holds.

use core::sync::atomic::Ordering;

use crate::concurrency::atomic::GhostAtomicUsize;

/// Sentinel for an empty stack / null next pointer.
pub const NONE: usize = usize::MAX;

/// A branded lock-free stack of indices `0..capacity`.
pub struct GhostTreiberStack<'brand> {
    head: GhostAtomicUsize<'brand>,
    next: Vec<GhostAtomicUsize<'brand>>,
}

impl<'brand> GhostTreiberStack<'brand> {
    /// Creates an empty stack with a fixed `capacity`.
    pub fn new(capacity: usize) -> Self {
        let next = (0..capacity).map(|_| GhostAtomicUsize::new(NONE)).collect();
        Self {
            head: GhostAtomicUsize::new(NONE),
            next,
        }
    }

    /// Clears the stack (does not clear `next` for all nodes; push overwrites it).
    #[inline]
    pub fn clear(&self) {
        self.head.store(NONE, Ordering::Relaxed);
    }

    /// Pushes `idx` onto the stack.
    ///
    /// # Panics
    /// Panics if `idx >= capacity`.
    #[inline]
    pub fn push(&self, idx: usize) {
        assert!(idx < self.next.len());
        loop {
            let h = self.head.load(Ordering::Acquire);
            self.next[idx].store(h, Ordering::Relaxed);
            if self
                .head
                .compare_exchange(h, idx, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Pushes a batch of indices onto the stack using a single CAS (amortized).
    ///
    /// The stack will contain all `batch` items (in the same relative order as
    /// provided), but the exact interleaving with other threads is unspecified.
    ///
    /// # Panics
    /// Panics if any index is out of bounds.
    ///
    /// # Correctness requirement
    /// This stack stores only indices, so the caller must ensure each index is not
    /// concurrently pushed multiple times without a reclamation/ABA strategy.
    /// In our graph traversal usage, `visited` guarantees single push per node.
    pub fn push_batch(&self, batch: &[usize]) {
        if batch.is_empty() {
            return;
        }
        for &idx in batch {
            assert!(idx < self.next.len());
        }

        // Link the internal list once (excluding the tail->old_head link which is set per-attempt).
        for w in batch.windows(2) {
            self.next[w[0]].store(w[1], Ordering::Relaxed);
        }

        let head_idx = batch[0];
        let tail_idx = *batch.last().unwrap();

        loop {
            let old = self.head.load(Ordering::Acquire);
            self.next[tail_idx].store(old, Ordering::Relaxed);
            if self
                .head
                .compare_exchange(old, head_idx, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Pops an index, if any.
    #[inline]
    pub fn pop(&self) -> Option<usize> {
        loop {
            let h = self.head.load(Ordering::Acquire);
            if h == NONE {
                return None;
            }
            let n = self.next[h].load(Ordering::Relaxed);
            if self
                .head
                .compare_exchange(h, n, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(h);
            }
        }
    }
}


