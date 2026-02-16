//! `GhostMutex` — a mutex that guards a `GhostToken`.

use crate::token::GhostToken;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};
use super::{wait_on_u32, wake_one_u32};

const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1;
const CONTENDED: u32 = 2;

/// A mutex that protects a `GhostToken`, allowing it to be shared across threads
/// while ensuring exclusive access.
///
/// This is useful when you have a `GhostToken` that brands a collection of data structure
/// (e.g. `GhostCell`s) and you want to share mutable access to that data structure
/// across multiple threads.
///
/// Since `GhostToken` is a linear type (effectively), only one thread can hold
/// `&mut GhostToken` at a time. `GhostMutex` enforces this invariant at runtime.
pub struct GhostMutex<'brand> {
    token: UnsafeCell<GhostToken<'brand>>,
    /// 0: unlocked, 1: locked, 2: locked & contended
    state: AtomicU32,
}

unsafe impl<'brand> Sync for GhostMutex<'brand> {}
unsafe impl<'brand> Send for GhostMutex<'brand> {}

impl<'brand> GhostMutex<'brand> {
    /// Creates a new mutex wrapping the given token.
    pub const fn new(token: GhostToken<'brand>) -> Self {
        Self {
            token: UnsafeCell::new(token),
            state: AtomicU32::new(UNLOCKED),
        }
    }

    /// Acquires the mutex, blocking the current thread until it is able to do so.
    ///
    /// Returns a guard that provides mutable access to the inner `GhostToken`.
    pub fn lock(&self) -> GhostMutexGuard<'_, 'brand> {
        if self.state.compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed).is_err() {
            self.lock_slow();
        }
        GhostMutexGuard { lock: self }
    }

    /// Attempts to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<GhostMutexGuard<'_, 'brand>> {
        if self.state.compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(GhostMutexGuard { lock: self })
        } else {
            None
        }
    }

    #[cold]
    fn lock_slow(&self) {
        let mut state = self.state.load(Ordering::Relaxed);
        loop {
            // If unlocked, try to acquire
            if state == UNLOCKED {
                match self.state.compare_exchange_weak(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed) {
                    Ok(_) => return, // Locked!
                    Err(s) => state = s,
                }
                continue;
            }

            // If not contended, mark as contended
            if state == LOCKED {
                match self.state.compare_exchange_weak(LOCKED, CONTENDED, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => state = CONTENDED,
                    Err(s) => state = s,
                }
            }

            // If contended, wait
            if state == CONTENDED {
                wait_on_u32(&self.state, CONTENDED);
                state = self.state.load(Ordering::Relaxed);
            }
        }
    }

    /// Unlocks the mutex. This is called by `GhostMutexGuard`'s Drop impl,
    /// but is exposed here for `GhostCondvar` use.
    ///
    /// # Safety
    ///
    /// This must only be called by the thread that currently holds the lock.
    pub(crate) unsafe fn unlock(&self) {
        if self.state.swap(UNLOCKED, Ordering::Release) == CONTENDED {
            wake_one_u32(&self.state);
        }
    }
}

/// A guard that provides mutable access to the `GhostToken` protected by a `GhostMutex`.
pub struct GhostMutexGuard<'a, 'brand> {
    lock: &'a GhostMutex<'brand>,
}

impl<'a, 'brand> Deref for GhostMutexGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We hold the lock, so we have exclusive access.
        // However, we can only return shared reference here because Deref::Target is &T.
        // Wait, Deref::deref returns &Target.
        // But we want to be able to use the token for mutation?
        // Yes, GhostToken usage usually requires &mut token for mutation of cells.
        // But &token allows reading cells.
        unsafe { &*self.lock.token.get() }
    }
}

impl<'a, 'brand> DerefMut for GhostMutexGuard<'a, 'brand> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We hold the lock, so we have exclusive access.
        unsafe { &mut *self.lock.token.get() }
    }
}

impl<'a, 'brand> Drop for GhostMutexGuard<'a, 'brand> {
    fn drop(&mut self) {
        unsafe {
            self.lock.unlock();
        }
    }
}

/// Helper to allow `GhostCondvar` to access inner lock.
impl<'a, 'brand> GhostMutexGuard<'a, 'brand> {
    pub(crate) fn mutex(&self) -> &'a GhostMutex<'brand> {
        self.lock
    }
}
