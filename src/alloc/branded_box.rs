use core::marker::PhantomData;
use core::ptr::NonNull;
use core::alloc::Layout;
use std::alloc::{alloc, dealloc, handle_alloc_error};
use crate::GhostToken;
use super::static_rc::StaticRc;
use crate::cell::GhostCell;

/// A uniquely owned heap allocation that is tied to a specific type-level brand.
///
/// Implemented from scratch using raw pointers to ensure full control over allocation
/// and branding, independent of `Box<T>`.
pub struct BrandedBox<'id, T> {
    ptr: NonNull<T>,
    _marker: PhantomData<fn(&'id ()) -> &'id ()>,
}

impl<'id, T> BrandedBox<'id, T> {
    /// Creates a new `BrandedBox` containing `value`.
    ///
    /// The existence of `&mut GhostToken<'id>` proves we are in the scope of brand `'id`.
    pub fn new(value: T, _token: &mut GhostToken<'id>) -> Self {
        let layout = Layout::new::<T>();
        let ptr = if layout.size() == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: Layout is correct for T.
            let raw = unsafe { alloc(layout) } as *mut T;
            if raw.is_null() {
                handle_alloc_error(layout);
            }
            // SAFETY: pointer is valid and non-null.
            unsafe {
                NonNull::new_unchecked(raw)
            }
        };

        // SAFETY: ptr is valid for writes (dangling is valid for ZST, alloc ptr is valid for sized).
        unsafe {
            ptr::write(ptr.as_ptr(), value);
        }

        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Access the inner value using the token.
    ///
    /// Requires `&mut self` (unique ownership of box) and `&mut GhostToken` (unique ownership of token/brand access).
    /// This satisfies the AXM requirement.
    pub fn borrow_mut<'a>(&'a mut self, _token: &'a mut GhostToken<'id>) -> &'a mut T {
        // SAFETY: We own the allocation and have exclusive access via &mut self.
        unsafe { self.ptr.as_mut() }
    }

    /// Access the inner value immutably using the token.
    pub fn borrow<'a>(&'a self, _token: &'a GhostToken<'id>) -> &'a T {
        // SAFETY: We own the allocation.
        unsafe { self.ptr.as_ref() }
    }

    /// Downgrades the BrandedBox into a shared StaticRc.
    ///
    /// Converts `BrandedBox<'id, T>` into `StaticRc<GhostCell<'id, T>, D, D>`.
    /// This allows the object to enter a shared/cyclic structure while maintaining the brand.
    /// The result has full ownership (N=D), which can then be split using `StaticRc::split`.
    pub fn into_shared<const D: usize>(self) -> StaticRc<GhostCell<'id, T>, D, D> {
        let ptr = self.ptr;
        // Forget self so we don't deallocate.
        std::mem::forget(self);

        // Cast to GhostCell pointer.
        // SAFETY: GhostCell<T> is #[repr(transparent)] over T (transitively), so layout matches.
        // Allocator was std::alloc::alloc with Layout::new::<T>(), which matches Layout::new::<GhostCell<T>>().
        let cell_ptr = ptr.as_ptr() as *mut GhostCell<'id, T>;

        // SAFETY:
        // 1. ptr is a valid heap allocation of T.
        // 2. We transferred ownership from `self` (consumed) to `StaticRc`.
        // 3. The allocation was created via `std::alloc::alloc`, which is compatible with `Box::from_raw`
        //    (which StaticRc uses for Drop) IF layout matches.
        unsafe {
             StaticRc::from_raw(NonNull::new_unchecked(cell_ptr))
        }
    }
}

impl<'id, T> Drop for BrandedBox<'id, T> {
    fn drop(&mut self) {
        let layout = Layout::new::<T>();
        // SAFETY: We own the pointer. It is valid to drop the value.
        unsafe {
            std::ptr::drop_in_place(self.ptr.as_ptr());
        }

        if layout.size() != 0 {
            // SAFETY: We own the pointer, it was allocated with alloc, and layout matches.
            unsafe {
                dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

use core::ptr;
