//! Active wrapper for `BrandedSkipList`.

use super::BrandedSkipList;
use crate::collections::BrandedCollection;
// use crate::GhostToken;
use std::borrow::Borrow;

/// A wrapper around a mutable reference to a `BrandedSkipList` and a mutable reference to a `GhostToken`.
pub struct ActiveSkipList<'a, 'brand, K, V, Token>
where
    Token: crate::token::traits::GhostBorrowMut<'brand>,
{
    list: &'a mut BrandedSkipList<'brand, K, V>,
    token: &'a mut Token,
}

impl<'a, 'brand, K, V, Token> ActiveSkipList<'a, 'brand, K, V, Token>
where
    Token: crate::token::traits::GhostBorrowMut<'brand>,
{
    /// Creates a new active skip list handle.
    pub fn new(
        list: &'a mut BrandedSkipList<'brand, K, V>,
        token: &'a mut Token,
    ) -> Self {
        Self { list, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        self.list.get(self.token, key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        self.list.get_mut(self.token, key)
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        K: Ord,
    {
        self.list.insert(self.token, key, value)
    }

    /// Iterates over the list elements.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_ + use<'_, 'brand, K, V, Token> {
        self.list.iter(self.token)
    }

    /// Iterates over the list elements mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> + '_ + use<'_, 'brand, K, V, Token> {
        self.list.iter_mut(self.token)
    }
}

/// Extension trait to easily create ActiveSkipList from BrandedSkipList.
pub trait ActivateSkipList<'brand, K, V> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveSkipList<'a, 'brand, K, V, Token>
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>;
}

impl<'brand, K, V> ActivateSkipList<'brand, K, V> for BrandedSkipList<'brand, K, V> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveSkipList<'a, 'brand, K, V, Token>
    where
        Token: crate::token::traits::GhostBorrowMut<'brand>,
    {
        ActiveSkipList::new(self, token)
    }
}

impl<'a, 'brand, K, V, Token> Extend<(K, V)> for ActiveSkipList<'a, 'brand, K, V, Token>
where
    K: Ord,
    Token: crate::token::traits::GhostBorrowMut<'brand>,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_skip_list_extend() {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            {
                let mut active = list.activate(&mut token);
                active.extend(vec![(1, 10), (2, 20), (3, 30)]);
            }

            assert_eq!(list.len(), 3);
            assert_eq!(*list.get(&token, &1).unwrap(), 10);
            assert_eq!(*list.get(&token, &2).unwrap(), 20);
            assert_eq!(*list.get(&token, &3).unwrap(), 30);
        });
    }
}
