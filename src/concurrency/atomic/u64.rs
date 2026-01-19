use core::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

/// A branded `AtomicU64`.
///
/// The brand is a compile-time marker used to tie an atomic to a Ghost “domain”.
/// It does **not** affect the atomic’s concurrency behavior.
#[repr(transparent)]
pub struct GhostAtomicU64<'brand> {
    inner: AtomicU64,
    _brand: PhantomData<&'brand mut ()>,
}

impl<'brand> GhostAtomicU64<'brand> {
    /// Creates a new atomic value.
    #[inline(always)]
    pub const fn new(value: u64) -> Self {
        Self {
            inner: AtomicU64::new(value),
            _brand: PhantomData,
        }
    }

    /// Loads the current value.
    #[inline(always)]
    pub fn load(&self, order: Ordering) -> u64 {
        self.inner.load(order)
    }

    /// Stores a new value.
    #[inline(always)]
    pub fn store(&self, value: u64, order: Ordering) {
        self.inner.store(value, order);
    }

    /// Swaps the current value, returning the previous value.
    #[inline(always)]
    pub fn swap(&self, value: u64, order: Ordering) -> u64 {
        self.inner.swap(value, order)
    }

    /// Stores `new` if the current value equals `current`.
    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.inner.compare_exchange(current, new, success, failure)
    }

    /// Adds to the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_add(&self, value: u64, order: Ordering) -> u64 {
        self.inner.fetch_add(value, order)
    }

    /// Subtracts from the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_sub(&self, value: u64, order: Ordering) -> u64 {
        self.inner.fetch_sub(value, order)
    }
}

// SAFETY: `AtomicU64` is Send + Sync; brand is a ZST marker.
unsafe impl<'brand> Send for GhostAtomicU64<'brand> {}
unsafe impl<'brand> Sync for GhostAtomicU64<'brand> {}
