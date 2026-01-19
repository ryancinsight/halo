//! `ActiveBPlusTree` — a convenient wrapper for `BrandedBPlusTree`.

use super::bplus_tree::{BrandedBPlusTree, Iter};
use crate::GhostToken;

pub struct ActiveBPlusTree<'a, 'brand, K, V> {
    tree: &'a mut BrandedBPlusTree<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand, K, V> ActiveBPlusTree<'a, 'brand, K, V> {
    pub fn new(
        tree: &'a mut BrandedBPlusTree<'brand, K, V>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { tree, token }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        K: Ord + Clone,
    {
        self.tree.insert(self.token, key, value)
    }

    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        self.tree.get(self.token, key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V>
    where
        K: Ord,
    {
        self.tree.get_mut(self.token, key)
    }

    pub fn len(&self) -> usize {
        self.tree.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    pub fn iter<'b>(&'b self) -> Iter<'b, 'brand, K, V> {
        self.tree.iter(self.token)
    }
}

impl<'brand, K, V> BrandedBPlusTree<'brand, K, V> {
    pub fn activate<'a>(
        &'a mut self,
        token: &'a mut GhostToken<'brand>,
    ) -> ActiveBPlusTree<'a, 'brand, K, V> {
        ActiveBPlusTree::new(self, token)
    }
}
