use super::static_rc::StaticRc;
use crate::cell::GhostCell;
use crate::foundation::ghost::ptr::BrandedNonNull;
use crate::GhostToken;
use core::alloc::Layout;
use core::ptr;
use core::ptr::NonNull;
use std::alloc::{alloc, dealloc, handle_alloc_error};

/// A uniquely owned heap allocation that is tied to a specific type-level brand.
///
/// Implemented from scratch using raw pointers to ensure full control over allocation
/// and branding, independent of `Box<T>`.
pub struct BrandedBox<'id, T> {
    ptr: BrandedNonNull<'id, T>,
}

impl<'id, T> BrandedBox<'id, T> {
    /// Creates a new `BrandedBox` containing `value`.
    ///
    /// The allocation uses `std::alloc::alloc`.
    pub fn new(value: T) -> Self {
        let layout = Layout::new::<T>();
        // SAFETY: T is Sized, layout is valid.
        let raw = if layout.size() == 0 {
            NonNull::dangling().as_ptr()
        } else {
            unsafe { alloc(layout) as *mut T }
        };

        if raw.is_null() {
            handle_alloc_error(layout);
        }

        // SAFETY: raw is non-null.
        unsafe {
            ptr::write(raw, value);
            Self {
                ptr: BrandedNonNull::new_unchecked(raw),
            }
        }
    }

    /// Access the inner value using the token.
    ///
    /// Requires `&mut self` (unique ownership of box) and `&mut GhostToken` (unique ownership of token/brand access).
    pub fn borrow_mut<'a>(&'a mut self, token: &'a mut GhostToken<'id>) -> &'a mut T {
        // SAFETY: We own the allocation and have exclusive access via &mut self.
        // The token ensures we have the right brand.
        unsafe { self.ptr.borrow_mut(token) }
    }

    /// Access the inner value immutably using the token.
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'id>) -> &'a T {
        // SAFETY: We own the allocation.
        unsafe { self.ptr.borrow(token) }
    }

    /// Downgrades the BrandedBox into a shared StaticRc.
    ///
    /// Converts `BrandedBox<'id, T>` into `StaticRc<'id, GhostCell<'id, T>, D, D>`.
    /// This allows the object to enter a shared/cyclic structure while maintaining the brand.
    /// The result has full ownership (N=D), which can then be split using `StaticRc::split`.
    pub fn into_shared<const D: usize>(self) -> StaticRc<'id, GhostCell<'id, T>, D, D> {
        let ptr = self.ptr;
        // Forget self so we don't deallocate.
        std::mem::forget(self);

        // Cast to GhostCell pointer.
        // SAFETY: GhostCell<T> is #[repr(transparent)] over T (transitively), so layout matches.
        let cell_ptr = ptr.as_ptr() as *mut GhostCell<'id, T>;

        // SAFETY:
        // 1. ptr is a valid heap allocation of T.
        // 2. We transferred ownership from `self` (consumed) to `StaticRc`.
        // 3. The allocation was created via `std::alloc::alloc`, which is compatible with `StaticRc::drop` (dealloc).
        unsafe { StaticRc::from_raw(BrandedNonNull::new_unchecked(cell_ptr)) }
    }
}

impl<'id, T> Drop for BrandedBox<'id, T> {
    fn drop(&mut self) {
        // SAFETY: We own the pointer. It is valid to drop the value.
        unsafe {
            ptr::drop_in_place(self.ptr.as_ptr());

            let layout = Layout::new::<T>();
            if layout.size() != 0 {
                dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

// SAFETY: Send/Sync if T is Send/Sync.
// The brand ensures safety, but thread safety depends on T.
unsafe impl<'id, T: Send> Send for BrandedBox<'id, T> {}
unsafe impl<'id, T: Sync> Sync for BrandedBox<'id, T> {}
