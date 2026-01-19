use core::ptr::NonNull;
use core::mem;
use core::ops::Deref;

/// A compile-time reference-counted pointer that tracks ownership fractions.
///
/// `N` is the number of shares held by this instance.
/// `D` is the total number of shares in existence.
///
/// Safety invariant: `N <= D` and the sum of `N` across all instances pointing to the same allocation equals `D`.
#[derive(Debug)]
pub struct StaticRc<T, const N: usize, const D: usize> {
    ptr: NonNull<T>,
}

impl<T, const N: usize, const D: usize> StaticRc<T, N, D> {
    /// Creates a new `StaticRc` with full ownership.
    ///
    /// # Panics
    ///
    /// Panics if `N != D`.
    pub fn new(value: T) -> Self {
        assert_eq!(N, D, "New StaticRc must have N == D");
        let ptr = Box::into_raw(Box::new(value));
        // SAFETY: Box::into_raw returns a non-null pointer.
        unsafe {
            Self {
                ptr: NonNull::new_unchecked(ptr),
            }
        }
    }

    /// Constructs a `StaticRc` from a raw pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` points to a valid heap allocation of `T`,
    /// and that the ownership fractions are correctly managed such that `N` and `D`
    /// reflect the state relative to other `StaticRc` instances.
    /// Specifically for `N == D`, this instance takes full ownership.
    ///
    /// The pointer must be compatible with `Box::from_raw` for deallocation.
    pub unsafe fn from_raw(ptr: NonNull<T>) -> Self {
        Self { ptr }
    }

    /// Splits the ownership into two instances.
    ///
    /// The caller must specify the amount `M` to split off, and the remaining amount `R`.
    /// `M + R` must equal `N`.
    ///
    /// Returns `(StaticRc<T, M, D>, StaticRc<T, R, D>)`.
    ///
    /// # Panics
    ///
    /// Panics if `M + R != N`.
    pub fn split<const M: usize, const R: usize>(self) -> (StaticRc<T, M, D>, StaticRc<T, R, D>) {
        assert_eq!(M + R, N, "Split amounts must sum to current shares");
        // We are consuming self, so we don't drop it.
        let ptr = self.ptr;
        mem::forget(self);

        unsafe {
            (
                StaticRc { ptr },
                StaticRc { ptr },
            )
        }
    }

    /// Adjusts the total density `D` using type-level arithmetic.
    ///
    /// Converts `StaticRc<T, N, D>` to `StaticRc<T, NEW_N, NEW_D>`.
    ///
    /// # Panics
    ///
    /// Panics if the ownership fraction is not preserved: `N / D != NEW_N / NEW_D` (i.e., `N * NEW_D != NEW_N * D`).
    pub fn adjust<const NEW_N: usize, const NEW_D: usize>(self) -> StaticRc<T, NEW_N, NEW_D> {
        // Check if fraction is equivalent: N/D == NEW_N/NEW_D => N * NEW_D == NEW_N * D
        assert_eq!(N * NEW_D, NEW_N * D, "Ownership fraction must be preserved");
         let ptr = self.ptr;
        mem::forget(self);
        unsafe { StaticRc { ptr } }
    }

    /// Joins two instances back together.
    ///
    /// The caller must specify the result amount `SUM`.
    /// `SUM` must equal `N + M`.
    ///
    /// Returns `StaticRc<T, SUM, D>`.
    ///
    /// # Panics
    ///
    /// Panics if the two instances point to different allocations, or if `N + M != SUM`.
    pub fn join<const M: usize, const SUM: usize>(self, other: StaticRc<T, M, D>) -> StaticRc<T, SUM, D> {
        assert_eq!(self.ptr, other.ptr, "Cannot join StaticRc pointing to different allocations");
        assert_eq!(N + M, SUM, "Join result amount must equal sum of shares");

        let ptr = self.ptr;
        mem::forget(self);
        mem::forget(other);

        unsafe { StaticRc { ptr } }
    }

    /// Returns a reference to the inner value.
    pub fn get(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T, const N: usize, const D: usize> Drop for StaticRc<T, N, D> {
    fn drop(&mut self) {
        if N == D {
            // We own all shares, so we can deallocate.
            unsafe {
                let _ = Box::from_raw(self.ptr.as_ptr());
            }
        } else {
             #[cfg(debug_assertions)]
             {
                 // In debug builds, panic if we drop a partial share, as this indicates a leak.
                 // However, normal program termination might drop partial shares if we don't care about leaks?
                 // The prompt says: "Implement Drop such that it panics in debug_assertions if an instance is dropped while N!= D to help catch logic leaks."
                 if std::thread::panicking() {
                     // If we are already panicking, don't double panic.
                 } else {
                     panic!("StaticRc dropped with N != D (N={}, D={})", N, D);
                 }
             }
        }
    }
}

impl<T, const N: usize, const D: usize> Deref for StaticRc<T, N, D> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

unsafe impl<T: Send + Sync, const N: usize, const D: usize> Send for StaticRc<T, N, D> {}
unsafe impl<T: Send + Sync, const N: usize, const D: usize> Sync for StaticRc<T, N, D> {}
