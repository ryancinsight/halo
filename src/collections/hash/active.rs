//! `ActiveHashMap` — a BrandedHashMap bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope. By holding the token exclusively, it can expose a standard `HashMap`-like
//! API without requiring the token as an argument for every call.

use crate::GhostToken;
use super::{BrandedHashMap, BrandedHashSet};
use std::hash::{Hash, BuildHasher};

/// A wrapper around a mutable reference to a `BrandedHashMap` and a mutable reference to a `GhostToken`.
pub struct ActiveHashMap<'a, 'brand, K, V, S> {
    map: &'a mut BrandedHashMap<'brand, K, V, S>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, V, S> ActiveHashMap<'a, 'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Creates a new active map handle.
    pub fn new(map: &'a mut BrandedHashMap<'brand, K, V, S>, token: &'a mut GhostToken<'brand>) -> Self {
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

    /// Returns a shared reference to the value corresponding to the key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(self.token, key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(self.token, key)
    }

    /// Returns true if the map contains a value for the specified key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(key, value)
    }

    /// Removes a key from the map.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key)
    }

    /// Reserves capacity.
    pub fn reserve(&mut self, additional: usize) {
        self.map.reserve(additional);
    }

    /// Iterates over keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }

    /// Iterates over values.
    pub fn values(&self) -> impl Iterator<Item = &V> + use<'_, 'brand, K, V, S> {
        self.map.values(self.token)
    }

    /// Iterates over key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + use<'_, 'brand, K, V, S> {
        self.map.keys().zip(self.map.values(self.token))
    }

    /// Iterates over entries mutably.
    pub fn for_each_mut<F>(&mut self, f: F)
    where
        F: FnMut(&K, &mut V),
    {
        self.map.for_each_mut(self.token, f)
    }

    /// Returns a mutable iterator over the map entries.
    pub fn iter_mut(&mut self) -> super::hash_map::IterMut<'_, 'brand, K, V> {
        self.map.iter_mut(self.token)
    }
}

/// Extension trait to easily create ActiveHashMap from BrandedHashMap.
pub trait ActivateHashMap<'brand, K, V, S> {
    /// Activates the map with the given token, returning a handle that bundles them.
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveHashMap<'a, 'brand, K, V, S>;
}

impl<'brand, K, V, S> ActivateHashMap<'brand, K, V, S> for BrandedHashMap<'brand, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveHashMap<'a, 'brand, K, V, S> {
        ActiveHashMap::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedHashSet` and a mutable reference to a `GhostToken`.
pub struct ActiveHashSet<'a, 'brand, K, S> {
    set: &'a mut BrandedHashSet<'brand, K, S>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, S> ActiveHashSet<'a, 'brand, K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Creates a new active set handle.
    pub fn new(set: &'a mut BrandedHashSet<'brand, K, S>, token: &'a mut GhostToken<'brand>) -> Self {
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

    /// Returns `true` if the set contains the value.
    pub fn contains(&self, value: &K) -> bool {
        self.set.contains(value)
    }

    /// Returns `true` if the set contains the value (gated check).
    pub fn contains_gated(&self, value: &K) -> bool {
        self.set.contains_gated(self.token, value)
    }

    /// Inserts a value.
    pub fn insert(&mut self, value: K) -> bool {
        self.set.insert(value)
    }

    /// Removes a value.
    pub fn remove(&mut self, value: &K) -> bool {
        self.set.remove(value)
    }

    /// Iterates over elements.
    pub fn iter(&self) -> impl Iterator<Item = &K> + use<'_, 'brand, K, S> {
        self.set.iter()
    }
}

/// Extension trait to easily create ActiveHashSet from BrandedHashSet.
pub trait ActivateHashSet<'brand, K, S> {
    /// Activates the set with the given token, returning a handle that bundles them.
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveHashSet<'a, 'brand, K, S>;
}

impl<'brand, K, S> ActivateHashSet<'brand, K, S> for BrandedHashSet<'brand, K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveHashSet<'a, 'brand, K, S> {
        ActiveHashSet::new(self, token)
    }
}
