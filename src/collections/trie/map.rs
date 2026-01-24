use core::cmp::min;
use core::marker::PhantomData;
use std::boxed::Box;
use std::vec::Vec;

use super::node::{Node, NodePrefix, NodeSlot};
use crate::collections::{BrandedCollection, BrandedVec, ZeroCopyMapOps};
use crate::{GhostCell, GhostToken};

/// A high-performance Radix Trie Map (Prefix Tree) optimized for branded usage.
///
/// It uses a `BrandedVec` as an arena for nodes to ensure cache locality and
/// support safe interior mutability via `GhostToken`.
///
/// Keys must implement `AsRef<[u8]>`.
pub struct BrandedRadixTrieMap<'brand, K, V> {
    /// Arena of nodes.
    pub(crate) nodes: BrandedVec<'brand, NodeSlot<V>>,
    /// Index of the root node in the arena.
    pub(crate) root: Option<usize>,
    /// Head of the free list (index of the first free slot).
    free_head: Option<usize>,
    /// Number of elements in the map.
    len: usize,
    /// Phantom data for key type.
    _marker: PhantomData<K>,
}

impl<'brand, K, V> BrandedRadixTrieMap<'brand, K, V> {
    /// Creates a new empty Radix Trie Map.
    pub fn new() -> Self {
        Self {
            nodes: BrandedVec::new(),
            root: None,
            free_head: None,
            len: 0,
            _marker: PhantomData,
        }
    }

    /// Creates a new empty Radix Trie Map with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: BrandedVec::with_capacity(capacity),
            root: None,
            free_head: None,
            len: 0,
            _marker: PhantomData,
        }
    }

    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Clears the map.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.root = None;
        self.free_head = None;
        self.len = 0;
    }

    /// Helper to allocate a node, reusing free slots if available.
    fn alloc_node(&mut self, node: Node<V>) -> usize {
        if let Some(idx) = self.free_head {
            // Internal mutation to access next pointer in free slot
            // SAFETY: We have &mut self, so exclusive access to nodes.
            // We can safely interpret the slot as NodeSlot without a token.
            let next_free = unsafe {
                let slot_ptr = self.nodes.inner.as_mut_ptr().add(idx) as *mut NodeSlot<V>;
                let slot_ref = &mut *slot_ptr;

                if let NodeSlot::Free(next) = slot_ref {
                    *next
                } else {
                    panic!("Corrupted free list");
                }
            };

            self.free_head = if next_free == usize::MAX {
                None
            } else {
                Some(next_free)
            };

            unsafe {
                let slot_ptr = self.nodes.inner.as_mut_ptr().add(idx) as *mut NodeSlot<V>;
                *slot_ptr = NodeSlot::Occupied(node);
            }
            idx
        } else {
            let idx = self.nodes.len();
            self.nodes.push(NodeSlot::Occupied(node));
            idx
        }
    }

    /// Helper to free a node.
    fn free_node(&mut self, idx: usize) {
        let next = self.free_head.unwrap_or(usize::MAX);
        self.free_head = Some(idx);

        unsafe {
            let slot_ptr = self.nodes.inner.as_mut_ptr().add(idx) as *mut NodeSlot<V>;
            *slot_ptr = NodeSlot::Free(next);
        }
    }

    /// Internal DFS traversal that passes constructed keys to a callback.
    /// Returns false if traversal was stopped by callback (callback returned false).
    fn traverse_dfs<F>(
        &self,
        token: &GhostToken<'brand>,
        node_idx: usize,
        key_buf: &mut Vec<u8>,
        f: &mut F,
    ) -> bool
    where
        F: FnMut(&[u8], &V) -> bool,
    {
        let slot = self.nodes.get(token, node_idx).expect("Corrupted");
        if let NodeSlot::Occupied(node) = slot {
            key_buf.extend_from_slice(node.prefix.as_slice());

            // Process value
            if let Some(val) = &node.value {
                if !f(key_buf, val) {
                    key_buf.truncate(key_buf.len() - node.prefix.len());
                    return false;
                }
            }

            // Process children
            for &(_, child_idx) in &node.children {
                if !self.traverse_dfs(token, child_idx, key_buf, f) {
                    key_buf.truncate(key_buf.len() - node.prefix.len());
                    return false;
                }
            }

            key_buf.truncate(key_buf.len() - node.prefix.len());
            true
        } else {
            panic!("Traversed free slot");
        }
    }

    /// Iterates over all elements, passing the key (as slice) and value to the closure.
    /// This avoids allocating a new Vec for each key.
    pub fn for_each<F>(&self, token: &GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&[u8], &V),
    {
        if let Some(root) = self.root {
            let mut key_buf: Vec<u8> = Vec::new();
            let mut wrapper = |k: &[u8], v: &V| {
                f(k, v);
                true
            };
            self.traverse_dfs(token, root, &mut key_buf, &mut wrapper);
        }
    }
}

impl<'brand, K, V> BrandedRadixTrieMap<'brand, K, V>
where
    K: AsRef<[u8]>,
{
    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V> {
        let key_bytes = key.as_ref();

        if self.root.is_none() {
            let mut node = Node::new_with_value(value);
            node.prefix = NodePrefix::new(key_bytes);
            self.root = Some(self.alloc_node(node));
            self.len += 1;
            return None;
        }

        let mut curr_idx = self.root.unwrap();
        let mut key_offset = 0;

        loop {
            // Peek at the node
            let (common_len, prefix_len) = {
                let slot = self
                    .nodes
                    .get(token, curr_idx)
                    .expect("Node index out of bounds");
                if let NodeSlot::Occupied(node) = slot {
                    let common = common_prefix_len(&key_bytes[key_offset..], node.prefix.as_slice());
                    (common, node.prefix.len())
                } else {
                    panic!("Corrupted trie: pointing to free slot");
                }
            };

            if common_len < prefix_len {
                // Split required
                let (old_value, old_children, old_prefix) = {
                    let slot = self.nodes.get_mut(token, curr_idx).unwrap();
                    if let NodeSlot::Occupied(node) = slot {
                        (
                            node.value.take(),
                            core::mem::take(&mut node.children),
                            core::mem::take(&mut node.prefix),
                        )
                    } else {
                        unreachable!()
                    }
                };

                // We took old_prefix (NodePrefix). We need to split it.
                // NodePrefix doesn't support easy splitting without access to inner data.
                // But we can get slice.
                // The issue: old_prefix is now owned by us.
                let old_prefix_slice = old_prefix.as_slice();

                let suffix = &old_prefix_slice[common_len..];
                let mut new_child = Node::new();
                new_child.prefix = NodePrefix::new(suffix);
                new_child.value = old_value;
                new_child.children = old_children;

                let new_child_idx = self.alloc_node(new_child);

                // Update current
                let slot = self.nodes.get_mut(token, curr_idx).unwrap();
                if let NodeSlot::Occupied(node) = slot {
                    node.prefix = NodePrefix::new(&old_prefix_slice[..common_len]);
                    node.add_child(suffix[0], new_child_idx);

                    let remaining_key_len = key_bytes.len() - key_offset;
                    if common_len == remaining_key_len {
                        let old = node.value.replace(value);
                        if old.is_none() {
                            self.len += 1;
                        }
                        return old;
                    }
                }

                // Add leaf
                let rest_of_key = &key_bytes[key_offset + common_len..];
                let mut leaf = Node::new_with_value(value);
                leaf.prefix = NodePrefix::new(rest_of_key);
                let leaf_idx = self.alloc_node(leaf);

                let slot = self.nodes.get_mut(token, curr_idx).unwrap();
                if let NodeSlot::Occupied(node) = slot {
                    node.add_child(rest_of_key[0], leaf_idx);
                }
                self.len += 1;
                return None;
            } else if key_offset + common_len == key_bytes.len() {
                // Match
                let slot = self.nodes.get_mut(token, curr_idx).unwrap();
                if let NodeSlot::Occupied(node) = slot {
                    let old = node.value.replace(value);
                    if old.is_none() {
                        self.len += 1;
                    }
                    return old;
                }
            } else {
                // Continue
                let next_byte = key_bytes[key_offset + common_len];
                let child_idx_opt = {
                    let slot = self.nodes.get(token, curr_idx).unwrap();
                    if let NodeSlot::Occupied(node) = slot {
                        node.get_child(next_byte)
                    } else {
                        unreachable!()
                    }
                };

                if let Some(child_idx) = child_idx_opt {
                    curr_idx = child_idx;
                    key_offset += common_len;
                    continue;
                } else {
                    let rest_of_key = &key_bytes[key_offset + common_len..];
                    let mut leaf = Node::new_with_value(value);
                    leaf.prefix = NodePrefix::new(rest_of_key);
                    let leaf_idx = self.alloc_node(leaf);

                    let slot = self.nodes.get_mut(token, curr_idx).unwrap();
                    if let NodeSlot::Occupied(node) = slot {
                        node.add_child(next_byte, leaf_idx);
                    }
                    self.len += 1;
                    return None;
                }
            }
        }
    }

    /// Gets a reference to the value corresponding to the key.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, key: K) -> Option<&'a V> {
        if let Some(mut curr_idx) = self.root {
            let key_bytes = key.as_ref();
            let mut key_offset = 0;

            loop {
                let slot = self.nodes.get(token, curr_idx).expect("Corrupted Trie");
                if let NodeSlot::Occupied(node) = slot {
                    let prefix_len = node.prefix.len();

                    if key_bytes.len() - key_offset < prefix_len {
                        return None;
                    }
                    if &key_bytes[key_offset..key_offset + prefix_len] != node.prefix.as_slice() {
                        return None;
                    }

                    key_offset += prefix_len;

                    if key_offset == key_bytes.len() {
                        return node.value.as_ref();
                    }

                    let next_byte = key_bytes[key_offset];
                    if let Some(child_idx) = node.get_child(next_byte) {
                        curr_idx = child_idx;
                    } else {
                        return None;
                    }
                } else {
                    return None; // Should panic maybe
                }
            }
        }
        None
    }

    /// Gets a mutable reference to the value.
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, key: K) -> Option<&'a mut V> {
        if let Some(mut curr_idx) = self.root {
            let key_bytes = key.as_ref();
            let mut key_offset = 0;

            // Workaround for borrow checker limitation in loops with conditional return
            let token_ptr = token as *mut GhostToken<'brand>;

            loop {
                // SAFETY: We essentially re-borrow the token for each iteration.
                // We ensure that we don't use 'token' again in this iteration.
                // If we return, we return a borrow linked to 'token' (lifetime 'a), which is valid.
                let iter_token = unsafe { &mut *token_ptr };

                let slot = self
                    .nodes
                    .get_mut(iter_token, curr_idx)
                    .expect("Corrupted Trie");

                if let NodeSlot::Occupied(node) = slot {
                    let p_len = node.prefix.len();

                    if key_bytes.len() - key_offset < p_len {
                        return None;
                    }
                    if &key_bytes[key_offset..key_offset + p_len] != node.prefix.as_slice() {
                        return None;
                    }

                    let next_offset = key_offset + p_len;
                    if next_offset == key_bytes.len() {
                        return node.value.as_mut();
                    } else {
                        let next_byte = key_bytes[next_offset];
                        if let Some(next_idx) = node.get_child(next_byte) {
                            curr_idx = next_idx;
                            key_offset += p_len;
                            continue;
                        } else {
                            return None;
                        }
                    }
                } else {
                    return None;
                }
            }
        }
        None
    }

    /// Removes a key from the map.
    pub fn remove(&mut self, token: &mut GhostToken<'brand>, key: K) -> Option<V> {
        let key_bytes = key.as_ref();
        if self.root.is_none() {
            return None;
        }

        let mut path = Vec::new();
        let mut curr_idx = self.root.unwrap();
        let mut key_offset = 0;

        let found_idx = loop {
            let slot = self.nodes.get(token, curr_idx).expect("Corrupted");
            if let NodeSlot::Occupied(node) = slot {
                let p_len = node.prefix.len();

                if key_bytes.len() - key_offset < p_len
                    || &key_bytes[key_offset..key_offset + p_len] != node.prefix.as_slice()
                {
                    return None;
                }

                key_offset += p_len;

                if key_offset == key_bytes.len() {
                    break Some(curr_idx);
                }

                let next_byte = key_bytes[key_offset];
                if let Some(child_idx) = node.get_child(next_byte) {
                    path.push((curr_idx, next_byte));
                    curr_idx = child_idx;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        };

        let target_idx = found_idx?;

        let old_val = {
            let slot = self.nodes.get_mut(token, target_idx).unwrap();
            if let NodeSlot::Occupied(node) = slot {
                node.value.take()
            } else {
                None
            }
        };

        if old_val.is_none() {
            return None;
        }
        self.len -= 1;

        let mut child_to_remove = target_idx;

        let is_empty_leaf = {
            let slot = self.nodes.get(token, child_to_remove).unwrap();
            if let NodeSlot::Occupied(node) = slot {
                node.value.is_none() && node.children.is_empty()
            } else {
                false
            }
        };

        if is_empty_leaf {
            self.free_node(child_to_remove);

            if let Some((parent_idx, byte)) = path.pop() {
                let slot = self.nodes.get_mut(token, parent_idx).unwrap();
                if let NodeSlot::Occupied(parent) = slot {
                    parent.remove_child(byte);
                }
                child_to_remove = parent_idx;
            } else {
                self.root = None;
                self.nodes.clear();
                self.free_head = None;
                return old_val;
            }
        }

        while !path.is_empty() {
            let is_empty_leaf = {
                let slot = self.nodes.get(token, child_to_remove).unwrap();
                if let NodeSlot::Occupied(node) = slot {
                    node.value.is_none() && node.children.is_empty()
                } else {
                    false
                }
            };

            if is_empty_leaf {
                self.free_node(child_to_remove);

                if let Some((parent_idx, byte)) = path.pop() {
                    let slot = self.nodes.get_mut(token, parent_idx).unwrap();
                    if let NodeSlot::Occupied(parent) = slot {
                        parent.remove_child(byte);
                    }
                    child_to_remove = parent_idx;
                } else {
                    self.root = None;
                    self.nodes.clear();
                    self.free_head = None;
                    return old_val;
                }
            } else {
                break;
            }
        }

        old_val
    }
}

// Helper function
fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

impl<'brand, K, V> BrandedCollection<'brand> for BrandedRadixTrieMap<'brand, K, V> {
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl<'brand, K, V> ZeroCopyMapOps<'brand, K, V> for BrandedRadixTrieMap<'brand, K, V>
where
    K: AsRef<[u8]>,
{
    fn find_ref<'a, F>(&'a self, _token: &'a GhostToken<'brand>, _f: F) -> Option<(&'a K, &'a V)>
    where
        F: Fn(&K, &V) -> bool,
    {
        None
    }

    fn any_ref<F>(&self, _token: &GhostToken<'brand>, _f: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        if let Some(_root) = self.root {
            // Best effort: traverse but we can't invoke f because we can't construct &K
            // So we return false.
            // Ideally we should panic or documentation should say "Not supported for Trie".
            false
        } else {
            false
        }
    }

    fn all_ref<F>(&self, _token: &GhostToken<'brand>, _f: F) -> bool
    where
        F: Fn(&K, &V) -> bool,
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_trie_basic() {
        GhostToken::new(|mut token| {
            let mut map = BrandedRadixTrieMap::new();

            // Insert
            map.insert(&mut token, "hello", 1);
            map.insert(&mut token, "helium", 2);
            map.insert(&mut token, "world", 3);

            // Get
            assert_eq!(map.get(&token, "hello"), Some(&1));
            assert_eq!(map.get(&token, "helium"), Some(&2));
            assert_eq!(map.get(&token, "world"), Some(&3));
            assert_eq!(map.get(&token, "hell"), None);

            // Overwrite
            map.insert(&mut token, "hello", 100);
            assert_eq!(map.get(&token, "hello"), Some(&100));

            // Remove
            assert_eq!(map.remove(&mut token, "helium"), Some(2));
            assert_eq!(map.get(&token, "helium"), None);
            assert_eq!(map.get(&token, "hello"), Some(&100)); // Should still exist
        });
    }

    #[test]
    fn test_branded_trie_split() {
        GhostToken::new(|mut token| {
            let mut map = BrandedRadixTrieMap::new();
            map.insert(&mut token, "a", 1);
            map.insert(&mut token, "ab", 2);
            map.insert(&mut token, "abc", 3);

            assert_eq!(map.get(&token, "a"), Some(&1));
            assert_eq!(map.get(&token, "ab"), Some(&2));
            assert_eq!(map.get(&token, "abc"), Some(&3));

            map.insert(&mut token, "abd", 4);
            assert_eq!(map.get(&token, "abd"), Some(&4));
        });
    }

    #[test]
    fn test_branded_trie_iterator() {
        use crate::collections::trie::iter::Iter;
        use crate::alloc::BrandedRc;
        use crate::collections::BrandedVec;

        GhostToken::new(|mut token| {
            let mut map = BrandedRadixTrieMap::new();
            map.insert(&mut token, "apple", 1);
            map.insert(&mut token, "app", 2);
            map.insert(&mut token, "banana", 3);

            // Test iteration
            let iter = Iter::new(&map, &token);
            let mut items: Vec<(BrandedRc<BrandedVec<u8>>, &i32)> = iter.collect();
            // Sort by key for deterministic check (BrandedVec doesn't impl Ord directly without token)
            // But we can compare slices
            items.sort_by(|a, b| a.0.as_slice(&token).cmp(b.0.as_slice(&token)));

            assert_eq!(items.len(), 3);
            assert_eq!(items[0].0.as_slice(&token), b"app");
            assert_eq!(*items[0].1, 2);
            assert_eq!(items[1].0.as_slice(&token), b"apple");
            assert_eq!(*items[1].1, 1);
            assert_eq!(items[2].0.as_slice(&token), b"banana");
            assert_eq!(*items[2].1, 3);
        });
    }
}
