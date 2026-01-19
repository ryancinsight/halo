use super::wait_queue::{WaitQueue, WaitNode};
use super::mutex::{GhostMutexGuard};
use std::ptr::NonNull;

/// A Condition Variable that works with `GhostMutex`.
pub struct GhostCondvar {
    queue: WaitQueue,
}

impl GhostCondvar {
    pub const fn new() -> Self {
        Self {
            queue: WaitQueue::new(),
        }
    }

    /// Blocks the current thread until this condition variable is notified.
    ///
    /// The `guard` is consumed (lock released), the thread blocks, and then
    /// the lock is re-acquired and a new guard returned.
    pub fn wait<'a, 'brand>(
        &self,
        guard: GhostMutexGuard<'a, 'brand>,
    ) -> GhostMutexGuard<'a, 'brand> {
        let mutex = guard.mutex;

        // Create wait node
        let node = WaitNode::new();
        let node_ptr = NonNull::from(&node);

        unsafe {
            self.queue.lock();
            self.queue.push_locked(node_ptr);
            self.queue.unlock();
        }

        // Release the mutex
        // This triggers unlock() which might wake a mutex waiter.
        drop(guard);

        // Park
        std::thread::park();

        // Re-acquire mutex
        mutex.lock()
    }

    /// Wakes up one blocked thread on this condvar.
    pub fn notify_one(&self) {
        unsafe {
            self.queue.lock();
            if let Some(node) = self.queue.pop_locked() {
                node.as_ref().wake();
            }
            self.queue.unlock();
        }
    }

    /// Wakes up all blocked threads on this condvar.
    pub fn notify_all(&self) {
        unsafe {
            self.queue.lock();
            while let Some(node) = self.queue.pop_locked() {
                node.as_ref().wake();
            }
            self.queue.unlock();
        }
    }
}

// Safety: Condvar is thread-safe.
unsafe impl Sync for GhostCondvar {}
unsafe impl Send for GhostCondvar {}
