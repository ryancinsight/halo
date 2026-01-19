//! Branded visited sets for graph traversals.
//!
//! This provides two internal implementations:
//! - `VisitedSet`: word-packed atomics (`GhostAtomicBitset`) for fixed-size graphs
//! - `VisitedFlags`: per-node atomics (`GhostAtomicBool`) for dynamically-sized graphs
//!
//! The goal is to keep graph algorithms expressing visited logic in one place,
//! while allowing data-structure-specific storage choices.

use core::sync::atomic::Ordering;

use crate::concurrency::atomic::{GhostAtomicBitset, GhostAtomicBool};

/// A dense, word-packed visited set for fixed-size graphs.
pub(crate) struct VisitedSet<'brand> {
    bits: GhostAtomicBitset<'brand>,
}

impl<'brand> VisitedSet<'brand> {
    #[inline(always)]
    pub(crate) fn new(bits: usize) -> Self {
        Self {
            bits: GhostAtomicBitset::new(bits),
        }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.bits.len_bits()
    }

    #[inline(always)]
    pub(crate) fn clear(&self) {
        self.bits.clear_all()
    }

    /// Returns `true` iff this call observed the node as not-yet-visited and marks it visited.
    #[inline(always)]
    pub(crate) fn try_visit(&self, node: usize, order: Ordering) -> bool {
        self.bits.test_and_set(node, order)
    }

    /// Like `try_visit`, but without bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `node < self.len()`.
    #[inline(always)]
    pub(crate) unsafe fn try_visit_unchecked(&self, node: usize, order: Ordering) -> bool {
        // SAFETY: caller proves bounds.
        unsafe { self.bits.test_and_set_unchecked(node, order) }
    }

    #[inline(always)]
    pub(crate) fn is_visited(&self, node: usize) -> bool {
        self.bits.is_set(node)
    }
}

/// A per-node visited flag vector for dynamically-sized graphs.
pub(crate) struct VisitedFlags<'brand> {
    flags: Vec<GhostAtomicBool<'brand>>,
}

impl<'brand> VisitedFlags<'brand> {
    pub(crate) fn new(len: usize) -> Self {
        let flags = (0..len).map(|_| GhostAtomicBool::new(false)).collect();
        Self { flags }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.flags.len()
    }

    pub(crate) fn push(&mut self) {
        self.flags.push(GhostAtomicBool::new(false));
    }

    pub(crate) fn remove(&mut self, idx: usize) {
        self.flags.remove(idx);
    }

    #[inline]
    pub(crate) fn clear(&self, order: Ordering) {
        for f in &self.flags {
            f.store(false, order);
        }
    }

    #[inline(always)]
    pub(crate) fn is_visited(&self, idx: usize, order: Ordering) -> bool {
        self.flags[idx].load(order)
    }

    #[inline(always)]
    pub(crate) fn mark(&self, idx: usize, order: Ordering) {
        self.flags[idx].store(true, order);
    }

    #[inline(always)]
    pub(crate) fn try_visit(&self, idx: usize, order: Ordering) -> bool {
        self.flags[idx]
            .compare_exchange(false, true, order, Ordering::Relaxed)
            .is_ok()
    }
}
