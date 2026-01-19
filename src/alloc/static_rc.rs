use crate::token::InvariantLifetime;
use core::alloc::Layout;
use core::mem;
use core::ops::Deref;
use core::ptr::{self, NonNull};
use std::alloc::{dealloc, handle_alloc_error};

/// A compile-time reference-counted pointer that tracks ownership fractions.
///
/// `N` is the number of shares held by this instance.
/// `D` is the total number of shares in existence.
///
/// Safety invariant: `N <= D` and the sum of `N` across all instances pointing to the same allocation equals `D`.
#[derive(Debug)]
pub struct StaticRc<'id, T, const N: usize, const D: usize> {
    ptr: NonNull<T>,
    _brand: InvariantLifetime<'id>,
}

impl<'id, T, const N: usize, const D: usize> StaticRc<'id, T, N, D> {
    /// Creates a new `StaticRc` with full ownership.
    ///
    /// # Panics
    ///
    /// Panics if `N != D`.
    pub fn new(value: T) -> Self {
        assert_eq!(N, D, "New StaticRc must have N == D");

        let layout = Layout::new::<T>();
        // SAFETY: T is Sized, layout is valid.
        let raw = if layout.size() == 0 {
            NonNull::dangling().as_ptr()
        } else {
            unsafe { std::alloc::alloc(layout) as *mut T }
        };

        if raw.is_null() {
            handle_alloc_error(layout);
        }

        // SAFETY: raw is non-null.
        unsafe {
            ptr::write(raw, value);
            Self {
                ptr: NonNull::new_unchecked(raw),
                _brand: InvariantLifetime::default(),
            }
        }
    }

    /// Constructs a `StaticRc` from a raw pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` points to a valid heap allocation of `T`,
    /// allocated via `std::alloc::alloc` with `Layout::new::<T>()`.
    /// The ownership fractions must be correctly managed.
    pub unsafe fn from_raw(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _brand: InvariantLifetime::default(),
        }
    }

    /// Splits the ownership into two instances.
    ///
    /// The caller must specify the amount `M` to split off, and the remaining amount `R`.
    /// `M + R` must equal `N`.
    ///
    /// Returns `(StaticRc<'id, T, M, D>, StaticRc<'id, T, R, D>)`.
    ///
    /// # Panics
    ///
    /// Panics if `M + R != N`.
    pub fn split<const M: usize, const R: usize>(
        self,
    ) -> (StaticRc<'id, T, M, D>, StaticRc<'id, T, R, D>) {
        assert_eq!(M + R, N, "Split amounts must sum to current shares");
        // We are consuming self, so we don't drop it.
        let ptr = self.ptr;
        mem::forget(self);

        // SAFETY: We are just splitting ownership, ptr remains valid.
        unsafe {
            (
                StaticRc {
                    ptr,
                    _brand: InvariantLifetime::default(),
                },
                StaticRc {
                    ptr,
                    _brand: InvariantLifetime::default(),
                },
            )
        }
    }

    /// Adjusts the total density `D` using type-level arithmetic.
    ///
    /// Converts `StaticRc<'id, T, N, D>` to `StaticRc<'id, T, NEW_N, NEW_D>`.
    ///
    /// # Panics
    ///
    /// Panics if the ownership fraction is not preserved: `N / D != NEW_N / NEW_D` (i.e., `N * NEW_D != NEW_N * D`).
    pub fn adjust<const NEW_N: usize, const NEW_D: usize>(self) -> StaticRc<'id, T, NEW_N, NEW_D> {
        // Check if fraction is equivalent: N/D == NEW_N/NEW_D => N * NEW_D == NEW_N * D
        assert_eq!(N * NEW_D, NEW_N * D, "Ownership fraction must be preserved");
        let ptr = self.ptr;
        mem::forget(self);
        unsafe {
            StaticRc {
                ptr,
                _brand: InvariantLifetime::default(),
            }
        }
    }

    /// Joins two instances back together.
    ///
    /// The caller must specify the result amount `SUM`.
    /// `SUM` must equal `N + M`.
    ///
    /// Returns `StaticRc<'id, T, SUM, D>`.
    ///
    /// # Panics
    ///
    /// Panics if the two instances point to different allocations, or if `N + M != SUM`.
    pub fn join<const M: usize, const SUM: usize>(
        self,
        other: StaticRc<'id, T, M, D>,
    ) -> StaticRc<'id, T, SUM, D> {
        assert_eq!(
            self.ptr, other.ptr,
            "Cannot join StaticRc pointing to different allocations"
        );
        assert_eq!(N + M, SUM, "Join result amount must equal sum of shares");

        let ptr = self.ptr;
        mem::forget(self);
        mem::forget(other);

        unsafe {
            StaticRc {
                ptr,
                _brand: InvariantLifetime::default(),
            }
        }
    }

    /// Returns a reference to the inner value.
    pub fn get(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<'id, T, const N: usize, const D: usize> Drop for StaticRc<'id, T, N, D> {
    fn drop(&mut self) {
        if N == D {
            // We own all shares, so we can deallocate.
            unsafe {
                ptr::drop_in_place(self.ptr.as_ptr());

                let layout = Layout::new::<T>();
                if layout.size() != 0 {
                    dealloc(self.ptr.as_ptr() as *mut u8, layout);
                }
            }
        } else {
            #[cfg(debug_assertions)]
            {
                if !std::thread::panicking() {
                    panic!("StaticRc dropped with N != D (N={}, D={})", N, D);
                }
            }
        }
    }
}

impl<'id, T, const N: usize, const D: usize> Deref for StaticRc<'id, T, N, D> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

unsafe impl<'id, T: Send + Sync, const N: usize, const D: usize> Send for StaticRc<'id, T, N, D> {}
unsafe impl<'id, T: Send + Sync, const N: usize, const D: usize> Sync for StaticRc<'id, T, N, D> {}
