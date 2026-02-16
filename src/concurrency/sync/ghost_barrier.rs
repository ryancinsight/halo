//! `GhostBarrier` — a token-gated barrier.

use std::sync::{Barrier, BarrierWaitResult};
use std::marker::PhantomData;
use crate::token::traits::GhostBorrow;

/// A barrier that requires a `GhostToken` to participate.
///
/// This ensures that only threads with access to a specific brand can synchronize
/// using this barrier. This is useful for scoped concurrency where threads
/// operate on shared branded data.
pub struct GhostBarrier<'brand> {
    inner: Barrier,
    _phantom: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> GhostBarrier<'brand> {
    /// Creates a new barrier that can block a given number of threads.
    pub fn new(n: usize) -> Self {
        Self {
            inner: Barrier::new(n),
            _phantom: PhantomData,
        }
    }

    /// Blocks the current thread until all threads have rendezvoused here.
    ///
    /// The `_token` argument proves that the thread possesses the necessary
    /// capability (branded token) to participate in this synchronization scope.
    pub fn wait(&self, _token: &impl GhostBorrow<'brand>) -> BarrierWaitResult {
        self.inner.wait()
    }
}
