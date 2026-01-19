//! Concurrency helpers for ghost-branded types.
//!
//! Important: Ghost types enforce aliasing discipline, not synchronization.
//! This module provides *scoped* patterns for sending/sharing the token across
//! threads with minimal overhead and without locking the data itself.

pub mod scoped;
pub mod atomic;
pub mod worklist;
pub mod sync;


