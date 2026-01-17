//! `BrandedBTreeMap` — a B-Tree map with token-gated values.
//!
//! This implementation uses GhostCell to protect values, allowing interior mutability
//! patterns where values can be mutated via a unique token even while holding a
//! shared reference to the map.

use crate::{GhostCell, GhostToken};
use std::borrow::Borrow;
use std::cmp::Ordering;

// B-Tree order parameters.
// B = 6 roughly corresponds to std::collections::BTreeMap logic but simplified.
const B: usize = 6;
// MIN_LEN is the minimum number of keys in a node (except root).
const MIN_LEN: usize = B - 1;
// MAX_LEN is the maximum number of keys in a node.
const MAX_LEN: usize = 2 * B - 1;

struct Node<'brand, K, V> {
    // Invariants:
    // keys.len() == vals.len()
    // children.len() == keys.len() + 1 (for internal nodes)
    // children.len() == 0 (for leaf nodes)
    keys: Vec<K>,
    vals: Vec<GhostCell<'brand, V>>,
    children: Vec<Box<Node<'brand, K, V>>>,
}

impl<'brand, K, V> Node<'brand, K, V> {
    fn new_leaf() -> Self {
        Self {
            keys: Vec::with_capacity(MAX_LEN),
            vals: Vec::with_capacity(MAX_LEN),
            children: Vec::new(),
        }
    }

    fn new_internal() -> Self {
        Self {
            keys: Vec::with_capacity(MAX_LEN),
            vals: Vec::with_capacity(MAX_LEN),
            children: Vec::with_capacity(MAX_LEN + 1),
        }
    }

    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    fn split_child(&mut self, index: usize) {
        let child = &mut self.children[index];

        let mut sibling = if child.is_leaf() {
            Node::new_leaf()
        } else {
            Node::new_internal()
        };

        // Move keys and values to sibling
        // child keys: [0..B-1] [B-1] [B..2B-1]
        //             Left     Mid   Right
        // Right part (B elements) goes to sibling.
        // Mid goes to parent.

        // split_off(at) returns elements starting from at.
        // We want elements from B onwards.
        // child.keys has 2*B-1 elements.
        // split_off(B) leaves 0..B in child (B elements) and returns B..2B-1 (B-1 elements).
        // Wait, MAX_LEN = 2*B - 1.
        // B=6. MAX_LEN=11.
        // Indices: 0..11.
        // Mid index is B-1 = 5.
        // Left: 0..5 (5 elements).
        // Mid: 5 (1 element).
        // Right: 6..11 (5 elements).

        // split_off(B) -> indices 6,7,8,9,10. Correct.
        let right_keys = child.keys.split_off(B);
        let right_vals = child.vals.split_off(B);

        // The element at B-1 is the median.
        let median_key = child.keys.pop().unwrap();
        let median_val = child.vals.pop().unwrap();

        sibling.keys = right_keys;
        sibling.vals = right_vals;

        // Move children if internal
        if !child.is_leaf() {
            // Children indices: 0..2B (0..12).
            // Split at B.
            // Left: 0..B (6 children). Right: B..2B (6 children).
            let right_children = child.children.split_off(B);
            sibling.children = right_children;
        }

        // Insert median into parent
        self.keys.insert(index, median_key);
        self.vals.insert(index, median_val);
        self.children.insert(index + 1, Box::new(sibling));
    }

    fn insert_non_full(&mut self, key: K, value: V) -> Option<V>
    where
        K: Ord,
    {
        match self.search_key(&key) {
            Ok(idx) => {
                // Key exists, replace value
                let old_val = std::mem::replace(&mut self.vals[idx], GhostCell::new(value));
                Some(old_val.into_inner())
            }
            Err(idx) => {
                if self.is_leaf() {
                    self.keys.insert(idx, key);
                    self.vals.insert(idx, GhostCell::new(value));
                    None
                } else {
                    // Recurse to child
                    // Check if child is full
                    if self.children[idx].keys.len() == MAX_LEN {
                        self.split_child(idx);
                        // After split, the key might go to the new child or stay in current child
                        // depending on comparison with the median key that moved up.
                        // The median key is now at node.keys[idx].
                        if key > self.keys[idx] {
                            // Go to the new sibling, which is at idx + 1
                            self.children[idx + 1].insert_non_full(key, value)
                        } else if key < self.keys[idx] {
                            self.children[idx].insert_non_full(key, value)
                        } else {
                            // Key matches the median key that moved up. Update value.
                            let old_val = std::mem::replace(&mut self.vals[idx], GhostCell::new(value));
                            Some(old_val.into_inner())
                        }
                    } else {
                        self.children[idx].insert_non_full(key, value)
                    }
                }
            }
        }
    }

    /// Search for a given key in the node.
    /// Returns:
    /// - Ok(index) if key is found at keys[index]
    /// - Err(index) if key is not found, index is where it should be inserted (child index)
    fn search_key<Q: ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        // Linear search for small B, binary search for large B.
        // For B=6, linear is likely faster or comparable, but binary is standard.
        // std uses linear for small arrays.
        // Let's use binary search for correctness and scalability if B changes.
        self.keys.binary_search_by(|k| k.borrow().cmp(key))
    }

    fn remove_key<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        match self.search_key(key) {
            Ok(idx) => {
                if self.is_leaf() {
                    self.keys.remove(idx);
                    let val = self.vals.remove(idx);
                    Some(val.into_inner())
                } else {
                    // Internal node: swap with successor (min of right child)
                    // We go to child at idx + 1
                    let maybe_succ = self.children[idx + 1].pop_min();

                    if let Some((s_key, s_val)) = maybe_succ {
                        self.keys[idx] = s_key;
                        let old_val = std::mem::replace(&mut self.vals[idx], GhostCell::new(s_val));
                        self.fix_empty_child(idx + 1);
                        Some(old_val.into_inner())
                    } else {
                        // Successor child is empty.
                        // We can just remove the key and the empty child.
                        self.keys.remove(idx);
                        let val = self.vals.remove(idx);
                        self.children.remove(idx + 1);
                        Some(val.into_inner())
                    }
                }
            }
            Err(idx) => {
                if self.is_leaf() {
                    None
                } else {
                    let res = self.children[idx].remove_key(key);
                    self.fix_empty_child(idx);
                    res
                }
            }
        }
    }

    fn pop_min(&mut self) -> Option<(K, V)> {
        if self.keys.is_empty() {
            if self.is_leaf() {
                return None;
            } else {
                let res = self.children[0].pop_min();
                self.fix_empty_child(0);
                if res.is_some() {
                    return res;
                }
                // Child 0 is empty, but we are internal and have no keys?
                // This shouldn't happen if invariants hold (internal node with 0 keys has 1 child).
                // If keys is empty, we must have exactly 1 child.
                // If that child returned None, it means the whole subtree is empty.
                return None;
            }
        }

        if self.is_leaf() {
            let key = self.keys.remove(0);
            let val = self.vals.remove(0);
            Some((key, val.into_inner()))
        } else {
            let res = self.children[0].pop_min();
            self.fix_empty_child(0);

            if res.is_none() {
                // Child 0 has no keys. So the min key is our keys[0].
                // We remove keys[0] and merge empty child 0 with child 1?
                // Actually, if child 0 is empty, we can just drop it.
                // keys[0] corresponds to separator between children[0] and children[1].
                let key = self.keys.remove(0);
                let val = self.vals.remove(0);
                self.children.remove(0);
                Some((key, val.into_inner()))
            } else {
                res
            }
        }
    }

    fn fix_empty_child(&mut self, index: usize) {
        if self.children[index].keys.is_empty() {
             // If child is internal and empty, it has 1 child (because of invariants).
             // We can pull that child up.
             if !self.children[index].is_leaf() {
                 let mut child = self.children.remove(index);
                 let grandchild = child.children.pop().unwrap();
                 self.children.insert(index, grandchild);
             }
             // If child is leaf and empty, we leave it as empty leaf.
             // It still acts as a valid child pointer.
        }
    }
}

/// A B-Tree map with token-gated values.
pub struct BrandedBTreeMap<'brand, K, V> {
    root: Option<Box<Node<'brand, K, V>>>,
    len: usize,
}

impl<'brand, K, V> BrandedBTreeMap<'brand, K, V> {
    /// Creates an empty map.
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
        }
    }

    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the map contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'brand, K, V> BrandedBTreeMap<'brand, K, V>
where
    K: Ord,
{
    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<'a, Q: ?Sized>(&'a self, token: &'a GhostToken<'brand>, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut node = self.root.as_ref()?;
        loop {
            match node.search_key(key) {
                Ok(idx) => {
                    // Key found
                    return Some(node.vals[idx].borrow(token));
                }
                Err(idx) => {
                    // Key not found, go to child
                    if node.is_leaf() {
                        return None;
                    }
                    node = &node.children[idx];
                }
            }
        }
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// Note that this takes `&self` (shared reference) to the map, but `&mut GhostToken` (exclusive token).
    /// This allows mutation of values without needing exclusive access to the map structure.
    pub fn get_mut<'a, Q: ?Sized>(&'a self, token: &'a mut GhostToken<'brand>, key: &Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut node = self.root.as_ref()?;
        loop {
            match node.search_key(key) {
                Ok(idx) => {
                    // Key found
                    return Some(node.vals[idx].borrow_mut(token));
                }
                Err(idx) => {
                    // Key not found, go to child
                    if node.is_leaf() {
                        return None;
                    }
                    node = &node.children[idx];
                }
            }
        }
    }

    /// Returns true if the map contains a value for the specified key.
    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut node = match self.root.as_ref() {
            Some(n) => n,
            None => return false,
        };
        loop {
            match node.search_key(key) {
                Ok(_) => return true,
                Err(idx) => {
                    if node.is_leaf() {
                        return false;
                    }
                    node = &node.children[idx];
                }
            }
        }
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.root.is_none() {
            let mut root = Node::new_leaf();
            root.keys.push(key);
            root.vals.push(GhostCell::new(value));
            self.root = Some(Box::new(root));
            self.len += 1;
            return None;
        }

        // Check if root is full
        if self.root.as_ref().unwrap().keys.len() == MAX_LEN {
            let mut new_root = Node::new_internal();
            // Move old root to be first child of new root
            let old_root = self.root.take().unwrap();
            new_root.children.push(old_root);

            // Split the old root (which is now child 0 of new_root)
            new_root.split_child(0);

            self.root = Some(Box::new(new_root));

            // Now insert into the non-full root
            let res = self.root.as_mut().unwrap().insert_non_full(key, value);
            if res.is_none() {
                self.len += 1;
            }
            res
        } else {
            let root = self.root.as_mut().unwrap();
            let res = root.insert_non_full(key, value);
            if res.is_none() {
                self.len += 1;
            }
            res
        }
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        if self.root.is_none() {
            return None;
        }

        let root = self.root.as_mut().unwrap();
        let res = root.remove_key(key);

        if res.is_some() {
            self.len -= 1;
        }

        // Check if root became empty
        if self.root.as_ref().unwrap().keys.is_empty() {
            let root = self.root.as_mut().unwrap();
             if !root.is_leaf() {
                 // Root is empty internal node. It must have 1 child.
                 // Promote child to root.
                 let child = root.children.pop().unwrap();
                 self.root = Some(child);
             } else {
                 // Root is empty leaf.
                 self.root = None;
             }
        }

        res
    }

    /// Returns an iterator over the map.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> Iter<'a, 'brand, K, V> {
        let mut iter = Iter {
            stack: Vec::new(),
            token,
            len: self.len,
        };
        if let Some(root) = &self.root {
            iter.push_leftmost(root);
        }
        iter
    }

    /// Applies `f` to all entries in the map, allowing mutation of values.
    ///
    /// This is the canonical safe pattern for exclusive iteration with GhostCell:
    /// each `&mut V` is scoped to one callback invocation, preserving token linearity.
    pub fn for_each_mut<F>(&self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&K, &mut V),
    {
         if let Some(root) = &self.root {
             Self::for_each_node(root, token, &mut f);
         }
    }

    fn for_each_node<F>(node: &Node<'brand, K, V>, token: &mut GhostToken<'brand>, f: &mut F)
    where
        F: FnMut(&K, &mut V),
    {
         for i in 0..node.keys.len() {
             if !node.is_leaf() {
                 Self::for_each_node(&node.children[i], token, f);
             }
             let key = &node.keys[i];
             let val = node.vals[i].borrow_mut(token);
             f(key, val);
         }
         if !node.is_leaf() {
             Self::for_each_node(&node.children[node.keys.len()], token, f);
         }
    }
}

pub struct Iter<'a, 'brand, K, V> {
    stack: Vec<(&'a Node<'brand, K, V>, usize)>,
    token: &'a GhostToken<'brand>,
    len: usize,
}

impl<'a, 'brand, K, V> Iter<'a, 'brand, K, V> {
    fn push_leftmost(&mut self, mut node: &'a Node<'brand, K, V>) {
        loop {
            self.stack.push((node, 0));
            if node.is_leaf() {
                break;
            }
            node = &node.children[0];
        }
    }
}

impl<'a, 'brand, K, V> Iterator for Iter<'a, 'brand, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let should_pop;
            let item_to_yield;
            let child_to_push;

            {
                let (node, idx) = match self.stack.last_mut() {
                    Some(pair) => pair,
                    None => return None,
                };

                if *idx < node.keys.len() {
                    let node_ref = *node;
                    let key = &node_ref.keys[*idx];
                    let val = node_ref.vals[*idx].borrow(self.token);
                    *idx += 1;

                    if !node_ref.is_leaf() {
                         child_to_push = Some(&node_ref.children[*idx]);
                    } else {
                         child_to_push = None;
                    }
                    item_to_yield = Some((key, val));
                    should_pop = false;
                } else {
                    should_pop = true;
                    item_to_yield = None;
                    child_to_push = None;
                }
            }

            if should_pop {
                self.stack.pop();
                continue;
            }

            if let Some(child) = child_to_push {
                self.push_leftmost(child);
            }

            return item_to_yield;
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

pub struct IntoIter<'brand, K, V> {
    stack: Vec<Box<Node<'brand, K, V>>>,
    len: usize,
}

impl<'brand, K, V> IntoIter<'brand, K, V> {
    fn push_leftmost(&mut self, mut node: Box<Node<'brand, K, V>>) {
        while !node.is_leaf() {
            let child = node.children.remove(0);
            self.stack.push(node);
            node = child;
        }
        self.stack.push(node);
    }
}

impl<'brand, K, V> Iterator for IntoIter<'brand, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut node = self.stack.pop()?;

            if node.is_leaf() {
                if !node.keys.is_empty() {
                    let k = node.keys.remove(0);
                    let v = node.vals.remove(0);
                    self.len -= 1;
                    if !node.keys.is_empty() {
                        self.stack.push(node);
                    }
                    return Some((k, v.into_inner()));
                } else {
                    continue;
                }
            } else {
                // Internal node
                if !node.keys.is_empty() {
                    let k = node.keys.remove(0);
                    let v = node.vals.remove(0);
                    self.len -= 1;
                    let child = node.children.remove(0);

                    self.stack.push(node);
                    self.push_leftmost(child);

                    return Some((k, v.into_inner()));
                } else {
                    // No keys left, but one child left
                    let child = node.children.remove(0);
                    self.push_leftmost(child);
                    continue;
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'brand, K, V> IntoIterator for BrandedBTreeMap<'brand, K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<'brand, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        let mut iter = IntoIter {
            stack: Vec::new(),
            len: self.len,
        };
        if let Some(root) = self.root {
            iter.push_leftmost(root);
        }
        iter
    }
}

impl<'brand, K, V> Default for BrandedBTreeMap<'brand, K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, K, V> crate::collections::BrandedCollection<'brand> for BrandedBTreeMap<'brand, K, V> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_basic_insert_get() {
        GhostToken::new(|mut token| {
            let mut map = BrandedBTreeMap::new();
            assert!(map.is_empty());

            map.insert(1, 10);
            assert_eq!(map.len(), 1);
            assert_eq!(*map.get(&token, &1).unwrap(), 10);

            map.insert(2, 20);
            assert_eq!(*map.get(&token, &2).unwrap(), 20);

            // Update
            assert_eq!(map.insert(1, 11), Some(10));
            assert_eq!(*map.get(&token, &1).unwrap(), 11);
        });
    }

    #[test]
    fn test_insert_split() {
        GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            // Insert enough elements to cause splits
            // MAX_LEN = 11.

            for i in 0..100 {
                map.insert(i, i * 10);
            }

            assert_eq!(map.len(), 100);
            for i in 0..100 {
                 assert_eq!(*map.get(&token, &i).unwrap(), i * 10);
            }
        });
    }

    #[test]
    fn test_contains_key() {
        GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            map.insert("a", 1);
            assert!(map.contains_key(&"a"));
            assert!(!map.contains_key(&"b"));
        });
    }

    #[test]
    fn test_get_mut_shared_self() {
        GhostToken::new(|mut token| {
            let mut map = BrandedBTreeMap::new();
            map.insert(1, 100);

            // Borrow map immutably
            let map_ref = &map;

            // Mutate value via mutable token
            if let Some(val) = map_ref.get_mut(&mut token, &1) {
                *val += 1;
            }

            assert_eq!(*map.get(&token, &1).unwrap(), 101);
        });
    }

    #[test]
    fn test_remove() {
        GhostToken::new(|mut token| {
            let mut map = BrandedBTreeMap::new();
            map.insert(1, 10);
            map.insert(2, 20);
            map.insert(3, 30);

            assert_eq!(map.len(), 3);

            assert_eq!(map.remove(&2), Some(20));
            assert_eq!(map.len(), 2);
            assert_eq!(map.get(&token, &2), None);

            assert_eq!(map.remove(&1), Some(10));
            assert_eq!(map.len(), 1);
            assert_eq!(map.get(&token, &3), Some(&30));

            assert_eq!(map.remove(&3), Some(30));
            assert!(map.is_empty());
        });
    }

    #[test]
    fn test_remove_complex() {
         GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            // Insert enough to create internal nodes
            for i in 0..100 {
                map.insert(i, i * 10);
            }

            // Remove some elements
            // Remove internal node key (median of root likely around 50?)
            assert_eq!(map.remove(&50), Some(500));
            assert_eq!(map.len(), 99);
            assert!(!map.contains_key(&50));

            // Check consistency
            for i in 0..100 {
                if i != 50 {
                     assert_eq!(*map.get(&token, &i).unwrap(), i * 10);
                }
            }

            // Remove everything
            for i in 0..100 {
                if i != 50 {
                    map.remove(&i);
                }
            }
            assert!(map.is_empty());
         });
    }

    #[test]
    fn test_insert_split_update() {
        GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            // MAX_LEN = 11. B=6.
            // Insert enough to cause splits.
            for i in 0..20 {
                map.insert(i, i * 10);
            }
            // Update all keys.
            for i in 0..20 {
                map.insert(i, i * 100);
            }

            for i in 0..20 {
                assert_eq!(*map.get(&token, &i).unwrap(), i * 100);
            }
        });
    }

    #[test]
    fn test_iterators() {
         GhostToken::new(|mut token| {
            let mut map = BrandedBTreeMap::new();
            for i in 0..10 {
                map.insert(i, i * 10);
            }

            // Iter
            let mut count = 0;
            for (k, v) in map.iter(&token) {
                assert_eq!(*v, *k * 10);
                count += 1;
            }
            assert_eq!(count, 10);

            // ForEachMut
            map.for_each_mut(&mut token, |_, v| {
                *v += 1;
            });

            assert_eq!(*map.get(&token, &0).unwrap(), 1);

            // IntoIter
            let mut items = Vec::new();
            for (k, v) in map {
                items.push((k, v));
            }
            items.sort();
            assert_eq!(items.len(), 10);
            assert_eq!(items[0], (0, 1));
         });
    }
}
