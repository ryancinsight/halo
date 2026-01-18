//! `SharedGhostToken` — a thread-safe, reference-counted handle for ghost tokens.
//!
//! This primitive allows a `GhostToken` to be shared across multiple threads, enabling
//! concurrent read access to branded data structures (like `BrandedHashMap`) and controlled
//! exclusive write access.
//!
//! It effectively acts as an `RwLock` for the token capability, allowing:
//! - Multiple concurrent readers (holding `&GhostToken`)
//! - One exclusive writer (holding `&mut GhostToken`)
//!
//! This is crucial for sharing branded data structures across threads without losing
//! the safety guarantees of the branding system.

use crate::GhostToken;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::ops::{Deref, DerefMut};

/// A thread-safe, shared handle to a ghost token.
///
/// This struct wraps a `GhostToken` in an `RwLock`, allowing it to be shared (e.g., via `Arc`)
/// across threads. It provides methods to acquire read or write access to the token, which
/// in turn allows access to the branded data.
pub struct SharedGhostToken<'brand> {
    lock: RwLock<GhostToken<'brand>>,
}

impl<'brand> SharedGhostToken<'brand> {
    /// Creates a new shared token handle.
    ///
    /// Consumes the unique `GhostToken` to ensure exclusive control is transferred to this handle.
    pub fn new(token: GhostToken<'brand>) -> Self {
        Self {
            lock: RwLock::new(token),
        }
    }

    /// Acquires a shared read lock on the token.
    ///
    /// Returns a guard that dereferences to `&GhostToken<'brand>`.
    /// While this guard is held, multiple other read locks can be acquired, but no write locks.
    ///
    /// # Panics
    /// Panics if the lock is poisoned.
    pub fn read<'a>(&'a self) -> SharedTokenReadGuard<'a, 'brand> {
        SharedTokenReadGuard {
            guard: self.lock.read().expect("SharedGhostToken lock poisoned"),
        }
    }

    /// Acquires an exclusive write lock on the token.
    ///
    /// Returns a guard that dereferences to `&mut GhostToken<'brand>`.
    /// While this guard is held, no other read or write locks can be acquired.
    ///
    /// # Panics
    /// Panics if the lock is poisoned.
    pub fn write<'a>(&'a self) -> SharedTokenWriteGuard<'a, 'brand> {
        SharedTokenWriteGuard {
            guard: self.lock.write().expect("SharedGhostToken lock poisoned"),
        }
    }
}

/// RAII guard for shared read access to a ghost token.
pub struct SharedTokenReadGuard<'a, 'brand> {
    guard: RwLockReadGuard<'a, GhostToken<'brand>>,
}

impl<'a, 'brand> Deref for SharedTokenReadGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// RAII guard for exclusive write access to a ghost token.
pub struct SharedTokenWriteGuard<'a, 'brand> {
    guard: RwLockWriteGuard<'a, GhostToken<'brand>>,
}

impl<'a, 'brand> Deref for SharedTokenWriteGuard<'a, 'brand> {
    type Target = GhostToken<'brand>;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, 'brand> DerefMut for SharedTokenWriteGuard<'a, 'brand> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}
