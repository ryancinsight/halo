use crate::GhostToken;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};
use std::ptr::NonNull;
use super::wait_queue::{WaitQueue, WaitNode};
use std::ops::{Deref, DerefMut};

/// A blocking mutex that protects a `GhostToken`.
///
/// This primitive allows for exclusive access to the branded token, blocking the thread
/// if the token is already in use.
///
/// # States
/// - 0: Unlocked
/// - 1: Locked, no waiters (likely)
/// - 2: Locked, waiters exist (contended)
pub struct GhostMutex<'brand> {
    token: UnsafeCell<GhostToken<'brand>>,
    state: AtomicU8,
    pub(super) queue: WaitQueue,
}

// Safety: Mutex provides exclusive access.
unsafe impl<'brand> Sync for GhostMutex<'brand> {}
unsafe impl<'brand> Send for GhostMutex<'brand> {}

impl<'brand> GhostMutex<'brand> {
    const UNLOCKED: u8 = 0;
    const LOCKED: u8 = 1;
    const CONTENDED: u8 = 2;

    pub const fn new(token: GhostToken<'brand>) -> Self {
        Self {
            token: UnsafeCell::new(token),
            state: AtomicU8::new(Self::UNLOCKED),
            queue: WaitQueue::new(),
        }
    }

    #[inline]
    pub fn lock(&self) -> GhostMutexGuard<'_, 'brand> {
        if self.state.compare_exchange(Self::UNLOCKED, Self::LOCKED, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            return GhostMutexGuard { mutex: self };
        }
        self.lock_slow()
    }

    #[cold]
    fn lock_slow(&self) -> GhostMutexGuard<'_, 'brand> {
        let mut spin_count = 0;
        loop {
            // Spin
            if self.state.load(Ordering::Relaxed) == Self::UNLOCKED {
                 if self.state.compare_exchange(Self::UNLOCKED, Self::LOCKED, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                     return GhostMutexGuard { mutex: self };
                 }
            }
            if spin_count < 40 {
                spin_count += 1;
                std::hint::spin_loop();
                continue;
            }

            // Park
            let node = WaitNode::new();
            let node_ptr = NonNull::from(&node);

            unsafe {
                self.queue.lock();

                // Double check state under lock
                let s = self.state.load(Ordering::Relaxed);
                if s == Self::UNLOCKED {
                     // Try to grab
                     if self.state.compare_exchange(Self::UNLOCKED, Self::LOCKED, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                         self.queue.unlock();
                         return GhostMutexGuard { mutex: self };
                     }
                }

                // Mark as contended if needed
                if s != Self::CONTENDED {
                     self.state.store(Self::CONTENDED, Ordering::Relaxed);
                }

                // Push self to queue
                self.queue.push_locked(node_ptr);
                self.queue.unlock();
            }

            // Park
            std::thread::park();
        }
    }

    pub fn unlock(&self) {
        if self.state.compare_exchange(Self::LOCKED, Self::UNLOCKED, Ordering::Release, Ordering::Relaxed).is_ok() {
            return;
        }
        self.unlock_slow();
    }

    #[cold]
    fn unlock_slow(&self) {
        unsafe {
            self.queue.lock();
            // We release the lock fundamentally
            self.state.store(Self::UNLOCKED, Ordering::Release);

            // Wake one waiter
            if let Some(node) = self.queue.pop_locked() {
                node.as_ref().wake();
            }
            self.queue.unlock();
        }
    }
}

pub struct GhostMutexGuard<'a, 'brand> {
    pub(super) mutex: &'a GhostMutex<'brand>,
}

impl<'a, 'brand> Deref for GhostMutexGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.token.get() }
    }
}

impl<'a, 'brand> DerefMut for GhostMutexGuard<'a, 'brand> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.token.get() }
    }
}

impl<'a, 'brand> Drop for GhostMutexGuard<'a, 'brand> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}
