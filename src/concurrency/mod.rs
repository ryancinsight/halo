//! Concurrency helpers for ghost-branded types.
//!
//! Important: Ghost types enforce aliasing discipline, not synchronization.
//! This module provides *scoped* patterns for sending/sharing the token across
//! threads with minimal overhead and without locking the data itself.

pub mod atomic;
pub mod cache_padded;
pub mod scoped;
pub mod sync;
pub mod worklist;

pub use cache_padded::CachePadded;
