use core::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

/// A branded `AtomicUsize`.
#[repr(transparent)]
pub struct GhostAtomicUsize<'brand> {
    inner: AtomicUsize,
    _brand: PhantomData<&'brand mut ()>,
}

impl<'brand> GhostAtomicUsize<'brand> {
    /// Creates a new branded atomic usize.
    #[inline(always)]
    pub const fn new(value: usize) -> Self {
        Self {
            inner: AtomicUsize::new(value),
            _brand: PhantomData,
        }
    }

    /// Loads the current value.
    #[inline(always)]
    pub fn load(&self, order: Ordering) -> usize {
        self.inner.load(order)
    }

    /// Stores a new value.
    #[inline(always)]
    pub fn store(&self, value: usize, order: Ordering) {
        self.inner.store(value, order);
    }

    /// Swaps the current value, returning the previous value.
    #[inline(always)]
    pub fn swap(&self, value: usize, order: Ordering) -> usize {
        self.inner.swap(value, order)
    }

    /// Stores a value if the current value equals `current`.
    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.inner.compare_exchange(current, new, success, failure)
    }

    /// Adds to the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_add(&self, value: usize, order: Ordering) -> usize {
        self.inner.fetch_add(value, order)
    }

    /// Bitwise OR with the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_or(&self, value: usize, order: Ordering) -> usize {
        self.inner.fetch_or(value, order)
    }

    /// Subtracts from the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_sub(&self, value: usize, order: Ordering) -> usize {
        self.inner.fetch_sub(value, order)
    }
}

unsafe impl<'brand> Send for GhostAtomicUsize<'brand> {}
unsafe impl<'brand> Sync for GhostAtomicUsize<'brand> {}


