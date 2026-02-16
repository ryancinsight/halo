//! `GhostOnceLock` — a thread-safe, token-branded once-lock.

use std::sync::OnceLock;
use crate::token::traits::{GhostBorrow, GhostBorrowMut};
use crate::cell::raw::GhostUnsafeCell;

/// A thread-safe initialization primitive that requires a ghost token for access.
///
/// `GhostOnceLock` mirrors `std::sync::OnceLock` but ensures that the value
/// can only be accessed by threads possessing the correct `GhostToken` (or a compatible guard).
pub struct GhostOnceLock<'brand, T> {
    inner: GhostUnsafeCell<'brand, OnceLock<T>>,
}

impl<'brand, T> GhostOnceLock<'brand, T> {
    /// Creates a new empty `GhostOnceLock`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            inner: GhostUnsafeCell::new(OnceLock::new()),
        }
    }

    /// Returns `true` if the lock has been initialized.
    #[inline]
    pub fn is_initialized(&self, token: &impl GhostBorrow<'brand>) -> bool {
        self.inner.get(token).get().is_some()
    }

    /// Gets a reference to the value if initialized, requiring a token.
    #[inline]
    pub fn get<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> Option<&'a T> {
        self.inner.get(token).get()
    }

    /// Gets a mutable reference to the value if initialized, requiring a mutable token.
    #[inline]
    pub fn get_mut_branded<'a>(&'a self, token: &'a mut impl GhostBorrowMut<'brand>) -> Option<&'a mut T> {
        self.inner.get_mut(token).get_mut()
    }

    /// Gets a mutable reference to the value if initialized, without requiring a token.
    ///
    /// This is safe because `&mut self` guarantees exclusive access.
    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.inner.get_mut_exclusive().get_mut()
    }

    /// Sets the value if uninitialized, requiring a token.
    ///
    /// Returns `Ok(())` if the value was set, or `Err(value)` if it was already set.
    #[inline]
    pub fn set(&self, token: &impl GhostBorrow<'brand>, value: T) -> Result<(), T> {
        self.inner.get(token).set(value)
    }

    /// Gets the value, initializing it with `f` if needed, requiring a token.
    #[inline]
    pub fn get_or_init<'a, F>(&'a self, token: &'a impl GhostBorrow<'brand>, f: F) -> &'a T
    where
        F: FnOnce() -> T,
    {
        self.inner.get(token).get_or_init(f)
    }

    /// Consumes the lock, returning the initialized value if it exists.
    #[inline]
    pub fn into_inner(self) -> Option<T> {
        self.inner.into_inner().into_inner()
    }

    /// Takes the value out of the lock, leaving it uninitialized, without requiring a token.
    ///
    /// This is safe because `&mut self` guarantees exclusive access.
    #[inline]
    pub fn take(&mut self) -> Option<T> {
        self.inner.get_mut_exclusive().take()
    }
}

impl<'brand, T> Default for GhostOnceLock<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: GhostOnceLock is Sync/Send if OnceLock<T> is Sync/Send.
// This is automatically handled by the compiler as GhostUnsafeCell is Sync/Send
// with appropriate bounds.
