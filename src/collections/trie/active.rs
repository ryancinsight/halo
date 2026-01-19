//! Active wrappers for Radix Trie collections.
//!
//! These wrappers bundle the collection with a mutable reference to the `GhostToken`,
//! reducing token redundancy in API calls.

use crate::GhostToken;
use super::{BrandedRadixTrieMap, BrandedRadixTrieSet};
use crate::collections::trie::iter::Iter;

/// Active wrapper for `BrandedRadixTrieMap`.
pub struct ActiveRadixTrieMap<'a, 'brand, K, V> {
    map: &'a mut BrandedRadixTrieMap<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, V> ActiveRadixTrieMap<'a, 'brand, K, V> {
    pub fn new(map: &'a mut BrandedRadixTrieMap<'brand, K, V>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { map, token }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<'a, 'brand, K, V> ActiveRadixTrieMap<'a, 'brand, K, V>
where K: AsRef<[u8]>
{
    pub fn get(&self, key: K) -> Option<&V> {
        self.map.get(self.token, key)
    }

    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        self.map.get_mut(self.token, key)
    }

    pub fn contains_key(&self, key: K) -> bool {
        self.map.get(self.token, key).is_some()
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.map.insert(self.token, key, value)
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        self.map.remove(self.token, key)
    }

    pub fn for_each<F>(&self, f: F)
    where F: FnMut(&[u8], &V)
    {
        self.map.for_each(self.token, f)
    }

    pub fn iter(&self) -> Iter<'_, 'brand, K, V> {
        Iter::new(self.map, self.token)
    }
}

pub trait ActivateRadixTrieMap<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveRadixTrieMap<'a, 'brand, K, V>;
}

impl<'brand, K, V> ActivateRadixTrieMap<'brand, K, V> for BrandedRadixTrieMap<'brand, K, V> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveRadixTrieMap<'a, 'brand, K, V> {
        ActiveRadixTrieMap::new(self, token)
    }
}

/// Active wrapper for `BrandedRadixTrieSet`.
pub struct ActiveRadixTrieSet<'a, 'brand, T> {
    set: &'a mut BrandedRadixTrieSet<'brand, T>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, T> ActiveRadixTrieSet<'a, 'brand, T> {
    pub fn new(set: &'a mut BrandedRadixTrieSet<'brand, T>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { set, token }
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn clear(&mut self) {
        self.set.clear();
    }

    pub fn for_each<F>(&self, f: F)
    where F: FnMut(&[u8])
    {
        self.set.for_each(self.token, f)
    }
}

impl<'a, 'brand, T> ActiveRadixTrieSet<'a, 'brand, T>
where T: AsRef<[u8]>
{
    pub fn insert(&mut self, value: T) -> bool {
        self.set.insert(self.token, value)
    }

    pub fn contains(&self, value: T) -> bool {
        self.set.contains(self.token, value)
    }

    pub fn remove(&mut self, value: T) -> bool {
        self.set.remove(self.token, value)
    }
}

pub trait ActivateRadixTrieSet<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveRadixTrieSet<'a, 'brand, T>;
}

impl<'brand, T> ActivateRadixTrieSet<'brand, T> for BrandedRadixTrieSet<'brand, T> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveRadixTrieSet<'a, 'brand, T> {
        ActiveRadixTrieSet::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_trie_map() {
        GhostToken::new(|mut token| {
            let mut map = BrandedRadixTrieMap::new();
            {
                let mut active = map.activate(&mut token);
                active.insert("hello", 1);
                active.insert("world", 2);

                assert_eq!(active.get("hello"), Some(&1));
                *active.get_mut("hello").unwrap() += 10;

                active.remove("world");
            }
            assert_eq!(map.get(&token, "hello"), Some(&11));
            assert_eq!(map.get(&token, "world"), None);
        });
    }

    #[test]
    fn test_active_trie_set() {
        GhostToken::new(|mut token| {
            let mut set = BrandedRadixTrieSet::new();
            {
                let mut active = set.activate(&mut token);
                active.insert("hello");
                active.insert("world");

                assert!(active.contains("hello"));
                active.remove("hello");
            }
            assert!(!set.contains(&token, "hello"));
            assert!(set.contains(&token, "world"));
        });
    }
}
