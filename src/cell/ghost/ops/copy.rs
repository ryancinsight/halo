use super::super::GhostCell;
use crate::GhostToken;

impl<'brand, T: Copy> GhostCell<'brand, T> {
    /// Copies the contained value.
    #[inline(always)]
    pub fn get(&self, token: &GhostToken<'brand>) -> T {
        *self.borrow(token)
    }

    /// Overwrites the contained value.
    #[inline(always)]
    pub fn set(&self, token: &mut GhostToken<'brand>, value: T) {
        *self.borrow_mut(token) = value;
    }
}
