//! `SharedGhostToken` — a thread-safe, reference-counted handle for ghost tokens.
//!
//! This primitive allows a `GhostToken` to be shared across multiple threads, enabling
//! concurrent read access to branded data structures (like `BrandedHashMap`) and controlled
//! exclusive write access.
//!
//! # implementation
//!
//! This implementation uses a custom **atomic spin-lock** instead of `std::sync::RwLock` to
//! avoid heavy operating system mutexes. It is optimized for high-contention read scenarios
//! and short critical sections.
//!
//! - **State**: An `AtomicUsize` tracks the number of readers and the writer status.
//! - **Reader-Writer Logic**:
//!   - Multiple readers can acquire the lock if no writer is active.
//!   - Only one writer can acquire the lock, waiting for all readers to exit.
//!   - Writers have priority (implementation detail: writers block new readers).

use crate::GhostToken;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::hint::spin_loop;

const WRITER_BIT: usize = 1 << (usize::BITS - 1);
const READER_MASK: usize = !WRITER_BIT;

/// A thread-safe, shared handle to a ghost token.
///
/// This struct wraps a `GhostToken` in a custom atomic lock, allowing it to be shared
/// (e.g., via `Arc`) across threads.
pub struct SharedGhostToken<'brand> {
    /// The token is wrapped in UnsafeCell because we need to vend `&mut` references
    /// to it when we have the write lock, even though `SharedGhostToken` is shared (`&self`).
    token: UnsafeCell<GhostToken<'brand>>,
    /// Lock state: MSB is writer flag, remaining bits are reader count.
    state: AtomicUsize,
}

// SAFETY: SharedGhostToken manages synchronization internally.
// Access to the inner `GhostToken` is guarded by the atomic state.
unsafe impl<'brand> Sync for SharedGhostToken<'brand> {}
unsafe impl<'brand> Send for SharedGhostToken<'brand> {}

impl<'brand> SharedGhostToken<'brand> {
    /// Creates a new shared token handle.
    ///
    /// Consumes the unique `GhostToken` to ensure exclusive control is transferred to this handle.
    pub fn new(token: GhostToken<'brand>) -> Self {
        Self {
            token: UnsafeCell::new(token),
            state: AtomicUsize::new(0),
        }
    }

    /// Acquires a shared read lock on the token.
    ///
    /// Returns a guard that dereferences to `&GhostToken<'brand>`.
    /// Spins if a writer is currently holding the lock.
    pub fn read<'a>(&'a self) -> SharedTokenReadGuard<'a, 'brand> {
        loop {
            let state = self.state.load(Ordering::Relaxed);
            if state & WRITER_BIT != 0 {
                spin_loop();
                continue;
            }

            // Try to increment reader count
            if self.state.compare_exchange_weak(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                return SharedTokenReadGuard { parent: self };
            }
        }
    }

    /// Acquires an exclusive write lock on the token.
    ///
    /// Returns a guard that dereferences to `&mut GhostToken<'brand>`.
    /// Spins until the lock can be acquired.
    pub fn write<'a>(&'a self) -> SharedTokenWriteGuard<'a, 'brand> {
        // Phase 1: Set the writer bit to block new readers.
        loop {
            let state = self.state.load(Ordering::Relaxed);
            if state & WRITER_BIT != 0 {
                spin_loop();
                continue;
            }

            if self.state.compare_exchange_weak(
                state,
                state | WRITER_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                break;
            }
        }

        // Phase 2: Wait for existing readers to drain.
        while self.state.load(Ordering::Acquire) & READER_MASK != 0 {
            spin_loop();
        }

        SharedTokenWriteGuard { parent: self }
    }
}

/// RAII guard for shared read access to a ghost token.
pub struct SharedTokenReadGuard<'a, 'brand> {
    parent: &'a SharedGhostToken<'brand>,
}

impl<'a, 'brand> Deref for SharedTokenReadGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We have incremented the reader count, ensuring no writer can exist.
        unsafe { &*self.parent.token.get() }
    }
}

impl<'a, 'brand> Drop for SharedTokenReadGuard<'a, 'brand> {
    fn drop(&mut self) {
        self.parent.state.fetch_sub(1, Ordering::Release);
    }
}

/// RAII guard for exclusive write access to a ghost token.
pub struct SharedTokenWriteGuard<'a, 'brand> {
    parent: &'a SharedGhostToken<'brand>,
}

impl<'a, 'brand> Deref for SharedTokenWriteGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We have set the writer bit and waited for readers to drain.
        // We have exclusive access.
        unsafe { &*self.parent.token.get() }
    }
}

impl<'a, 'brand> DerefMut for SharedTokenWriteGuard<'a, 'brand> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We have set the writer bit and waited for readers to drain.
        // We have exclusive access.
        unsafe { &mut *self.parent.token.get() }
    }
}

impl<'a, 'brand> Drop for SharedTokenWriteGuard<'a, 'brand> {
    fn drop(&mut self) {
        // Clear the writer bit.
        // Since we are the writer, and we blocked new readers, the state should just be WRITER_BIT.
        // However, we just mask it out to be safe and use Release ordering.
        self.parent.state.fetch_and(!WRITER_BIT, Ordering::Release);
    }
}
