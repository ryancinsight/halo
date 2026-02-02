//! `BrandedCow` — a copy-on-write pointer for token-gated values.
//!
//! This type allows working with either owned values or token-gated references
//! uniformly. It provides methods to access the value using a `GhostToken`.

use crate::token::traits::GhostBorrow;
use crate::GhostCell;

/// A copy-on-write pointer for token-gated values.
pub enum BrandedCow<'a, 'brand, T> {
    /// An owned value.
    Owned(T),
    /// A reference to a token-gated cell.
    Borrowed(&'a GhostCell<'brand, T>),
}

impl<'a, 'brand, T> BrandedCow<'a, 'brand, T> {
    /// Returns a reference to the value, requiring a token if borrowed.
    #[inline]
    pub fn get<'token, Token>(&'token self, token: &'token Token) -> &'token T
    where
        Token: GhostBorrow<'brand>,
    {
        match self {
            BrandedCow::Owned(val) => val,
            BrandedCow::Borrowed(cell) => cell.borrow(token),
        }
    }

    /// Returns a mutable reference to the value if owned.
    ///
    /// If borrowed, returns `None`. To mutate a borrowed value, you must
    /// clone it into an `Owned` variant or access the original cell mutably separately.
    #[inline]
    pub fn get_mut_if_owned(&mut self) -> Option<&mut T> {
        match self {
            BrandedCow::Owned(val) => Some(val),
            BrandedCow::Borrowed(_) => None,
        }
    }

    /// Converts into an owned value, cloning if necessary.
    #[inline]
    pub fn into_owned<Token>(self, token: &Token) -> T
    where
        T: Clone,
        Token: GhostBorrow<'brand>,
    {
        match self {
            BrandedCow::Owned(val) => val,
            BrandedCow::Borrowed(cell) => cell.borrow(token).clone(),
        }
    }
}

impl<'a, 'brand, T: Clone> Clone for BrandedCow<'a, 'brand, T> {
    fn clone(&self) -> Self {
        match self {
            BrandedCow::Owned(val) => BrandedCow::Owned(val.clone()),
            BrandedCow::Borrowed(cell) => BrandedCow::Borrowed(*cell),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_cow_owned() {
        GhostToken::new(|token| {
            let cow: BrandedCow<'_, '_, i32> = BrandedCow::Owned(42);
            assert_eq!(*cow.get(&token), 42);

            let mut cow = cow;
            *cow.get_mut_if_owned().unwrap() += 1;
            assert_eq!(*cow.get(&token), 43);
        });
    }

    #[test]
    fn test_branded_cow_borrowed() {
        GhostToken::new(|token| {
            let cell = GhostCell::new(100);
            let cow: BrandedCow<'_, '_, i32> = BrandedCow::Borrowed(&cell);

            assert_eq!(*cow.get(&token), 100);

            // Cannot mutate borrowed cow via get_mut_if_owned
            // let mut cow = cow;
            // assert!(cow.get_mut_if_owned().is_none());

            // Conversion to owned
            let owned = cow.into_owned(&token);
            assert_eq!(owned, 100);
        });
    }
}
