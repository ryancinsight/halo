//! Cache-padded wrapper to prevent false sharing.

use std::ops::{Deref, DerefMut};

/// Helper struct for cache line padding to avoid false sharing.
/// We use 128 bytes to be safe for most architectures (x86 is 64, Apple Silicon can be 128).
#[repr(align(128))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    /// Creates a new cache-padded value.
    pub const fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}
