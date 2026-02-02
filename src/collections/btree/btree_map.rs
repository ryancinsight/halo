//! `BrandedBTreeMap` — a B-Tree map with token-gated values.
//!
//! This implementation uses a `BrandedVec` arena to store nodes, improving cache locality
//! and reducing allocations compared to pointer-based implementations.
//! Values are stored inline in the nodes, protected by the `BrandedVec`'s token mechanism.

use crate::collections::BrandedCollection;
use crate::{BrandedVec, GhostToken};
use crate::token::traits::{GhostBorrow, GhostBorrowMut};
use std::borrow::Borrow;
// use std::cmp::Ordering;
use std::marker::PhantomData;
use std::mem::MaybeUninit;

// B-Tree order parameters.
// B = 6 roughly corresponds to std::collections::BTreeMap logic but simplified.
const B: usize = 6;
const MIN_LEN: usize = B - 1;
const MAX_LEN: usize = 2 * B - 1;
const MAX_CHILDREN: usize = 2 * B;

/// A branded index into the B-Tree storage.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NodeIdx<'brand>(u32, PhantomData<fn(&'brand ()) -> &'brand ()>);

impl<'brand> NodeIdx<'brand> {
    const NONE: Self = Self(u32::MAX, PhantomData);

    #[inline(always)]
    fn new(idx: usize) -> Self {
        debug_assert!(idx < u32::MAX as usize);
        Self(idx as u32, PhantomData)
    }

    #[inline(always)]
    fn index(self) -> usize {
        self.0 as usize
    }

    #[inline(always)]
    fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    #[inline(always)]
    fn is_some(self) -> bool {
        self.0 != u32::MAX
    }
}

struct NodeData<'brand, K, V> {
    keys: [MaybeUninit<K>; MAX_LEN],
    vals: [MaybeUninit<V>; MAX_LEN],
    children: [NodeIdx<'brand>; MAX_CHILDREN],
    len: u16,
    is_leaf: bool,
    next_free: NodeIdx<'brand>, // For free list
}

impl<'brand, K, V> NodeData<'brand, K, V> {
    fn new(is_leaf: bool) -> Self {
        // Safety: MaybeUninit array initialization is safe as we don't read it.
        let keys = unsafe { MaybeUninit::uninit().assume_init() };
        let vals = unsafe { MaybeUninit::uninit().assume_init() };

        Self {
            keys,
            vals,
            children: [NodeIdx::NONE; MAX_CHILDREN],
            len: 0,
            is_leaf,
            next_free: NodeIdx::NONE,
        }
    }

    #[inline(always)]
    unsafe fn key_at(&self, idx: usize) -> &K {
        self.keys.get_unchecked(idx).assume_init_ref()
    }

    #[inline(always)]
    unsafe fn key_at_mut(&mut self, idx: usize) -> &mut K {
        self.keys.get_unchecked_mut(idx).assume_init_mut()
    }

    #[inline(always)]
    unsafe fn val_at(&self, idx: usize) -> &V {
        self.vals.get_unchecked(idx).assume_init_ref()
    }

    #[inline(always)]
    unsafe fn val_at_mut(&mut self, idx: usize) -> &mut V {
        self.vals.get_unchecked_mut(idx).assume_init_mut()
    }

    #[inline(always)]
    fn is_full(&self) -> bool {
        self.len as usize == MAX_LEN
    }

    // Search for a key in the node.
    // Returns Ok(index) if found, Err(index) if not found (index is where it should be).
    fn search_key<Q: ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        // Binary search
        let len = self.len as usize;
        let slice = unsafe { std::slice::from_raw_parts(self.keys.as_ptr() as *const K, len) };
        slice.binary_search_by(|k| k.borrow().cmp(key))
    }
}

impl<'brand, K, V> Drop for NodeData<'brand, K, V> {
    fn drop(&mut self) {
        for i in 0..self.len as usize {
            unsafe {
                self.keys.get_unchecked_mut(i).assume_init_drop();
                self.vals.get_unchecked_mut(i).assume_init_drop();
            }
        }
    }
}

/// A B-Tree map with token-gated values.
pub struct BrandedBTreeMap<'brand, K, V> {
    nodes: BrandedVec<'brand, NodeData<'brand, K, V>>,
    root: NodeIdx<'brand>,
    len: usize,
    free_head: NodeIdx<'brand>,
}

impl<'brand, K, V> BrandedBTreeMap<'brand, K, V> {
    /// Creates an empty map.
    pub fn new() -> Self {
        Self {
            nodes: BrandedVec::new(),
            root: NodeIdx::NONE,
            len: 0,
            free_head: NodeIdx::NONE,
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

    fn alloc_node(&mut self, is_leaf: bool) -> NodeIdx<'brand> {
        if self.free_head.is_some() {
            let idx = self.free_head;
            unsafe {
                let node = self.nodes.get_unchecked_mut_exclusive(idx.index());
                self.free_head = node.next_free;
                // Re-initialize node. len is already 0 (from free_node).
                // But we must overwrite everything to be safe.
                // We use write to avoid dropping invalid data (Wait, free_node sets len=0, so drop does nothing).
                // So we can overwrite.
                *node = NodeData::new(is_leaf);
            }
            idx
        } else {
            let idx = NodeIdx::new(self.nodes.len());
            self.nodes.push(NodeData::new(is_leaf));
            idx
        }
    }

    fn free_node(&mut self, idx: NodeIdx<'brand>) {
        unsafe {
            let node = self.nodes.get_unchecked_mut_exclusive(idx.index());
            // Assume caller has handled children and keys/vals.
            // We set len to 0 to prevent Drop from dropping anything.
            node.len = 0;
            node.next_free = self.free_head;
        }
        self.free_head = idx;
    }
}

impl<'brand, K, V> BrandedBTreeMap<'brand, K, V>
where
    K: Ord,
{
    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<'a, Q: ?Sized, Token>(&'a self, token: &'a Token, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q>,
        Q: Ord,
        Token: GhostBorrow<'brand>,
    {
        let mut curr = self.root;
        while curr.is_some() {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr.index());
                match node.search_key(key) {
                    Ok(idx) => return Some(node.val_at(idx)),
                    Err(idx) => {
                        if node.is_leaf {
                            return None;
                        }
                        curr = node.children[idx];
                    }
                }
            }
        }
        None
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut<'a, Q: ?Sized, Token>(
        &'a self,
        token: &'a mut Token,
        key: &Q,
    ) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
        Token: GhostBorrowMut<'brand>,
    {
        let mut curr = self.root;
        while curr.is_some() {
            let found_idx = unsafe {
                // Shared search first to allow token re-use
                let node = self.nodes.get_unchecked(token, curr.index());
                match node.search_key(key) {
                    Ok(idx) => Some(idx),
                    Err(idx) => {
                        if node.is_leaf {
                            return None;
                        }
                        curr = node.children[idx];
                        None
                    }
                }
            };

            if let Some(idx) = found_idx {
                unsafe {
                    let node = self.nodes.get_unchecked_mut(token, curr.index());
                    return Some(node.val_at_mut(idx));
                }
            }
        }
        None
    }

    /// Returns true if the map contains a value for the specified key.
    pub fn contains_key_with_token<Q: ?Sized, Token>(&self, token: &Token, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
        Token: GhostBorrow<'brand>,
    {
        self.get(token, key).is_some()
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.root.is_none() {
            let root_idx = self.alloc_node(true);
            self.root = root_idx;
            unsafe {
                let root = self.nodes.get_unchecked_mut_exclusive(root_idx.index());
                root.keys[0].write(key);
                root.vals[0].write(value);
                root.len = 1;
            }
            self.len += 1;
            return None;
        }

        let is_root_full = unsafe {
            self.nodes
                .get_unchecked_mut_exclusive(self.root.index())
                .is_full()
        };

        if is_root_full {
            let old_root_idx = self.root;
            let new_root_idx = self.alloc_node(false);
            self.root = new_root_idx;

            unsafe {
                let new_root = self.nodes.get_unchecked_mut_exclusive(new_root_idx.index());
                new_root.children[0] = old_root_idx;
            }

            self.split_child(new_root_idx, 0);
            self.insert_non_full(new_root_idx, key, value)
        } else {
            self.insert_non_full(self.root, key, value)
        }
    }

    fn split_child(&mut self, parent_idx: NodeIdx<'brand>, child_index: usize) {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();

            let parent = &mut *nodes_ptr.add(parent_idx.index());
            let child_idx = parent.children[child_index];
            let child = &mut *nodes_ptr.add(child_idx.index());

            let new_child_idx = self.alloc_node(child.is_leaf);
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let parent = &mut *nodes_ptr.add(parent_idx.index());
            let child = &mut *nodes_ptr.add(child_idx.index());
            let new_child = &mut *nodes_ptr.add(new_child_idx.index());

            let mid_idx = B - 1;
            let split_idx = B;

            let move_count = MAX_LEN - split_idx;

            std::ptr::copy_nonoverlapping(
                child.keys.as_ptr().add(split_idx),
                new_child.keys.as_mut_ptr(),
                move_count,
            );
            std::ptr::copy_nonoverlapping(
                child.vals.as_ptr().add(split_idx),
                new_child.vals.as_mut_ptr(),
                move_count,
            );

            if !child.is_leaf {
                let children_move_count = MAX_CHILDREN - B;
                std::ptr::copy_nonoverlapping(
                    child.children.as_ptr().add(B),
                    new_child.children.as_mut_ptr(),
                    children_move_count,
                );
            }

            new_child.len = move_count as u16;
            child.len = mid_idx as u16;

            let p_len = parent.len as usize;
            if p_len > child_index {
                std::ptr::copy(
                    parent.keys.as_ptr().add(child_index),
                    parent.keys.as_mut_ptr().add(child_index + 1),
                    p_len - child_index,
                );
                std::ptr::copy(
                    parent.vals.as_ptr().add(child_index),
                    parent.vals.as_mut_ptr().add(child_index + 1),
                    p_len - child_index,
                );
                std::ptr::copy(
                    parent.children.as_ptr().add(child_index + 1),
                    parent.children.as_mut_ptr().add(child_index + 2),
                    p_len - child_index,
                );
            }

            std::ptr::copy_nonoverlapping(
                child.keys.as_ptr().add(mid_idx),
                parent.keys.as_mut_ptr().add(child_index),
                1,
            );
            std::ptr::copy_nonoverlapping(
                child.vals.as_ptr().add(mid_idx),
                parent.vals.as_mut_ptr().add(child_index),
                1,
            );

            parent.children[child_index + 1] = new_child_idx;
            parent.len += 1;
        }
    }

    fn insert_non_full(&mut self, node_idx: NodeIdx<'brand>, key: K, value: V) -> Option<V> {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let node = &mut *nodes_ptr.add(node_idx.index());

            let mut i = node.len as usize;

            if node.is_leaf {
                while i > 0 {
                    let k = node.key_at(i - 1);
                    if k == &key {
                        let old = std::mem::replace(node.val_at_mut(i - 1), value);
                        return Some(old);
                    }
                    if k < &key {
                        break;
                    }
                    i -= 1;
                }

                if i < node.len as usize {
                    std::ptr::copy(
                        node.keys.as_ptr().add(i),
                        node.keys.as_mut_ptr().add(i + 1),
                        node.len as usize - i,
                    );
                    std::ptr::copy(
                        node.vals.as_ptr().add(i),
                        node.vals.as_mut_ptr().add(i + 1),
                        node.len as usize - i,
                    );
                }

                node.keys[i].write(key);
                node.vals[i].write(value);
                node.len += 1;
                self.len += 1;
                None
            } else {
                while i > 0 {
                    let k = node.key_at(i - 1);
                    if k == &key {
                        let old = std::mem::replace(node.val_at_mut(i - 1), value);
                        return Some(old);
                    }
                    if k < &key {
                        break;
                    }
                    i -= 1;
                }

                let child_idx = node.children[i];
                let child = &*nodes_ptr.add(child_idx.index());

                if child.is_full() {
                    self.split_child(node_idx, i);

                    // Re-acquire pointer after split_child (which calls alloc_node)
                    let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
                    let node = &*nodes_ptr.add(node_idx.index());
                    if node.key_at(i) < &key {
                        self.insert_non_full(node.children[i + 1], key, value)
                    } else if node.key_at(i) > &key {
                        self.insert_non_full(node.children[i], key, value)
                    } else {
                        let old = std::mem::replace(
                            self.nodes
                                .get_unchecked_mut_exclusive(node_idx.index())
                                .val_at_mut(i),
                            value,
                        );
                        Some(old)
                    }
                } else {
                    self.insert_non_full(child_idx, key, value)
                }
            }
        }
    }

    /// Removes a key from the map.
    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        if self.root.is_none() {
            return None;
        }

        // We perform recursive delete
        let res = self.remove_from_node(self.root, key);

        if res.is_some() {
            self.len -= 1;
            // Check if root became empty
            unsafe {
                let root = self.nodes.get_unchecked_mut_exclusive(self.root.index());
                if root.len == 0 {
                    let old_root_idx = self.root;
                    if root.is_leaf {
                        self.root = NodeIdx::NONE;
                    } else {
                        self.root = root.children[0];
                    }
                    self.free_node(old_root_idx);
                }
            }
        }
        res
    }

    fn remove_from_node<Q: ?Sized>(&mut self, node_idx: NodeIdx<'brand>, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord,
    {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let node = &mut *nodes_ptr.add(node_idx.index());

            let idx = match node.search_key(key) {
                Ok(i) => i,
                Err(i) => {
                    // Not found in this node.
                    if node.is_leaf {
                        return None;
                    }
                    // Go to child i.
                    // Ensure child has >= B keys.
                    // If child has B-1 keys, we need to fix it.
                    // (Standard B-Tree: MIN_LEN = B-1. We need MIN_LEN+1 to remove safely?
                    // No, if we descend, we ensure child has >= B keys so that if we delete from it, it has >= B-1).

                    let child_idx = node.children[i];
                    let child = &*nodes_ptr.add(child_idx.index());

                    if (child.len as usize) < B {
                        self.fix_child(node_idx, i);
                        // After fix, key might be in node or one of children.
                        // Restart search in this node?
                        return self.remove_from_node(node_idx, key);
                    } else {
                        return self.remove_from_node(child_idx, key);
                    }
                }
            };

            // Found at idx.
            if node.is_leaf {
                let val = std::ptr::read(node.val_at(idx));
                let _k = std::ptr::read(node.key_at(idx)); // Drop key

                // Shift
                if idx < node.len as usize - 1 {
                    std::ptr::copy(
                        node.keys.as_ptr().add(idx + 1),
                        node.keys.as_mut_ptr().add(idx),
                        node.len as usize - 1 - idx,
                    );
                    std::ptr::copy(
                        node.vals.as_ptr().add(idx + 1),
                        node.vals.as_mut_ptr().add(idx),
                        node.len as usize - 1 - idx,
                    );
                }
                node.len -= 1;
                return Some(val);
            } else {
                // Internal node.
                // Replace with predecessor (from left child)
                let left_child_idx = node.children[idx];
                let left_child = &*nodes_ptr.add(left_child_idx.index());

                if (left_child.len as usize) >= B {
                    let (pred_key, pred_val) = self.pop_max(left_child_idx);
                    // Replace key/val at idx with pred
                    let old_val = std::mem::replace(node.val_at_mut(idx), pred_val);
                    let _old_key = std::mem::replace(node.key_at_mut(idx), pred_key);
                    return Some(old_val);
                }

                // Or successor (from right child)
                let right_child_idx = node.children[idx + 1];
                let right_child = &*nodes_ptr.add(right_child_idx.index());

                if (right_child.len as usize) >= B {
                    let (succ_key, succ_val) = self.pop_min(right_child_idx);
                    let old_val = std::mem::replace(node.val_at_mut(idx), succ_val);
                    let _old_key = std::mem::replace(node.key_at_mut(idx), succ_key);
                    return Some(old_val);
                }

                // Both have B-1. Merge them.
                self.merge_children(node_idx, idx);
                // Key is now in the merged child. Recurse.
                // merged child is at children[idx].
                let merged_child_idx = self
                    .nodes
                    .get_unchecked_mut_exclusive(node_idx.index())
                    .children[idx];
                return self.remove_from_node(merged_child_idx, key);
            }
        }
    }

    // Ensures child at child_idx has at least B keys.
    // child_idx is index in parent.children.
    fn fix_child(&mut self, parent_idx: NodeIdx<'brand>, child_idx: usize) {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let parent = &mut *nodes_ptr.add(parent_idx.index());

            // Try borrow from left sibling
            if child_idx > 0 {
                let left_sibling_idx = parent.children[child_idx - 1];
                let left_sibling = &mut *nodes_ptr.add(left_sibling_idx.index());
                if (left_sibling.len as usize) >= B {
                    self.rotate_right(parent_idx, child_idx);
                    return;
                }
            }

            // Try borrow from right sibling
            if child_idx < parent.len as usize {
                let right_sibling_idx = parent.children[child_idx + 1];
                let right_sibling = &mut *nodes_ptr.add(right_sibling_idx.index());
                if (right_sibling.len as usize) >= B {
                    self.rotate_left(parent_idx, child_idx);
                    return;
                }
            }

            // Merge
            if child_idx < parent.len as usize {
                self.merge_children(parent_idx, child_idx);
            } else {
                self.merge_children(parent_idx, child_idx - 1);
            }
        }
    }

    fn pop_max(&mut self, node_idx: NodeIdx<'brand>) -> (K, V) {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let node = &mut *nodes_ptr.add(node_idx.index());

            if node.is_leaf {
                let idx = node.len as usize - 1;
                let key = std::ptr::read(node.key_at(idx));
                let val = std::ptr::read(node.val_at(idx));
                node.len -= 1;
                return (key, val);
            } else {
                let child_idx = node.children[node.len as usize];
                let child = &*nodes_ptr.add(child_idx.index());
                if (child.len as usize) < B {
                    self.fix_child(node_idx, node.len as usize);
                    // Reload node as fix_child might invalidate refs if we held them (we don't here)
                    // Recurse
                    return self.pop_max(node_idx);
                }
                return self.pop_max(child_idx);
            }
        }
    }

    fn pop_min(&mut self, node_idx: NodeIdx<'brand>) -> (K, V) {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let node = &mut *nodes_ptr.add(node_idx.index());

            if node.is_leaf {
                let key = std::ptr::read(node.key_at(0));
                let val = std::ptr::read(node.val_at(0));

                // Shift
                std::ptr::copy(
                    node.keys.as_ptr().add(1),
                    node.keys.as_mut_ptr(),
                    node.len as usize - 1,
                );
                std::ptr::copy(
                    node.vals.as_ptr().add(1),
                    node.vals.as_mut_ptr(),
                    node.len as usize - 1,
                );
                node.len -= 1;
                return (key, val);
            } else {
                let child_idx = node.children[0];
                let child = &*nodes_ptr.add(child_idx.index());
                if (child.len as usize) < B {
                    self.fix_child(node_idx, 0);
                    return self.pop_min(node_idx);
                }
                return self.pop_min(child_idx);
            }
        }
    }

    // Merge children[idx] and children[idx+1]
    fn merge_children(&mut self, parent_idx: NodeIdx<'brand>, idx: usize) {
        let right_idx_to_free = unsafe {
            self.nodes
                .get_unchecked_mut_exclusive(parent_idx.index())
                .children[idx + 1]
        };

        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let parent = &mut *nodes_ptr.add(parent_idx.index());

            let left_idx = parent.children[idx];
            // right_idx_to_free is already captured

            let left = &mut *nodes_ptr.add(left_idx.index());
            let right = &mut *nodes_ptr.add(right_idx_to_free.index());

            // Move separator from parent to left
            let sep_key = std::ptr::read(parent.key_at(idx));
            let sep_val = std::ptr::read(parent.val_at(idx));

            left.keys[left.len as usize].write(sep_key);
            left.vals[left.len as usize].write(sep_val);

            // Move right to left
            std::ptr::copy_nonoverlapping(
                right.keys.as_ptr(),
                left.keys.as_mut_ptr().add(left.len as usize + 1),
                right.len as usize,
            );
            std::ptr::copy_nonoverlapping(
                right.vals.as_ptr(),
                left.vals.as_mut_ptr().add(left.len as usize + 1),
                right.len as usize,
            );

            if !left.is_leaf {
                std::ptr::copy_nonoverlapping(
                    right.children.as_ptr(),
                    left.children.as_mut_ptr().add(left.len as usize + 1),
                    right.len as usize + 1,
                );
            }

            left.len += 1 + right.len;

            // Shift parent
            std::ptr::copy(
                parent.keys.as_ptr().add(idx + 1),
                parent.keys.as_mut_ptr().add(idx),
                parent.len as usize - 1 - idx,
            );
            std::ptr::copy(
                parent.vals.as_ptr().add(idx + 1),
                parent.vals.as_mut_ptr().add(idx),
                parent.len as usize - 1 - idx,
            );
            std::ptr::copy(
                parent.children.as_ptr().add(idx + 2),
                parent.children.as_mut_ptr().add(idx + 1),
                parent.len as usize - 1 - idx,
            );
            parent.len -= 1;

            // right node is logically empty now (contents moved)
            right.len = 0;
        }

        self.free_node(right_idx_to_free);
    }

    fn rotate_right(&mut self, parent_idx: NodeIdx<'brand>, child_idx: usize) {
        // Move from left sibling to child
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let parent = &mut *nodes_ptr.add(parent_idx.index());
            let child = &mut *nodes_ptr.add(parent.children[child_idx].index());
            let sibling = &mut *nodes_ptr.add(parent.children[child_idx - 1].index());

            // Make room in child
            std::ptr::copy(
                child.keys.as_ptr(),
                child.keys.as_mut_ptr().add(1),
                child.len as usize,
            );
            std::ptr::copy(
                child.vals.as_ptr(),
                child.vals.as_mut_ptr().add(1),
                child.len as usize,
            );
            if !child.is_leaf {
                std::ptr::copy(
                    child.children.as_ptr(),
                    child.children.as_mut_ptr().add(1),
                    child.len as usize + 1,
                );
            }

            // Move parent separator to child
            child.keys[0].write(std::ptr::read(parent.key_at(child_idx - 1)));
            child.vals[0].write(std::ptr::read(parent.val_at(child_idx - 1)));

            // Move sibling's last to parent
            let sib_last = sibling.len as usize - 1;
            parent.keys[child_idx - 1].write(std::ptr::read(sibling.key_at(sib_last)));
            parent.vals[child_idx - 1].write(std::ptr::read(sibling.val_at(sib_last)));

            // Move sibling's last child to child's first
            if !child.is_leaf {
                child.children[0] = sibling.children[sib_last + 1];
            }

            child.len += 1;
            sibling.len -= 1;
        }
    }

    fn rotate_left(&mut self, parent_idx: NodeIdx<'brand>, child_idx: usize) {
        unsafe {
            let nodes_ptr = self.nodes.as_mut_slice_exclusive().as_mut_ptr();
            let parent = &mut *nodes_ptr.add(parent_idx.index());
            let child = &mut *nodes_ptr.add(parent.children[child_idx].index());
            let sibling = &mut *nodes_ptr.add(parent.children[child_idx + 1].index());

            // Move parent separator to child end
            child.keys[child.len as usize].write(std::ptr::read(parent.key_at(child_idx)));
            child.vals[child.len as usize].write(std::ptr::read(parent.val_at(child_idx)));

            // Move sibling first to parent
            parent.keys[child_idx].write(std::ptr::read(sibling.key_at(0)));
            parent.vals[child_idx].write(std::ptr::read(sibling.val_at(0)));

            // Move sibling first child to child end
            if !child.is_leaf {
                child.children[child.len as usize + 1] = sibling.children[0];
            }

            child.len += 1;

            // Shift sibling
            std::ptr::copy(
                sibling.keys.as_ptr().add(1),
                sibling.keys.as_mut_ptr(),
                sibling.len as usize - 1,
            );
            std::ptr::copy(
                sibling.vals.as_ptr().add(1),
                sibling.vals.as_mut_ptr(),
                sibling.len as usize - 1,
            );
            if !sibling.is_leaf {
                std::ptr::copy(
                    sibling.children.as_ptr().add(1),
                    sibling.children.as_mut_ptr(),
                    sibling.len as usize,
                );
            }
            sibling.len -= 1;
        }
    }

    /// Returns an iterator over the map.
    pub fn iter<'a, Token>(&'a self, token: &'a Token) -> impl Iterator<Item = (&'a K, &'a V)> + use<'a, 'brand, K, V, Token>
    where
        Token: GhostBorrow<'brand>,
    {
        let mut iter = Iter::<_, _, Token> {
            map: self,
            token,
            stack: Vec::new(),
            len: self.len,
        };
        if self.root.is_some() {
            iter.push_leftmost(self.root);
        }
        iter
    }

    /// Returns an iterator over the keys of the map.
    pub fn keys<'a, Token>(&'a self, token: &'a Token) -> impl Iterator<Item = &'a K> + use<'a, 'brand, K, V, Token>
    where
        Token: GhostBorrow<'brand>,
    {
        let mut iter = Keys::<_, _, Token> {
            map: self,
            token,
            stack: Vec::new(),
            len: self.len,
        };
        if self.root.is_some() {
            iter.push_leftmost(self.root);
        }
        iter
    }

    /// Applies `f` to all entries in the map, allowing mutation of values.
    pub fn for_each_mut<F, Token>(&self, token: &mut Token, mut f: F)
    where
        F: FnMut(&K, &mut V),
        Token: GhostBorrowMut<'brand>,
    {
        if self.root.is_some() {
            self.for_each_node(self.root, token, &mut f);
        }
    }

    fn for_each_node<F, Token>(
        &self,
        node_idx: NodeIdx<'brand>,
        token: &mut Token,
        f: &mut F,
    ) where
        F: FnMut(&K, &mut V),
        Token: GhostBorrowMut<'brand>,
    {
        unsafe {
            // We can't hold reference to node while recurring.
            // But we need to access keys/vals/children.
            // We can rely on indices.
            let node = self.nodes.get_unchecked(token, node_idx.index());
            let len = node.len as usize;
            let is_leaf = node.is_leaf;

            // To iterate mutably, we need &mut token.
            // But get_unchecked takes &token.
            // get_unchecked_mut takes &mut token.
            // We can't hold &mut node across recursion.

            for i in 0..len {
                if !is_leaf {
                    let child_idx = self
                        .nodes
                        .get_unchecked(token, node_idx.index())
                        .children[i];
                    self.for_each_node(child_idx, token, f);
                }

                // Visit key/val
                let node = self.nodes.get_unchecked_mut(token, node_idx.index());
                let k_ptr = node.key_at(i) as *const K;
                let v_ptr = node.val_at_mut(i) as *mut V;

                f(&*k_ptr, &mut *v_ptr);
            }

            if !is_leaf {
                let child_idx = self
                    .nodes
                    .get_unchecked(token, node_idx.index())
                    .children[len];
                self.for_each_node(child_idx, token, f);
            }
        }
    }
}

pub struct Iter<'a, 'brand, K, V, Token = GhostToken<'brand>>
where
    Token: GhostBorrow<'brand>,
{
    map: &'a BrandedBTreeMap<'brand, K, V>,
    token: &'a Token,
    stack: Vec<(NodeIdx<'brand>, usize)>,
    len: usize,
}

impl<'a, 'brand, K, V, Token> Iter<'a, 'brand, K, V, Token>
where
    Token: GhostBorrow<'brand>,
{
    fn push_leftmost(&mut self, mut node_idx: NodeIdx<'brand>) {
        while node_idx.is_some() {
            self.stack.push((node_idx, 0));
            unsafe {
                let node = self
                    .map
                    .nodes
                    .get_unchecked(self.token, node_idx.index());
                if node.is_leaf {
                    break;
                }
                node_idx = node.children[0];
            }
        }
    }
}

impl<'a, 'brand, K, V, Token> Iterator for Iter<'a, 'brand, K, V, Token>
where
    Token: GhostBorrow<'brand>,
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (node_idx, idx) = self.stack.last_mut()?;

            unsafe {
                let node = self
                    .map
                    .nodes
                    .get_unchecked(self.token, node_idx.index());

                if *idx < node.len as usize {
                    let key = node.key_at(*idx);
                    let val = node.val_at(*idx);
                    *idx += 1;

                    if !node.is_leaf {
                        let child_idx = node.children[*idx];
                        self.push_leftmost(child_idx);
                    }

                    return Some((key, val));
                } else {
                    self.stack.pop();
                    // See Iter implementation logic
                }
            }
        }
    }
}

pub struct Keys<'a, 'brand, K, V, Token = GhostToken<'brand>>
where
    Token: GhostBorrow<'brand>,
{
    map: &'a BrandedBTreeMap<'brand, K, V>,
    token: &'a Token,
    stack: Vec<(NodeIdx<'brand>, usize)>,
    len: usize,
}

impl<'a, 'brand, K: 'a, V, Token> Keys<'a, 'brand, K, V, Token>
where
    Token: GhostBorrow<'brand>,
{
    fn push_leftmost(&mut self, mut node_idx: NodeIdx<'brand>) {
        while node_idx.is_some() {
            self.stack.push((node_idx, 0));
            unsafe {
                let node = self
                    .map
                    .nodes
                    .get_unchecked(self.token, node_idx.index());
                if node.is_leaf {
                    break;
                }
                node_idx = node.children[0];
            }
        }
    }
}

impl<'a, 'brand, K: 'a, V, Token> Iterator for Keys<'a, 'brand, K, V, Token>
where
    Token: GhostBorrow<'brand>,
{
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (node_idx, idx) = self.stack.last_mut()?;

            unsafe {
                let node = self
                    .map
                    .nodes
                    .get_unchecked(self.token, node_idx.index());

                if *idx < node.len as usize {
                    let key = node.key_at(*idx);
                    *idx += 1;

                    if !node.is_leaf {
                        let child_idx = node.children[*idx];
                        self.push_leftmost(child_idx);
                    }

                    return Some(key);
                } else {
                    self.stack.pop();
                    // See Iter implementation logic
                }
            }
        }
    }
}

pub struct IntoIter<'brand, K, V> {
    vec: std::vec::IntoIter<(K, V)>,
    phantom: PhantomData<&'brand ()>,
}

impl<'brand, K, V> Iterator for IntoIter<'brand, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.vec.next()
    }
}

impl<'brand, K, V> IntoIterator for BrandedBTreeMap<'brand, K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<'brand, K, V>;

    fn into_iter(mut self) -> Self::IntoIter {
        // Collect all items efficiently
        let mut vec = Vec::with_capacity(self.len);
        if self.root.is_some() {
            Self::collect(self.root, &mut self.nodes, &mut vec);
        }
        IntoIter {
            vec: vec.into_iter(),
            phantom: PhantomData,
        }
    }
}

impl<'brand, K, V> BrandedBTreeMap<'brand, K, V> {
    fn collect(
        node_idx: NodeIdx<'brand>,
        nodes: &mut BrandedVec<'brand, NodeData<'brand, K, V>>,
        vec: &mut Vec<(K, V)>,
    ) {
        unsafe {
            let node = nodes.get_unchecked_mut_exclusive(node_idx.index());
            let len = node.len as usize;
            let is_leaf = node.is_leaf;

            // We need to move keys/vals out.
            // We can't use recursion easily because we hold mutable ref to node.
            // We need to drop ref before recursion.

            // Actually, we can just copy out indices of children, then recurse.
            // But we need to interleave.

            // Strategy: Read all keys/vals and children into temp buffers on stack.
            // Set node.len = 0 (so Drop does nothing).
            // Then recurse.

            let mut keys: [MaybeUninit<K>; MAX_LEN] = MaybeUninit::uninit().assume_init();
            let mut vals: [MaybeUninit<V>; MAX_LEN] = MaybeUninit::uninit().assume_init();
            let mut children: [NodeIdx<'brand>; MAX_CHILDREN] = [NodeIdx::NONE; MAX_CHILDREN];

            std::ptr::copy_nonoverlapping(node.keys.as_ptr(), keys.as_mut_ptr(), len);
            std::ptr::copy_nonoverlapping(node.vals.as_ptr(), vals.as_mut_ptr(), len);
            if !is_leaf {
                std::ptr::copy_nonoverlapping(
                    node.children.as_ptr(),
                    children.as_mut_ptr(),
                    len + 1,
                );
            }

            node.len = 0; // Prevent double free

            // Now we can recurse.
            for i in 0..len {
                if !is_leaf {
                    Self::collect(children[i], nodes, vec);
                }
                vec.push((keys[i].assume_init_read(), vals[i].assume_init_read()));
            }
            if !is_leaf {
                Self::collect(children[len], nodes, vec);
            }
        }
    }
}

impl<'brand, K, V> Default for BrandedBTreeMap<'brand, K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, K, V> BrandedCollection<'brand> for BrandedBTreeMap<'brand, K, V> {
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
            map.insert(1, 10);
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
    fn test_contains_key_with_token() {
        GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            map.insert("a", 1);
            assert!(map.contains_key_with_token(&token, &"a"));
            assert!(!map.contains_key_with_token(&token, &"b"));
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
            // With B=6, tree structure varies.
            assert_eq!(map.remove(&50), Some(500));
            assert_eq!(map.len(), 99);
            assert!(!map.contains_key_with_token(&token, &50));

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
        });
    }
}
