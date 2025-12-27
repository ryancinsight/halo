use crate::GhostToken;
use super::super::GhostCell;

impl<'brand, T> GhostCell<'brand, T> {
    /// Applies a function to the shared borrow and returns its result.
    #[inline]
    pub fn apply<F, R>(&self, token: &GhostToken<'brand>, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(self.borrow(token))
    }

    /// Applies a function to the mutable borrow and returns its result.
    #[inline]
    pub fn apply_mut<F, R>(&self, token: &mut GhostToken<'brand>, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        f(self.borrow_mut(token))
    }

    /// Mutates the contained value using `f`.
    #[inline]
    pub fn update<F>(&self, token: &mut GhostToken<'brand>, f: F)
    where
        F: FnOnce(&mut T),
    {
        f(self.borrow_mut(token));
    }

    /// Maps the cell's value into a new `GhostCell` of the same brand.
    #[inline]
    pub fn map<F, U>(&self, token: &GhostToken<'brand>, f: F) -> GhostCell<'brand, U>
    where
        F: FnOnce(&T) -> U,
    {
        GhostCell::new(f(self.borrow(token)))
    }
}

impl<'brand, T: Clone> GhostCell<'brand, T> {
    /// Clones the contained value.
    #[inline]
    pub fn cloned(&self, token: &GhostToken<'brand>) -> T {
        self.borrow(token).clone()
    }
}







