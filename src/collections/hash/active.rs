//! `ActiveHashMap` — a BrandedHashMap bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope. By holding the token exclusively, it can expose a standard `HashMap`-like
//! API without requiring the token as an argument for every call.

use crate::GhostToken;
use super::BrandedHashMap;
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
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.map.values(self.token)
    }

    /// Iterates over key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.keys().zip(self.map.values(self.token))
    }

    /// Iterates over entries mutably.
    ///
    /// This uses `for_each_mut` under the hood because returning a mutable iterator
    /// is not safely expressible with `GhostCell` without unsafe workarounds or
    /// restrictive lifetimes that `Iterator` trait doesn't support easily (streaming iterator).
    pub fn for_each_mut<F>(&mut self, f: F)
    where
        F: FnMut(&K, &mut V),
    {
        self.map.for_each_mut(self.token, f)
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
