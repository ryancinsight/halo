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

    /// Bitwise AND with the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_and(&self, value: usize, order: Ordering) -> usize {
        self.inner.fetch_and(value, order)
    }

    /// Bitwise XOR with the current value, returning the previous value.
    #[inline(always)]
    pub fn fetch_xor(&self, value: usize, order: Ordering) -> usize {
        self.inner.fetch_xor(value, order)
    }

    /// Stores a value if the current value equals `current` (weak version).
    #[inline(always)]
    pub fn compare_exchange_weak(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.inner
            .compare_exchange_weak(current, new, success, failure)
    }

    /// Atomically loads the current value and applies a function to it.
    ///
    /// This is a convenience method for read-modify-write operations.
    /// The function `f` receives the current value and returns the new value.
    /// Returns the previous value.
    #[inline]
    pub fn fetch_update<F>(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: F,
    ) -> Result<usize, usize>
    where
        F: FnMut(usize) -> Option<usize>,
    {
        self.inner.fetch_update(set_order, fetch_order, f)
    }

    /// Conditionally stores a value if the current value satisfies a predicate.
    ///
    /// This combines a load with a conditional store operation.
    /// Returns `Ok(previous_value)` if the store succeeded, `Err(current_value)` if not.
    #[inline]
    pub fn conditional_store<F>(
        &self,
        new_value: usize,
        success_order: Ordering,
        failure_order: Ordering,
        predicate: F,
    ) -> Result<usize, usize>
    where
        F: Fn(usize) -> bool,
    {
        let current = self.load(failure_order);
        if predicate(current) {
            self.compare_exchange(current, new_value, success_order, failure_order)
        } else {
            Err(current)
        }
    }

    /// Performs a compare-exchange operation with automatic ordering selection.
    ///
    /// Uses `AcqRel` for success and `Acquire` for failure, which is appropriate
    /// for most lock-free algorithms.
    #[inline(always)]
    pub fn compare_exchange_cas(&self, current: usize, new: usize) -> Result<usize, usize> {
        self.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
    }

    /// Performs a weak compare-exchange operation with automatic ordering selection.
    ///
    /// Uses `AcqRel` for success and `Acquire` for failure, which is appropriate
    /// for most lock-free algorithms. Weak operations may spuriously fail.
    #[inline(always)]
    pub fn compare_exchange_weak_cas(&self, current: usize, new: usize) -> Result<usize, usize> {
        self.compare_exchange_weak(current, new, Ordering::AcqRel, Ordering::Acquire)
    }
}

unsafe impl<'brand> Send for GhostAtomicUsize<'brand> {}
unsafe impl<'brand> Sync for GhostAtomicUsize<'brand> {}
