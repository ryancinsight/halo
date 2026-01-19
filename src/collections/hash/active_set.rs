//! `ActiveHashSet` — a BrandedHashSet bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope.

use super::BrandedHashSet;
use crate::GhostToken;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

/// A wrapper around a mutable reference to a `BrandedHashSet` and a mutable reference to a `GhostToken`.
pub struct ActiveHashSet<'a, 'brand, K, S = RandomState> {
    set: &'a mut BrandedHashSet<'brand, K, S>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, S> ActiveHashSet<'a, 'brand, K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Creates a new active set handle.
    pub fn new(
        set: &'a mut BrandedHashSet<'brand, K, S>,
        token: &'a mut GhostToken<'brand>,
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

    /// Inserts a value. Returns `true` if it was not already present.
    pub fn insert(&mut self, value: K) -> bool {
        self.set.insert(value)
    }

    /// Removes a value. Returns `true` if it was present.
    pub fn remove(&mut self, value: &K) -> bool {
        self.set.remove(value)
    }

    /// Returns `true` if the set contains the value.
    pub fn contains(&self, value: &K) -> bool {
        self.set.contains(value)
    }

    /// Iterates over all values.
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.set.iter()
    }
}

/// Extension trait to easily create ActiveHashSet from BrandedHashSet.
pub trait ActivateHashSet<'brand, K, S> {
    /// Activates the set with the given token.
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveHashSet<'a, 'brand, K, S>;
}

impl<'brand, K, S> ActivateHashSet<'brand, K, S> for BrandedHashSet<'brand, K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveHashSet<'a, 'brand, K, S> {
        ActiveHashSet::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_hash_set_workflow() {
        GhostToken::new(|mut token| {
            let mut set = BrandedHashSet::new();

            {
                let mut active = set.activate(&mut token);
                active.insert(1);
                active.insert(2);

                assert_eq!(active.len(), 2);
                assert!(active.contains(&1));
                assert!(!active.contains(&3));

                active.remove(&1);
                assert!(!active.contains(&1));
            }

            assert_eq!(set.len(), 1);
        });
    }
}
