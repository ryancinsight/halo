//! `ActiveBPlusTree` — a convenient wrapper for `BrandedBPlusTree`.

use super::bplus_tree::BrandedBPlusTree;
use crate::token::traits::GhostBorrowMut;

pub struct ActiveBPlusTree<'a, 'brand, K, V, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    tree: &'a mut BrandedBPlusTree<'brand, K, V>,
    token: &'a mut Token,
}

impl<'a, 'brand, K, V, Token> ActiveBPlusTree<'a, 'brand, K, V, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    pub fn new(tree: &'a mut BrandedBPlusTree<'brand, K, V>, token: &'a mut Token) -> Self {
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

    pub fn iter<'b>(&'b self) -> impl Iterator<Item = (&'b K, &'b V)> + use<'b, 'brand, K, V, Token> {
        self.tree.iter(self.token)
    }
}

pub trait ActivateBPlusTree<'brand, K, V> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveBPlusTree<'a, 'brand, K, V, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, K, V> ActivateBPlusTree<'brand, K, V> for BrandedBPlusTree<'brand, K, V> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveBPlusTree<'a, 'brand, K, V, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveBPlusTree::new(self, token)
    }
}
