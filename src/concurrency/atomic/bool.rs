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
}

unsafe impl<'brand> Send for GhostAtomicBool<'brand> {}
unsafe impl<'brand> Sync for GhostAtomicBool<'brand> {}


