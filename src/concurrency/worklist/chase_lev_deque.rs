//! A fixed-capacity Chase–Lev work-stealing deque (indices-only).
//!
//! Properties:
//! - Single owner: `push_bottom` / `pop_bottom`
//! - Multiple stealers: `steal`
//! - Fixed capacity, power-of-two ring buffer
//!
//! This implementation stores only `usize` items and uses atomics for the buffer
//! as well as `top`/`bottom` to avoid UB from concurrent reads.

use core::sync::atomic::{fence, Ordering};

use crate::concurrency::atomic::GhostAtomicUsize;
use crate::token::{GhostBorrowMut, ImmutableChild};

use super::treiber_stack::NONE;

/// A fixed-capacity Chase–Lev deque for indices.
pub struct GhostChaseLevDeque<'brand> {
    top: GhostAtomicUsize<'brand>,
    bottom: GhostAtomicUsize<'brand>,
    buf: Vec<GhostAtomicUsize<'brand>>,
    mask: usize,
}

impl<'brand> GhostChaseLevDeque<'brand> {
    /// Creates a new deque with `capacity` entries.
    ///
    /// `capacity` must be a power of two.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        assert!(capacity != 0);
        let buf = (0..capacity).map(|_| GhostAtomicUsize::new(NONE)).collect();
        Self {
            top: GhostAtomicUsize::new(0),
            bottom: GhostAtomicUsize::new(0),
            buf,
            mask: capacity - 1,
        }
    }

    /// Clears the deque (logical reset).
    #[inline]
    pub fn clear<T: GhostBorrowMut<'brand>>(&self, token: &T) {
        let _ = token;
        self.top.store(0, Ordering::Relaxed);
        self.bottom.store(0, Ordering::Relaxed);
    }

    /// Attempts to push `x` to the bottom. Owner-only.
    ///
    /// Returns `false` if the deque is full.
    pub fn push_bottom<T: GhostBorrowMut<'brand>>(&self, token: &T, x: usize) -> bool {
        let _ = token;
        debug_assert!(x != NONE);
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Acquire);
        if b < t {
            // Should be impossible for monotone counters; treat as full/invalid.
            return false;
        }
        if b - t >= self.buf.len() {
            return false;
        }
        self.buf[b & self.mask].store(x, Ordering::Relaxed);
        // Publish the element before making it stealable via `bottom`.
        fence(Ordering::Release);
        self.bottom.store(b + 1, Ordering::Release);
        true
    }

    /// Attempts to pop from the bottom. Owner-only.
    pub fn pop_bottom<T: GhostBorrowMut<'brand>>(&self, token: &T) -> Option<usize> {
        let _ = token;
        // Load bottom first; if empty, avoid underflow.
        let b = self.bottom.load(Ordering::Relaxed);
        let t0 = self.top.load(Ordering::Acquire);
        if b <= t0 {
            return None;
        }

        let b1 = b - 1;
        self.bottom.store(b1, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        let t = self.top.load(Ordering::Acquire);
        if t > b1 {
            // Lost a race; restore.
            self.bottom.store(b, Ordering::Relaxed);
            return None;
        }

        let x = self.buf[b1 & self.mask].load(Ordering::Relaxed);
        if t == b1 {
            // Last element: race with stealers.
            if self
                .top
                .compare_exchange(t, t + 1, Ordering::SeqCst, Ordering::Relaxed)
                .is_err()
            {
                self.bottom.store(b, Ordering::Relaxed);
                return None;
            }
            // Restore bottom to match the new top.
            self.bottom.store(b, Ordering::Relaxed);
        }
        Some(x)
    }

    /// Attempts to steal from the top. Multi-stealer.
    pub fn steal<'a>(&self, token: &ImmutableChild<'a, 'brand>) -> Option<usize> {
        let _ = token;
        loop {
            let t = self.top.load(Ordering::Acquire);
            fence(Ordering::SeqCst);
            let b = self.bottom.load(Ordering::Acquire);
            if t >= b {
                return None;
            }
            let x = self.buf[t & self.mask].load(Ordering::Relaxed);
            if self
                .top
                .compare_exchange(t, t + 1, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return Some(x);
            }
        }
    }
}
