use core::ptr;

use crate::GhostToken;

use super::ghost_cell::GhostCell;

impl<'brand, T> GhostCell<'brand, T> {
    /// Borrows the cell immutably.
    #[inline(always)]
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a T {
        self.inner.get(token)
    }

    /// Borrows the cell mutably.
    #[inline(always)]
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a mut T {
        self.inner.get_mut(token)
    }

    /// Replaces the contained value, returning the old value.
    #[inline]
    pub fn replace(&self, token: &mut GhostToken<'brand>, value: T) -> T {
        self.inner.replace(value, token)
    }

    /// Swaps the values of two `GhostCell`s.
    #[inline]
    pub fn swap(&self, token: &mut GhostToken<'brand>, other: &Self) {
        let a = self.inner.as_mut_ptr(token);
        let b = other.inner.as_mut_ptr(token);

        // SAFETY:
        // - `token` is a linear capability, so safe code cannot concurrently access
        //   either cell mutably.
        // - `ptr::swap` is safe for possibly-equal pointers.
        unsafe { ptr::swap(a, b) };
    }
}







