//! Active wrapper for `BrandedSkipList`.

use crate::GhostToken;
use super::BrandedSkipList;
use super::branded::{Iter, IterMut};
use crate::collections::BrandedCollection;
use std::borrow::Borrow;

/// A wrapper around a mutable reference to a `BrandedSkipList` and a mutable reference to a `GhostToken`.
pub struct ActiveSkipList<'a, 'brand, K, V> {
    list: &'a mut BrandedSkipList<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, V> ActiveSkipList<'a, 'brand, K, V> {
    /// Creates a new active skip list handle.
    pub fn new(list: &'a mut BrandedSkipList<'brand, K, V>, token: &'a mut GhostToken<'brand>) -> Self {
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
    pub fn iter(&self) -> Iter<'_, 'brand, K, V> {
        self.list.iter(self.token)
    }

    /// Iterates over the list elements mutably.
    pub fn iter_mut(&mut self) -> IterMut<'_, 'brand, K, V> {
        self.list.iter_mut(self.token)
    }
}

/// Extension trait to easily create ActiveSkipList from BrandedSkipList.
pub trait ActivateSkipList<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveSkipList<'a, 'brand, K, V>;
}

impl<'brand, K, V> ActivateSkipList<'brand, K, V> for BrandedSkipList<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveSkipList<'a, 'brand, K, V> {
        ActiveSkipList::new(self, token)
    }
}

// TODO: Implement FromIterator and Extend if possible, but that requires token access which traits don't provide.
// Active structs allow providing methods that look like standard ones but we can't implement standard traits easily.
