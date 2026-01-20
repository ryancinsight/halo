use super::branded_box::BrandedBox;
use crate::cell::GhostCell;
use crate::token::InvariantLifetime;
use crate::GhostToken;
use core::alloc::Layout;
use core::mem::{self, MaybeUninit};
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

    /// Joins two instances back together without checking pointer equality.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `self` and `other` originate from the same allocation.
    /// This is guaranteed if the `StaticRc` was created via `StaticRc::scope` and the types match.
    pub unsafe fn join_unchecked<const M: usize, const SUM: usize>(
        self,
        other: StaticRc<'id, T, M, D>,
    ) -> StaticRc<'id, T, SUM, D> {
        debug_assert_eq!(self.ptr, other.ptr, "StaticRc::join_unchecked mismatch");
        assert_eq!(N + M, SUM, "Join result amount must equal sum of shares");

        let ptr = self.ptr;
        mem::forget(self);
        mem::forget(other);

        StaticRc {
            ptr,
            _brand: InvariantLifetime::default(),
        }
    }

    /// Returns a reference to the inner value.
    pub fn get(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<'id, T, const D: usize> StaticRc<'id, T, D, D> {
    /// Returns a mutable reference to the inner value.
    ///
    /// This is only available when the `StaticRc` has full ownership (`N == D`).
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }

    /// Converts a `Box<T>` into a `StaticRc`.
    ///
    /// This reuses the allocation from the `Box`, avoiding reallocation.
    /// The resulting `StaticRc` has full ownership (`N == D`).
    pub fn from_box(b: Box<T>) -> Self {
        let ptr = Box::into_raw(b);
        // SAFETY: Box::into_raw gives a valid non-null pointer.
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
        Self {
            ptr,
            _brand: InvariantLifetime::default(),
        }
    }

    /// Converts the `StaticRc` back into a `Box<T>`.
    ///
    /// This is only possible if we hold full ownership (`N == D`).
    pub fn into_box(self) -> Box<T> {
        let ptr = self.ptr;
        mem::forget(self);
        // SAFETY: The pointer came from `std::alloc` (or compatible Box), and we own it fully.
        unsafe { Box::from_raw(ptr.as_ptr()) }
    }

    /// Converts a `BrandedBox<'id, T>` into a `StaticRc`.
    ///
    /// This reuses the allocation.
    pub fn from_branded_box(b: BrandedBox<'id, T>) -> Self {
        let ptr = b.into_raw();
        unsafe {
            Self {
                ptr,
                _brand: InvariantLifetime::default(),
            }
        }
    }

    /// Converts the `StaticRc` back into a `BrandedBox<'id, T>`.
    pub fn into_branded_box(self) -> BrandedBox<'id, T> {
        let ptr = self.ptr;
        mem::forget(self);
        unsafe { BrandedBox::from_raw(ptr) }
    }
}

impl<'id, T, const N: usize, const D: usize> StaticRc<'id, MaybeUninit<T>, N, D> {
    /// Creates a new `StaticRc` with uninitialized memory.
    ///
    /// # Panics
    ///
    /// Panics if `N != D`.
    pub fn new_uninit() -> Self {
        assert_eq!(N, D, "New StaticRc must have N == D");

        let layout = Layout::new::<T>();
        // SAFETY: T is Sized, layout is valid.
        let raw = if layout.size() == 0 {
            NonNull::dangling().as_ptr()
        } else {
            unsafe { std::alloc::alloc(layout) as *mut MaybeUninit<T> }
        };

        if raw.is_null() {
            handle_alloc_error(layout);
        }

        // SAFETY: raw is non-null.
        unsafe {
            Self {
                ptr: NonNull::new_unchecked(raw),
                _brand: InvariantLifetime::default(),
            }
        }
    }

    /// Assumes the memory is initialized and converts to `StaticRc<T>`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the content has been initialized.
    pub unsafe fn assume_init(self) -> StaticRc<'id, T, N, D> {
        let ptr = self.ptr.cast::<T>();
        mem::forget(self);
        StaticRc {
            ptr,
            _brand: InvariantLifetime::default(),
        }
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

impl<'id, T> StaticRc<'id, T, 1, 1> {
    /// Creates a new `StaticRc` within a scoped closure, ensuring a unique brand.
    ///
    /// This pattern guarantees that the `StaticRc` and its splits have a unique lifetime `'id`,
    /// preventing accidental mixing with other `StaticRc` instances.
    /// This allows for safe optimization when joining.
    pub fn scope<F, R>(value: T, f: F) -> R
    where
        F: for<'new_id> FnOnce(StaticRc<'new_id, T, 1, 1>) -> R,
    {
        // We create a new allocation.
        // We manually construct the StaticRc with a fresh brand via the closure bound.
        // Since StaticRc::new takes 'id from the caller, we can't use it directly and satisfy higher-ranked bounds cleanly
        // without some friction, so we inline the logic or use an unsafe cast.
        // Inlining logic is safer to see.
        let layout = Layout::new::<T>();
        let raw = if layout.size() == 0 {
            NonNull::dangling().as_ptr()
        } else {
            unsafe { std::alloc::alloc(layout) as *mut T }
        };

        if raw.is_null() {
            handle_alloc_error(layout);
        }

        unsafe {
            ptr::write(raw, value);
            // Construct the branded RC.
            // The closure expects StaticRc<'new_id, T, 1, 1>.
            // InvariantLifetime::default() creates the necessary ZST.
            f(StaticRc {
                ptr: NonNull::new_unchecked(raw),
                _brand: InvariantLifetime::default(),
            })
        }
    }
}

/// Integration with `GhostCell` for ergonomic token-gated access.
impl<'id, 'brand, T, const N: usize, const D: usize> StaticRc<'id, GhostCell<'brand, T>, N, D> {
    /// Borrows the inner `GhostCell` immutably using the provided token.
    ///
    /// This is a convenience method that forwards to `GhostCell::borrow`.
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a T {
        self.get().borrow(token)
    }

    /// Borrows the inner `GhostCell` mutably using the provided token.
    ///
    /// This is a convenience method that forwards to `GhostCell::borrow_mut`.
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a mut T {
        self.get().borrow_mut(token)
    }
}
