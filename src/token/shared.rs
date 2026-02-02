//! `SharedGhostToken` — a scalable, thread-safe handle for ghost tokens.
//!
//! This primitive allows a `GhostToken` to be shared across multiple threads, enabling
//! concurrent read access to branded data structures (like `BrandedHashMap`) and controlled
//! exclusive write access.
//!
//! # Implementation: Scalable Striped RWLock
//!
//! This implementation uses a **striped counter** approach to minimize cache contention
//! during concurrent reads.
//!
//! - **State**:
//!   - `writer_active`: A global atomic flag indicating if a writer is active or waiting.
//!   - `shards`: A fixed-size array of `CachePadded<AtomicUsize>` counters.
//! - **Reader Logic**:
//!   - Threads map their ID to a shard index (cached in `thread_local`).
//!   - If `writer_active` is set, they backoff (spin then yield).
//!   - Otherwise, they increment their shard's counter.
//!   - They check `writer_active` *after* incrementing (Store-Load barrier).
//! - **Writer Logic**:
//!   - Sets `writer_active` (SeqCst).
//!   - Waits for the sum of *all* shard counters to be zero (with backoff).

use crate::concurrency::{CachePadded, SHARD_COUNT, current_shard_index};
use crate::concurrency::sync::{wait_on_u32, wake_all_u32};
use crate::token::traits::{GhostBorrow, GhostBorrowMut};
use crate::GhostToken;
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};

const WRITER_ACTIVE: u32 = 1;
const WRITER_INACTIVE: u32 = 0;

/// A thread-safe, shared handle to a ghost token with scalable read performance.
pub struct SharedGhostToken<'brand> {
    token: UnsafeCell<GhostToken<'brand>>,
    /// Global flag: true if a writer is active or pending.
    writer_active: AtomicU32,
    /// Striped reader counters, padded to avoid false sharing.
    shards: [CachePadded<AtomicU32>; SHARD_COUNT],
}

// SAFETY: Synchronization is handled internally.
unsafe impl<'brand> Sync for SharedGhostToken<'brand> {}
unsafe impl<'brand> Send for SharedGhostToken<'brand> {}

impl<'brand> SharedGhostToken<'brand> {
    /// Creates a new shared token handle.
    pub fn new(token: GhostToken<'brand>) -> Self {
        let shards = core::array::from_fn(|_| CachePadded::new(AtomicU32::new(0)));

        Self {
            token: UnsafeCell::new(token),
            writer_active: AtomicU32::new(WRITER_INACTIVE),
            shards,
        }
    }

    /// Acquires a shared read lock on the token.
    pub fn read<'a>(&'a self) -> SharedTokenReadGuard<'a, 'brand> {
        let shard_idx = current_shard_index();
        let shard = &self.shards[shard_idx];
        loop {
            if self.writer_active.load(Ordering::SeqCst) == WRITER_ACTIVE {
                wait_on_u32(&self.writer_active, WRITER_ACTIVE);
                continue;
            }

            shard.fetch_add(1, Ordering::SeqCst);

            if self.writer_active.load(Ordering::SeqCst) == WRITER_ACTIVE {
                let prev = shard.fetch_sub(1, Ordering::SeqCst);
                if prev == 1 {
                    wake_all_u32(shard);
                }
                continue;
            }

            return SharedTokenReadGuard {
                parent: self,
                shard_index: shard_idx,
            };
        }
    }

    /// Acquires an exclusive write lock on the token.
    pub fn write<'a>(&'a self) -> SharedTokenWriteGuard<'a, 'brand> {
        while self.writer_active.swap(WRITER_ACTIVE, Ordering::SeqCst) == WRITER_ACTIVE {
            wait_on_u32(&self.writer_active, WRITER_ACTIVE);
        }

        for shard in &self.shards {
            loop {
                let count = shard.load(Ordering::SeqCst);
                if count == 0 {
                    break;
                }
                wait_on_u32(shard, count);
            }
        }

        SharedTokenWriteGuard { parent: self }
    }
}

/// RAII guard for shared read access.
pub struct SharedTokenReadGuard<'a, 'brand> {
    parent: &'a SharedGhostToken<'brand>,
    shard_index: usize,
}

impl<'a, 'brand> Deref for SharedTokenReadGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.parent.token.get() }
    }
}

impl<'a, 'brand> Drop for SharedTokenReadGuard<'a, 'brand> {
    fn drop(&mut self) {
        let shard = &self.parent.shards[self.shard_index];
        let prev = shard.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            wake_all_u32(shard);
        }
    }
}

// Implement GhostBorrow for ReadGuard to allow use with GhostCell
impl<'a, 'brand> GhostBorrow<'brand> for SharedTokenReadGuard<'a, 'brand> {}

/// RAII guard for exclusive write access.
pub struct SharedTokenWriteGuard<'a, 'brand> {
    parent: &'a SharedGhostToken<'brand>,
}

impl<'a, 'brand> Deref for SharedTokenWriteGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.parent.token.get() }
    }
}

impl<'a, 'brand> DerefMut for SharedTokenWriteGuard<'a, 'brand> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.parent.token.get() }
    }
}

impl<'a, 'brand> Drop for SharedTokenWriteGuard<'a, 'brand> {
    fn drop(&mut self) {
        self.parent
            .writer_active
            .store(WRITER_INACTIVE, Ordering::SeqCst);
        wake_all_u32(&self.parent.writer_active);
    }
}

// Implement GhostBorrow and GhostBorrowMut for WriteGuard
impl<'a, 'brand> GhostBorrow<'brand> for SharedTokenWriteGuard<'a, 'brand> {}
impl<'a, 'brand> GhostBorrowMut<'brand> for SharedTokenWriteGuard<'a, 'brand> {}
