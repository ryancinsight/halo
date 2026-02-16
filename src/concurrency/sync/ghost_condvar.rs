//! `GhostCondvar` — a condition variable for blocking threads.

use core::sync::atomic::{AtomicU32, Ordering};
use super::ghost_mutex::{GhostMutex, GhostMutexGuard};
use super::{wait_on_u32, wake_all_u32, wake_one_u32};

/// A condition variable that allows threads to wait for a signal while
/// releasing a `GhostMutex`.
pub struct GhostCondvar {
    state: AtomicU32,
}

impl Default for GhostCondvar {
    fn default() -> Self {
        Self::new()
    }
}

impl GhostCondvar {
    /// Creates a new condition variable.
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
        }
    }

    /// Blocks the current thread until this condition variable is notified.
    ///
    /// This function will atomically unlock the mutex specified (represented by
    /// `guard`) and block the current thread. Upon returning, the mutex will be
    /// re-acquired.
    pub fn wait<'a, 'brand>(
        &self,
        guard: GhostMutexGuard<'a, 'brand>,
    ) -> GhostMutexGuard<'a, 'brand> {
        let mutex = guard.mutex();
        let seq = self.state.load(Ordering::Relaxed);

        // Unlock the mutex by dropping the guard.
        drop(guard);

        // Wait for the state to change from `seq`.
        // If notify() happens before this call but after load, wait_on_u32
        // will see the changed value and return immediately.
        wait_on_u32(&self.state, seq);

        // Re-acquire the mutex.
        mutex.lock()
    }

    /// Wakes up one blocked thread on this condition variable.
    pub fn notify_one(&self) {
        self.state.fetch_add(1, Ordering::Relaxed);
        wake_one_u32(&self.state);
    }

    /// Wakes up all blocked threads on this condition variable.
    pub fn notify_all(&self) {
        self.state.fetch_add(1, Ordering::Relaxed);
        wake_all_u32(&self.state);
    }
}
