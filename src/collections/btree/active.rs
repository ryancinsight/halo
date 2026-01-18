//! `ActiveBTreeMap` — a BrandedBTreeMap bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope. By holding the token exclusively, it can expose a standard `BTreeMap`-like
//! API without requiring the token as an argument for every call.

use crate::GhostToken;
use super::BrandedBTreeMap;
use std::borrow::Borrow;

/// A wrapper around a mutable reference to a `BrandedBTreeMap` and a mutable reference to a `GhostToken`.
pub struct ActiveBTreeMap<'a, 'brand, K, V> {
    map: &'a mut BrandedBTreeMap<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, V> ActiveBTreeMap<'a, 'brand, K, V> {
    /// Creates a new active map handle.
    pub fn new(map: &'a mut BrandedBTreeMap<'brand, K, V>, token: &'a mut GhostToken<'brand>) -> Self {
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
}

impl<'a, 'brand, K, V> ActiveBTreeMap<'a, 'brand, K, V>
where
    K: Ord,
{
    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.map.get(self.token, key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.map.get_mut(self.token, key)
    }

    /// Returns true if the map contains a value for the specified key.
    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.map.contains_key(key)
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(key, value)
    }

    /// Removes a key from the map.
    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        self.map.remove(key)
    }

    /// Iterates over the map.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter(self.token)
    }

    /// Applies `f` to all entries in the map, allowing mutation of values.
    pub fn for_each_mut<F>(&mut self, f: F)
    where
        F: FnMut(&K, &mut V),
    {
        self.map.for_each_mut(self.token, f)
    }
}

/// Extension trait to easily create ActiveBTreeMap from BrandedBTreeMap.
pub trait ActivateBTreeMap<'brand, K, V> {
    /// Activates the map with the given token, returning a handle that bundles them.
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveBTreeMap<'a, 'brand, K, V>;
}

impl<'brand, K, V> ActivateBTreeMap<'brand, K, V> for BrandedBTreeMap<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveBTreeMap<'a, 'brand, K, V> {
        ActiveBTreeMap::new(self, token)
    }
}
