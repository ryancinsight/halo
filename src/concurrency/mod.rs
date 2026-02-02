//! Concurrency helpers for ghost-branded types.
//!
//! Important: Ghost types enforce aliasing discipline, not synchronization.
//! This module provides *scoped* patterns for sending/sharing the token across
//! threads with minimal overhead and without locking the data itself.

pub mod atomic;
pub mod cache_padded;
pub mod scoped;
/// Synchronization primitives.
pub mod sync;
pub mod worklist;

pub use cache_padded::CachePadded;

use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::thread;

/// The number of shards used for sharded concurrency patterns.
pub const SHARD_COUNT: usize = 32;

/// Bitmask for fast shard index calculation.
pub const SHARD_MASK: usize = SHARD_COUNT - 1;

thread_local! {
    static THREAD_SHARD_INDEX: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Generates a hash for the current thread.
pub fn current_thread_hash() -> usize {
    let mut hasher = DefaultHasher::new();
    thread::current().id().hash(&mut hasher);
    hasher.finish() as usize
}

/// Returns the shard index for the current thread.
///
/// This value is cached thread-locally to avoid recomputing the hash.
pub fn current_shard_index() -> usize {
    THREAD_SHARD_INDEX.with(|idx| {
        if let Some(i) = idx.get() {
            i
        } else {
            let i = current_thread_hash() & SHARD_MASK;
            idx.set(Some(i));
            i
        }
    })
}
