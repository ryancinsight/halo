use core::marker::PhantomData;
use core::ptr::NonNull;
use crate::GhostToken;
use super::static_rc::StaticRc;
use crate::cell::GhostCell;

/// A uniquely owned heap allocation that is tied to a specific type-level brand.
///
/// Wraps a `Box<T>` but restricts access via a `GhostToken`.
pub struct BrandedBox<'id, T> {
    inner: Box<T>,
    _marker: PhantomData<fn(&'id ()) -> &'id ()>,
}

impl<'id, T> BrandedBox<'id, T> {
    /// Creates a new `BrandedBox` containing `value`.
    ///
    /// The existence of `&mut GhostToken<'id>` proves we are in the scope of brand `'id`.
    pub fn new(value: T, _token: &mut GhostToken<'id>) -> Self {
        Self {
            inner: Box::new(value),
            _marker: PhantomData,
        }
    }

    /// Access the inner value using the token.
    ///
    /// Requires `&mut self` (unique ownership of box) and `&mut GhostToken` (unique ownership of token/brand access).
    /// This satisfies the AXM requirement.
    pub fn borrow_mut<'a>(&'a mut self, _token: &'a mut GhostToken<'id>) -> &'a mut T {
        &mut *self.inner
    }

    /// Access the inner value immutably using the token.
    pub fn borrow<'a>(&'a self, _token: &'a GhostToken<'id>) -> &'a T {
        &*self.inner
    }

    /// Downgrades the BrandedBox into a shared StaticRc.
    ///
    /// Converts `BrandedBox<'id, T>` into `StaticRc<GhostCell<'id, T>, D, D>`.
    /// This allows the object to enter a shared/cyclic structure while maintaining the brand.
    /// The result has full ownership (N=D), which can then be split using `StaticRc::split`.
    pub fn into_shared<const D: usize>(self) -> StaticRc<GhostCell<'id, T>, D, D> {
        let ptr = Box::into_raw(self.inner);
        // Box<T> is layout compatible with Box<GhostCell<'id, T>> because GhostCell is transparent.
        let ptr = ptr as *mut GhostCell<'id, T>;

        // SAFETY:
        // 1. ptr is a valid heap allocation of T (and thus GhostCell<T>).
        // 2. We are constructing with N=D (full ownership).
        // 3. We transferred ownership from `self` (consumed) to `StaticRc`.
        unsafe {
             StaticRc::from_raw(NonNull::new_unchecked(ptr))
        }
    }
}
