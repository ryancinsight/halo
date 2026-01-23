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

use crate::concurrency::CachePadded;
use crate::GhostToken;
use std::cell::{Cell, UnsafeCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

/// Number of stripes for the reader counters.
/// Power of two to allow cheap modulo masking.
const SHARD_COUNT: usize = 32;
const SHARD_MASK: usize = SHARD_COUNT - 1;

thread_local! {
    /// Caches the shard index for the current thread to avoid re-hashing.
    static THREAD_SHARD_INDEX: Cell<Option<usize>> = const { Cell::new(None) };
}

/// A thread-safe, shared handle to a ghost token with scalable read performance.
pub struct SharedGhostToken<'brand> {
    token: UnsafeCell<GhostToken<'brand>>,
    /// Global flag: true if a writer is active or pending.
    writer_active: AtomicBool,
    /// Striped reader counters, padded to avoid false sharing.
    shards: [CachePadded<AtomicUsize>; SHARD_COUNT],
}

// SAFETY: Synchronization is handled internally.
unsafe impl<'brand> Sync for SharedGhostToken<'brand> {}
unsafe impl<'brand> Send for SharedGhostToken<'brand> {}

impl<'brand> SharedGhostToken<'brand> {
    /// Creates a new shared token handle.
    pub fn new(token: GhostToken<'brand>) -> Self {
        let shards = core::array::from_fn(|_| CachePadded::new(AtomicUsize::new(0)));

        Self {
            token: UnsafeCell::new(token),
            writer_active: AtomicBool::new(false),
            shards,
        }
    }

    /// Helper to get the current thread's shard index, initializing it if necessary.
    #[inline(always)]
    fn current_shard_index() -> usize {
        THREAD_SHARD_INDEX.with(|idx| {
            if let Some(i) = idx.get() {
                i
            } else {
                let mut hasher = DefaultHasher::new();
                thread::current().id().hash(&mut hasher);
                let i = (hasher.finish() as usize) & SHARD_MASK;
                idx.set(Some(i));
                i
            }
        })
    }

    /// Simple backoff strategy: spin a bit, then yield.
    fn backoff(spin_count: &mut u32) {
        if *spin_count < 10 {
            std::hint::spin_loop();
        } else {
            thread::yield_now();
        }
        *spin_count = spin_count.saturating_add(1);
    }

    /// Acquires a shared read lock on the token.
    pub fn read<'a>(&'a self) -> SharedTokenReadGuard<'a, 'brand> {
        let shard_idx = Self::current_shard_index();
        let shard = &self.shards[shard_idx];
        let mut spins = 0;

        loop {
            // Optimistic read check
            if self.writer_active.load(Ordering::SeqCst) {
                Self::backoff(&mut spins);
                continue;
            }

            // Increment local shard counter.
            // We use SeqCst to ensure this store is visible before we load `writer_active`.
            shard.fetch_add(1, Ordering::SeqCst);

            // Re-check writer status to ensure we didn't race with a writer.
            // This Load must not be reordered before the previous Store.
            if self.writer_active.load(Ordering::SeqCst) {
                // Writer became active; back off
                shard.fetch_sub(1, Ordering::SeqCst);
                Self::backoff(&mut spins);
                continue;
            }

            // Successfully acquired
            return SharedTokenReadGuard {
                parent: self,
                shard_index: shard_idx,
            };
        }
    }

    /// Acquires an exclusive write lock on the token.
    pub fn write<'a>(&'a self) -> SharedTokenWriteGuard<'a, 'brand> {
        let mut spins = 0;

        // Phase 1: Announce writer intent.
        // Must be SeqCst to ensure readers see this store.
        while self.writer_active.swap(true, Ordering::SeqCst) {
            Self::backoff(&mut spins);
        }

        // Phase 2: Wait for all readers to drain.
        // We reset spins here because phase 1 might have taken a while,
        // but now we are waiting on a different condition.
        spins = 0;
        for shard in &self.shards {
            while shard.load(Ordering::SeqCst) > 0 {
                Self::backoff(&mut spins);
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
        // Decrement the specific shard we incremented.
        // SeqCst to match the Acquire/SeqCst logic in write().
        self.parent.shards[self.shard_index].fetch_sub(1, Ordering::SeqCst);
    }
}

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
        // Clear the writer bit.
        self.parent.writer_active.store(false, Ordering::SeqCst);
    }
}
