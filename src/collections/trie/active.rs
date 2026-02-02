//! Active wrappers for `trie` collections.

use super::{BrandedRadixTrieMap, BrandedRadixTrieSet};
use crate::token::traits::GhostBorrowMut;

/// A wrapper around a mutable reference to a `BrandedRadixTrieMap` and a mutable reference to a `Token`.
pub struct ActiveRadixTrieMap<'a, 'brand, K, V, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    map: &'a mut BrandedRadixTrieMap<'brand, K, V>,
    token: &'a mut Token,
}

impl<'a, 'brand, K, V, Token> ActiveRadixTrieMap<'a, 'brand, K, V, Token>
where
    K: AsRef<[u8]>,
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active map handle.
    pub fn new(
        map: &'a mut BrandedRadixTrieMap<'brand, K, V>,
        token: &'a mut Token,
    ) -> Self {
        Self { map, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Clears the map.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Inserts a key-value pair.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(self.token, key, value)
    }

    /// Returns a shared reference to the value corresponding to the key.
    pub fn get(&self, key: K) -> Option<&V> {
        self.map.get(self.token, key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        self.map.get_mut(self.token, key)
    }

    /// Removes a key from the map.
    pub fn remove(&mut self, key: K) -> Option<V> {
        self.map.remove(self.token, key)
    }

    /// Iterates over all elements.
    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(&[u8], &V),
    {
        self.map.for_each(self.token, f)
    }

    /// Iterates over the map.
    pub fn iter(&self) -> impl Iterator<Item = (crate::alloc::BrandedRc<'brand, crate::collections::vec::BrandedVec<'brand, u8>>, &V)> + use<'_, 'brand, K, V, Token> {
        self.map.iter(self.token)
    }
}

/// Extension trait to easily create ActiveRadixTrieMap from BrandedRadixTrieMap.
pub trait ActivateRadixTrieMap<'brand, K, V> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveRadixTrieMap<'a, 'brand, K, V, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, K, V> ActivateRadixTrieMap<'brand, K, V> for BrandedRadixTrieMap<'brand, K, V>
where
    K: AsRef<[u8]>,
{
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveRadixTrieMap<'a, 'brand, K, V, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveRadixTrieMap::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedRadixTrieSet` and a mutable reference to a `Token`.
pub struct ActiveRadixTrieSet<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    set: &'a mut BrandedRadixTrieSet<'brand, T>,
    token: &'a mut Token,
}

impl<'a, 'brand, T, Token> ActiveRadixTrieSet<'a, 'brand, T, Token>
where
    T: AsRef<[u8]>,
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active set handle.
    pub fn new(
        set: &'a mut BrandedRadixTrieSet<'brand, T>,
        token: &'a mut Token,
    ) -> Self {
        Self { set, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Clears the set.
    pub fn clear(&mut self) {
        self.set.clear();
    }

    /// Adds a value to the set.
    pub fn insert(&mut self, value: T) -> bool {
        self.set.insert(self.token, value)
    }

    /// Returns true if the set contains the value.
    pub fn contains(&self, value: T) -> bool {
        self.set.contains(self.token, value)
    }

    /// Removes a value from the set.
    pub fn remove(&mut self, value: T) -> bool {
        self.set.remove(self.token, value)
    }

    /// Iterates over all elements.
    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(&[u8]),
    {
        self.set.for_each(self.token, f)
    }

    /// Iterates over the set.
    pub fn iter(&self) -> impl Iterator<Item = crate::alloc::BrandedRc<'brand, crate::collections::vec::BrandedVec<'brand, u8>>> + use<'_, 'brand, T, Token> {
        self.set.iter(self.token)
    }
}

/// Extension trait to easily create ActiveRadixTrieSet from BrandedRadixTrieSet.
pub trait ActivateRadixTrieSet<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveRadixTrieSet<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T> ActivateRadixTrieSet<'brand, T> for BrandedRadixTrieSet<'brand, T>
where
    T: AsRef<[u8]>,
{
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveRadixTrieSet<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveRadixTrieSet::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_radix_trie_map() {
        GhostToken::new(|mut token| {
            let mut map = BrandedRadixTrieMap::new();
            let mut active = map.activate(&mut token);

            active.insert("foo", 1);
            active.insert("bar", 2);

            assert_eq!(active.get("foo"), Some(&1));
            assert_eq!(active.get("bar"), Some(&2));
            assert_eq!(active.get("baz"), None);

            active.insert("foo", 3);
            assert_eq!(active.get("foo"), Some(&3));

            active.remove("bar");
            assert_eq!(active.get("bar"), None);
        });
    }

    #[test]
    fn test_active_radix_trie_set() {
        GhostToken::new(|mut token| {
            let mut set = BrandedRadixTrieSet::new();
            let mut active = set.activate(&mut token);

            active.insert("foo");
            active.insert("bar");

            assert!(active.contains("foo"));
            assert!(active.contains("bar"));
            assert!(!active.contains("baz"));

            active.remove("bar");
            assert!(!active.contains("bar"));
        });
    }
}
