//! `BrandedSkipList` — a probabilistic data structure with token-gated values.
//!
//! This implementation uses a "structure-of-arrays" approach with two `BrandedVec`s:
//! one for node data and one for the forward pointers (links). This ensures compact
//! memory layout and cache efficiency, avoiding per-node allocations and `Box` overhead.
//!
//! Access is controlled via `GhostToken` to ensure safety while allowing interior mutability.

use crate::{GhostCell, GhostToken, BrandedVec};
use std::borrow::Borrow;
use std::cmp::Ordering;
use crate::collections::{BrandedCollection, ZeroCopyMapOps};

const MAX_LEVEL: usize = 16;

/// Simple Xorshift RNG for level generation.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 0xDEAD_BEEF_CAFE } else { seed } }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

struct NodeData<K, V> {
    key: K,
    val: V,
    link_offset: usize,
    level: usize,
}

/// A SkipList map with token-gated values.
pub struct BrandedSkipList<'brand, K, V> {
    nodes: BrandedVec<'brand, NodeData<K, V>>,
    links: BrandedVec<'brand, Option<usize>>, // Stores indices into `nodes`
    head_links: [Option<usize>; MAX_LEVEL],
    len: usize,
    max_level: usize, // Current max level in the list (1-based, or 0 if empty)
    rng: XorShift64,
}

impl<'brand, K, V> BrandedSkipList<'brand, K, V> {
    /// Creates a new empty SkipList.
    pub fn new() -> Self {
        Self {
            nodes: BrandedVec::new(),
            links: BrandedVec::new(),
            head_links: [None; MAX_LEVEL],
            len: 0,
            max_level: 0,
            rng: XorShift64::new(0x1234_5678),
        }
    }

    /// Creates a new SkipList with a specific RNG seed.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            nodes: BrandedVec::new(),
            links: BrandedVec::new(),
            head_links: [None; MAX_LEVEL],
            len: 0,
            max_level: 0,
            rng: XorShift64::new(seed),
        }
    }

    fn random_level(&mut self) -> usize {
        let mut level = 1;
        while level < MAX_LEVEL && (self.rng.next() % 2) == 0 {
            level += 1;
        }
        level
    }
}

impl<'brand, K, V> BrandedSkipList<'brand, K, V>
where
    K: Ord,
{
    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<'a, Q: ?Sized>(&'a self, token: &'a GhostToken<'brand>, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.find_entry(token, key).map(|(_, v)| v)
    }

    /// Internal helper to find an entry.
    ///
    /// Optimized with `unsafe` unchecked access because indices are managed internally
    /// and guaranteed to be valid by `insert`.
    fn find_entry<'a, Q: ?Sized>(&'a self, token: &'a GhostToken<'brand>, key: &Q) -> Option<(&'a K, &'a V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut curr: Option<usize> = None; // None represents head
        let mut level = self.max_level.saturating_sub(1);

        if self.max_level == 0 {
            return None;
        }

        loop {
            // Determine next pointer index
            // SAFETY:
            // - `curr` comes from valid internal links.
            // - `offset` calculation is bounded by `node.level` which matches link allocation.
            let next_idx_opt = if let Some(c_idx) = curr {
                unsafe {
                    let node = self.nodes.get_unchecked(token, c_idx);
                    // Assumption: node.level > level, so offset is valid
                    let offset = node.link_offset + level;
                    *self.links.get_unchecked(token, offset)
                }
            } else {
                self.head_links[level]
            };

            if let Some(next_idx) = next_idx_opt {
                // SAFETY: `next_idx` comes from valid links.
                unsafe {
                    let next_node = self.nodes.get_unchecked(token, next_idx);
                    match next_node.key.borrow().cmp(key) {
                        Ordering::Less => {
                            curr = Some(next_idx);
                            continue; // Keep moving forward at same level
                        }
                        Ordering::Equal => {
                            return Some((&next_node.key, &next_node.val));
                        }
                        Ordering::Greater => {
                            // Next is too big, go down
                        }
                    }
                }
            }

            // Move down
            if level == 0 {
                break;
            }
            level -= 1;
        }
        None
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut<'a, Q: ?Sized>(&'a self, token: &'a mut GhostToken<'brand>, key: &Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
         let idx = self.find_index(&*token, key)?;
         // SAFETY: idx found by find_index is valid.
         unsafe {
             let node = self.nodes.get_unchecked_mut(token, idx);
             Some(&mut node.val)
         }
    }

    /// Helper to find index of a key.
    ///
    /// Optimized with `unsafe`.
    fn find_index<Q: ?Sized>(&self, token: &GhostToken<'brand>, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut curr: Option<usize> = None;
        let mut level = self.max_level.saturating_sub(1);

        if self.max_level == 0 {
            return None;
        }

        loop {
            let next_idx_opt = self.get_next_unchecked(token, curr, level);

            if let Some(next_idx) = next_idx_opt {
                // SAFETY: next_idx valid
                unsafe {
                    let next_node = self.nodes.get_unchecked(token, next_idx);
                    match next_node.key.borrow().cmp(key) {
                        Ordering::Less => {
                            curr = Some(next_idx);
                            continue;
                        }
                        Ordering::Equal => return Some(next_idx),
                        Ordering::Greater => {}
                    }
                }
            }

            if level == 0 {
                break;
            }
            level -= 1;
        }
        None
    }

    // Unsafe version for hot paths
    fn get_next_unchecked(&self, token: &GhostToken<'brand>, curr: Option<usize>, level: usize) -> Option<usize> {
        if let Some(c_idx) = curr {
            // SAFETY: Caller guarantees curr and level are valid
            unsafe {
                let node = self.nodes.get_unchecked(token, c_idx);
                let offset = node.link_offset + level;
                *self.links.get_unchecked(token, offset)
            }
        } else {
            self.head_links[level]
        }
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V> {
        // First check if key exists to update it
        // We use shared token access for find to avoid exclusive borrow issues until we need to mutate
        if let Some(idx) = self.find_index(&*token, &key) {
             // SAFETY: idx is valid
             unsafe {
                 let node = self.nodes.get_unchecked_mut(token, idx);
                 let old = std::mem::replace(&mut node.val, value);
                 return Some(old);
             }
        }

        // Need to insert.
        // Find predecessors.
        let mut update = [None; MAX_LEVEL];
        let mut curr: Option<usize> = None;
        let mut level = self.max_level.saturating_sub(1);

        // If list is not empty, traverse
        if self.max_level > 0 {
            loop {
                // Use unsafe unchecked for performance
                let next_idx_opt = self.get_next_unchecked(token, curr, level);

                if let Some(next_idx) = next_idx_opt {
                    unsafe {
                        let next_node = self.nodes.get_unchecked(token, next_idx);
                        if next_node.key < key {
                            curr = Some(next_idx);
                            continue;
                        }
                    }
                }

                update[level] = curr;
                if level == 0 {
                    break;
                }
                level -= 1;
            }
        }

        let new_level = self.random_level();
        if new_level > self.max_level {
            for i in self.max_level..new_level {
                update[i] = None; // Head
            }
            self.max_level = new_level;
        }

        // Create new node
        let link_offset = self.links.len();
        let node_idx = self.nodes.len();

        // Push links (placeholders initially)
        for _ in 0..new_level {
            self.links.push(None);
        }

        // Push node
        self.nodes.push(NodeData {
            key,
            val: value,
            link_offset,
            level: new_level,
        });

        // Update pointers
        for i in 0..new_level {
            let pred_idx = update[i];

            // new_node.next[i] = pred.next[i]
            let old_next = if let Some(p_idx) = pred_idx {
                // Safe here because we are modifying, not in hot loop
                let pred_node = self.nodes.get(token, p_idx).unwrap();
                *self.links.get(token, pred_node.link_offset + i).unwrap()
            } else {
                self.head_links[i]
            };

            // Update new node link
            // We just pushed these, so they are valid.
            unsafe {
                 *self.links.get_unchecked_mut(token, link_offset + i) = old_next;
            }

            // pred.next[i] = new_node
            if let Some(p_idx) = pred_idx {
                let pred_node = self.nodes.get(token, p_idx).unwrap();
                // Safe or unsafe? Safe is fine here, insertion is dominated by traversal/allocation.
                *self.links.get_mut(token, pred_node.link_offset + i).unwrap() = Some(node_idx);
            } else {
                self.head_links[i] = Some(node_idx);
            }
        }

        self.len += 1;
        None
    }

    /// Returns true if the map contains the key.
    pub fn contains_key<Q: ?Sized>(&self, token: &GhostToken<'brand>, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.find_index(token, key).is_some()
    }
}

impl<'brand, K, V> BrandedCollection<'brand> for BrandedSkipList<'brand, K, V> {
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl<'brand, K, V> ZeroCopyMapOps<'brand, K, V> for BrandedSkipList<'brand, K, V> {
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<(&'a K, &'a V)>
    where
        F: Fn(&K, &V) -> bool,
    {
        // Iterating efficiently involves just walking level 0
        let mut curr = self.head_links[0];
        while let Some(idx) = curr {
            unsafe {
                let node = self.nodes.get_unchecked(token, idx);
                if f(&node.key, &node.val) {
                    return Some((&node.key, &node.val));
                }
                // Move next at level 0
                let offset = node.link_offset; // level 0 is at offset
                curr = *self.links.get_unchecked(token, offset);
            }
        }
        None
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        self.find_ref(token, f).is_some()
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
         let mut curr = self.head_links[0];
        while let Some(idx) = curr {
            unsafe {
                let node = self.nodes.get_unchecked(token, idx);
                if !f(&node.key, &node.val) {
                    return false;
                }
                let offset = node.link_offset;
                curr = *self.links.get_unchecked(token, offset);
            }
        }
        true
    }
}

// Iterator implementation
pub struct Iter<'a, 'brand, K, V> {
    list: &'a BrandedSkipList<'brand, K, V>,
    token: &'a GhostToken<'brand>,
    curr: Option<usize>,
}

impl<'a, 'brand, K, V> Iterator for Iter<'a, 'brand, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.curr?;
        unsafe {
            let node = self.list.nodes.get_unchecked(self.token, idx);

            // Advance
            let offset = node.link_offset;
            self.curr = *self.list.links.get_unchecked(self.token, offset);

            Some((&node.key, &node.val))
        }
    }
}

// Mutable Iterator implementation
pub struct IterMut<'a, 'brand, K, V> {
    list: &'a BrandedSkipList<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
    curr: Option<usize>,
}

impl<'a, 'brand, K, V> Iterator for IterMut<'a, 'brand, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.curr?;

        // We need to return mutable reference.
        // But we also need to advance self.curr using the list.
        // We can't hold mutable borrow of list and read from it?
        // Wait, iter_mut usually consumes the token or uses a split borrow.
        // Here we have `&'a mut GhostToken`.
        // To be safe and satisfy borrow checker, we need to leverage unsafe to extend lifetime
        // OR rely on the fact that we are yielding disjoint mutable references (which is true for different nodes).
        // Since `BrandedVec` doesn't support random access mutable iterators easily without consuming token,
        // we have to be careful.

        // However, standard pattern is `nodes.get_mut`. But we iterate sequentially.
        // We can get the node.

        // This is tricky safely.
        // We can read `next` BEFORE borrowing `curr` mutably?
        // Yes.

        unsafe {
             // 1. Get next pointer using SHARED access (we have exclusive token, but can downgrade)
             let node_shared = self.list.nodes.get_unchecked(&*self.token, idx);
             let offset = node_shared.link_offset;
             let next_curr = *self.list.links.get_unchecked(&*self.token, offset);

             // 2. Get MUTABLE reference to current
             // We must ensure we don't alias.
             // Since we advance `curr` and never look back, and SkipList is acyclic,
             // we won't visit the same node twice.
             // We can use `get_unchecked_mut`.
             // But the lifetime of returned `&mut V` must be 'a.
             // `get_unchecked_mut` takes `&'b mut Token` and returns `&'b mut T`.
             // We need to transmute the lifetime to 'a.
             // This is sound because each node is unique.

             let node_mut = self.list.nodes.get_unchecked_mut(self.token, idx);

             // Update state
             self.curr = next_curr;

             // Extend lifetime of return value to 'a
             let key_ptr = &node_mut.key as *const K;
             let val_ptr = &mut node_mut.val as *mut V;

             Some((&*key_ptr, &mut *val_ptr))
        }
    }
}


impl<'brand, K, V> BrandedSkipList<'brand, K, V> {
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> Iter<'a, 'brand, K, V> {
        Iter {
            list: self,
            token,
            curr: self.head_links[0],
        }
    }

    pub fn iter_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> IterMut<'a, 'brand, K, V> {
        IterMut {
            list: self,
            curr: self.head_links[0],
            token,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_skip_list_basic() {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            assert!(list.is_empty());

            list.insert(&mut token, 1, 10);
            assert_eq!(list.len(), 1);
            assert_eq!(*list.get(&token, &1).unwrap(), 10);

            list.insert(&mut token, 2, 20);
            assert_eq!(*list.get(&token, &2).unwrap(), 20);

            assert!(list.contains_key(&token, &1));
            assert!(!list.contains_key(&token, &3));

            *list.get_mut(&mut token, &1).unwrap() = 15;
            assert_eq!(*list.get(&token, &1).unwrap(), 15);
        });
    }

    #[test]
    fn test_skip_list_iter() {
         GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            for i in 0..10 {
                list.insert(&mut token, i, i * 10);
            }

            let mut count = 0;
            for (k, v) in list.iter(&token) {
                assert_eq!(*v, *k * 10);
                count += 1;
            }
            assert_eq!(count, 10);
         });
    }

    #[test]
    fn test_skip_list_iter_mut() {
         GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            for i in 0..10 {
                list.insert(&mut token, i, i * 10);
            }

            for (_, v) in list.iter_mut(&mut token) {
                *v += 1;
            }

            for i in 0..10 {
                assert_eq!(*list.get(&token, &i).unwrap(), i * 10 + 1);
            }
         });
    }

    #[test]
    fn test_skip_list_large() {
         GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::with_seed(12345);
            for i in 0..100 {
                list.insert(&mut token, i, i);
            }

            assert_eq!(list.len(), 100);
            for i in 0..100 {
                 assert_eq!(*list.get(&token, &i).unwrap(), i);
            }
         });
    }
}
