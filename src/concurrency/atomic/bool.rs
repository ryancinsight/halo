use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

/// A branded `AtomicBool`.
#[repr(transparent)]
pub struct GhostAtomicBool<'brand> {
    inner: AtomicBool,
    _brand: PhantomData<&'brand mut ()>,
}

impl<'brand> GhostAtomicBool<'brand> {
    /// Creates a new branded atomic bool.
    #[inline(always)]
    pub const fn new(value: bool) -> Self {
        Self {
            inner: AtomicBool::new(value),
            _brand: PhantomData,
        }
    }

    /// Loads the current value.
    #[inline(always)]
    pub fn load(&self, order: Ordering) -> bool {
        self.inner.load(order)
    }

    /// Stores a new value.
    #[inline(always)]
    pub fn store(&self, value: bool, order: Ordering) {
        self.inner.store(value, order);
    }

    /// Swaps the current value, returning the previous value.
    #[inline(always)]
    pub fn swap(&self, value: bool, order: Ordering) -> bool {
        self.inner.swap(value, order)
    }

    /// Stores a value if the current value equals `current`.
    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: bool,
        new: bool,
        success: Ordering,
        failure: Ordering,
    ) -> Result<bool, bool> {
        self.inner.compare_exchange(current, new, success, failure)
    }

    /// Stores a value if the current value equals `current` (weak version).
    #[inline(always)]
    pub fn compare_exchange_weak(
        &self,
        current: bool,
        new: bool,
        success: Ordering,
        failure: Ordering,
    ) -> Result<bool, bool> {
        self.inner.compare_exchange_weak(current, new, success, failure)
    }

    /// Performs a compare-exchange operation with automatic ordering selection.
    ///
    /// Uses `AcqRel` for success and `Acquire` for failure, which is appropriate
    /// for most lock-free algorithms.
    #[inline(always)]
    pub fn compare_exchange_cas(&self, current: bool, new: bool) -> Result<bool, bool> {
        self.compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
    }

    /// Performs a weak compare-exchange operation with automatic ordering selection.
    ///
    /// Uses `AcqRel` for success and `Acquire` for failure, which is appropriate
    /// for most lock-free algorithms. Weak operations may spuriously fail.
    #[inline(always)]
    pub fn compare_exchange_weak_cas(&self, current: bool, new: bool) -> Result<bool, bool> {
        self.compare_exchange_weak(current, new, Ordering::AcqRel, Ordering::Acquire)
    }

    /// Conditionally sets the value to `true` if it is currently `false`.
    ///
    /// Returns `true` if the value was set, `false` if it was already `true`.
    #[inline]
    pub fn test_and_set(&self, order: Ordering) -> bool {
        self.compare_exchange(false, true, order, Ordering::Relaxed).is_ok()
    }

    /// Atomically loads and conditionally updates the value.
    ///
    /// If the current value is `false`, sets it to `true` and returns `true`.
    /// If the current value is `true`, returns `false`.
    #[inline]
    pub fn fetch_set(&self, order: Ordering) -> bool {
        self.swap(true, order)
    }
}

unsafe impl<'brand> Send for GhostAtomicBool<'brand> {}
unsafe impl<'brand> Sync for GhostAtomicBool<'brand> {}


