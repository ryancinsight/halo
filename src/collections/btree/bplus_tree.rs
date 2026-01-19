//! `BrandedBPlusTree` — a B+Tree implementation using `BrandedPool` for node storage.

use crate::alloc::pool::{BrandedPool, PoolSlot};
use crate::{GhostCell, GhostToken};
use core::mem::MaybeUninit;
use core::ptr;
use std::borrow::Borrow;

pub const B: usize = 6;
pub const MAX_KEYS: usize = 2 * B - 1;
pub const MAX_CHILDREN: usize = 2 * B;

pub enum Node<'brand, K, V> {
    Internal {
        len: u16,
        keys: [MaybeUninit<K>; MAX_KEYS],
        children: [usize; MAX_CHILDREN],
    },
    Leaf {
        len: u16,
        keys: [MaybeUninit<K>; MAX_KEYS],
        vals: [MaybeUninit<GhostCell<'brand, V>>; MAX_KEYS],
        next: Option<usize>,
    },
}

impl<'brand, K, V> Node<'brand, K, V> {
    pub fn new_leaf() -> Self {
        let keys = unsafe { MaybeUninit::<[MaybeUninit<K>; MAX_KEYS]>::uninit().assume_init() };
        let vals = unsafe {
            MaybeUninit::<[MaybeUninit<GhostCell<'brand, V>>; MAX_KEYS]>::uninit().assume_init()
        };

        Self::Leaf {
            len: 0,
            keys,
            vals,
            next: None,
        }
    }

    pub fn new_internal() -> Self {
        let keys = unsafe { MaybeUninit::<[MaybeUninit<K>; MAX_KEYS]>::uninit().assume_init() };
        let children = [0; MAX_CHILDREN];

        Self::Internal {
            len: 0,
            keys,
            children,
        }
    }

    pub fn is_leaf(&self) -> bool {
        match self {
            Node::Leaf { .. } => true,
            Node::Internal { .. } => false,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Node::Internal { len, .. } => *len as usize,
            Node::Leaf { len, .. } => *len as usize,
        }
    }

    pub fn is_full(&self) -> bool {
        self.len() == MAX_KEYS
    }

    pub fn leaf_insert(&mut self, idx: usize, key: K, val: GhostCell<'brand, V>) {
        if let Node::Leaf {
            len, keys, vals, ..
        } = self
        {
            assert!((*len as usize) < MAX_KEYS);
            let l = *len as usize;
            unsafe {
                ptr::copy(
                    keys.as_ptr().add(idx),
                    keys.as_mut_ptr().add(idx + 1),
                    l - idx,
                );
                ptr::copy(
                    vals.as_ptr().add(idx),
                    vals.as_mut_ptr().add(idx + 1),
                    l - idx,
                );
                keys.get_unchecked_mut(idx).write(key);
                vals.get_unchecked_mut(idx).write(val);
            }
            *len += 1;
        } else {
            panic!("Not a leaf");
        }
    }

    pub fn internal_insert(&mut self, idx: usize, key: K, child: usize) {
        if let Node::Internal {
            len,
            keys,
            children,
            ..
        } = self
        {
            assert!((*len as usize) < MAX_KEYS);
            let l = *len as usize;
            unsafe {
                ptr::copy(
                    keys.as_ptr().add(idx),
                    keys.as_mut_ptr().add(idx + 1),
                    l - idx,
                );
                ptr::copy(
                    children.as_ptr().add(idx + 1),
                    children.as_mut_ptr().add(idx + 2),
                    l - idx,
                );
                keys.get_unchecked_mut(idx).write(key);
                children[idx + 1] = child;
            }
            *len += 1;
        } else {
            panic!("Not an internal node");
        }
    }

    pub fn children_mut(&mut self) -> &mut [usize] {
        if let Node::Internal { children, .. } = self {
            children
        } else {
            panic!("Not internal")
        }
    }

    pub fn child_at(&self, idx: usize) -> usize {
        if let Node::Internal { children, .. } = self {
            children[idx]
        } else {
            panic!("Not internal")
        }
    }

    pub fn key_at(&self, idx: usize) -> &K {
        match self {
            Node::Internal { keys, .. } => unsafe { keys.get_unchecked(idx).assume_init_ref() },
            Node::Leaf { keys, .. } => unsafe { keys.get_unchecked(idx).assume_init_ref() },
        }
    }
}

pub struct BrandedBPlusTree<'brand, K, V> {
    pool: BrandedPool<'brand, Node<'brand, K, V>>,
    root: Option<usize>,
    len: usize,
}

impl<'brand, K, V> BrandedBPlusTree<'brand, K, V> {
    pub fn new() -> Self {
        Self {
            pool: BrandedPool::new(),
            root: None,
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn get_node<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        index: usize,
    ) -> &'a Node<'brand, K, V> {
        self.pool.get(token, index).expect("Node should exist")
    }

    #[inline]
    fn get_node_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        index: usize,
    ) -> &'a mut Node<'brand, K, V> {
        self.pool.get_mut(token, index).expect("Node should exist")
    }

    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, key: &K) -> Option<&'a V>
    where
        K: Ord,
    {
        if let Some(root_idx) = self.root {
            let mut node_idx = root_idx;
            loop {
                let node = self.get_node(token, node_idx);
                match node {
                    Node::Leaf {
                        len, keys, vals, ..
                    } => {
                        let l = *len as usize;
                        let mut idx = 0;
                        while idx < l {
                            let k = unsafe { keys.get_unchecked(idx).assume_init_ref() };
                            match key.cmp(k) {
                                std::cmp::Ordering::Equal => {
                                    return Some(unsafe {
                                        vals.get_unchecked(idx).assume_init_ref().borrow(token)
                                    });
                                }
                                std::cmp::Ordering::Greater => idx += 1,
                                std::cmp::Ordering::Less => return None,
                            }
                        }
                        return None;
                    }
                    Node::Internal {
                        len,
                        keys,
                        children,
                    } => {
                        let l = *len as usize;
                        let mut idx = 0;
                        while idx < l {
                            let k = unsafe { keys.get_unchecked(idx).assume_init_ref() };
                            if key < k {
                                break;
                            }
                            idx += 1;
                        }
                        node_idx = children[idx];
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, key: &K) -> Option<&'a mut V>
    where
        K: Ord,
    {
        if let Some(root_idx) = self.root {
            let mut node_idx = root_idx;
            loop {
                let is_leaf = self.get_node(token, node_idx).is_leaf();

                if is_leaf {
                    let node = self.get_node_mut(token, node_idx);
                    if let Node::Leaf {
                        len, keys, vals, ..
                    } = node
                    {
                        let l = *len as usize;
                        let mut idx = 0;
                        while idx < l {
                            let k = unsafe { keys.get_unchecked(idx).assume_init_ref() };
                            match key.cmp(k) {
                                std::cmp::Ordering::Equal => {
                                    // We have exclusive access to the node (via get_node_mut which used the token),
                                    // so we have exclusive access to the GhostCell.
                                    // We can skip the token for inner access.
                                    return Some(unsafe {
                                        vals.get_unchecked_mut(idx).assume_init_mut().get_mut()
                                    });
                                }
                                std::cmp::Ordering::Greater => idx += 1,
                                std::cmp::Ordering::Less => return None,
                            }
                        }
                        return None;
                    } else {
                        unreachable!()
                    }
                } else {
                    let node = self.get_node(token, node_idx);
                    if let Node::Internal {
                        len,
                        keys,
                        children,
                    } = node
                    {
                        let l = *len as usize;
                        let mut idx = 0;
                        while idx < l {
                            let k = unsafe { keys.get_unchecked(idx).assume_init_ref() };
                            if key < k {
                                break;
                            }
                            idx += 1;
                        }
                        node_idx = children[idx];
                    } else {
                        unreachable!()
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> Iter<'a, 'brand, K, V> {
        let mut leaf_idx = None;
        if let Some(mut idx) = self.root {
            loop {
                let node = self.get_node(token, idx);
                match node {
                    Node::Leaf { .. } => {
                        leaf_idx = Some(idx);
                        break;
                    }
                    Node::Internal { children, .. } => {
                        idx = children[0];
                    }
                }
            }
        }

        Iter {
            tree: self,
            token,
            leaf_idx,
            key_idx: 0,
        }
    }

    pub fn insert(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V>
    where
        K: Ord + Clone,
    {
        if self.root.is_none() {
            let mut root = Node::new_leaf();
            root.leaf_insert(0, key, GhostCell::new(value));
            self.root = Some(self.pool.alloc(token, root));
            self.len += 1;
            return None;
        }

        let root_idx = self.root.unwrap();
        let is_full = self.get_node(token, root_idx).is_full();

        let res = if is_full {
            let mut new_root = Node::new_internal();
            new_root.children_mut()[0] = root_idx;

            let new_root_idx = self.pool.alloc(token, new_root);
            self.root = Some(new_root_idx);

            self.split_child(token, new_root_idx, 0);
            self.insert_non_full(token, new_root_idx, key, value)
        } else {
            self.insert_non_full(token, root_idx, key, value)
        };

        if res.is_none() {
            self.len += 1;
        }
        res
    }

    fn split_child(
        &self,
        token: &mut GhostToken<'brand>,
        parent_idx: usize,
        child_index_in_parent: usize,
    ) where
        K: Clone,
    {
        let child_idx = self
            .get_node(token, parent_idx)
            .child_at(child_index_in_parent);

        let sibling_idx = if self.get_node(token, child_idx).is_leaf() {
            self.pool.alloc(token, Node::new_leaf())
        } else {
            self.pool.alloc(token, Node::new_internal())
        };

        let pool_slice = self.pool.as_mut_slice(token);
        let ptr = pool_slice.as_mut_ptr();

        unsafe {
            let parent = match &mut *ptr.add(parent_idx) {
                PoolSlot::Occupied(n) => n,
                _ => unreachable!(),
            };
            let child = match &mut *ptr.add(child_idx) {
                PoolSlot::Occupied(n) => n,
                _ => unreachable!(),
            };
            let sibling = match &mut *ptr.add(sibling_idx) {
                PoolSlot::Occupied(n) => n,
                _ => unreachable!(),
            };

            if let Node::Internal {
                len: c_len,
                keys: c_keys,
                children: c_children,
            } = child
            {
                let mut s_keys = MaybeUninit::<[MaybeUninit<K>; MAX_KEYS]>::uninit().assume_init();
                let mut s_children = [0; MAX_CHILDREN];

                let median_idx = B - 1;
                let median_key = c_keys.get_unchecked(median_idx).assume_init_read();

                let right_start = median_idx + 1;
                let right_count = *c_len as usize - right_start;

                ptr::copy_nonoverlapping(
                    c_keys.as_ptr().add(right_start),
                    s_keys.as_mut_ptr(),
                    right_count,
                );

                ptr::copy_nonoverlapping(
                    c_children.as_ptr().add(right_start),
                    s_children.as_mut_ptr(),
                    right_count + 1,
                );

                *c_len = median_idx as u16;

                *sibling = Node::Internal {
                    len: right_count as u16,
                    keys: s_keys,
                    children: s_children,
                };

                parent.internal_insert(child_index_in_parent, median_key, sibling_idx);
            } else if let Node::Leaf {
                len: c_len,
                keys: c_keys,
                vals: c_vals,
                next: c_next,
            } = child
            {
                let split_idx = B - 1;
                let median_key = c_keys.get_unchecked(split_idx).assume_init_ref().clone();
                let mut s_keys = MaybeUninit::<[MaybeUninit<K>; MAX_KEYS]>::uninit().assume_init();
                let mut s_vals =
                    MaybeUninit::<[MaybeUninit<GhostCell<'brand, V>>; MAX_KEYS]>::uninit()
                        .assume_init();

                let count = *c_len as usize - split_idx;

                ptr::copy_nonoverlapping(
                    c_keys.as_ptr().add(split_idx),
                    s_keys.as_mut_ptr(),
                    count,
                );
                ptr::copy_nonoverlapping(
                    c_vals.as_ptr().add(split_idx),
                    s_vals.as_mut_ptr(),
                    count,
                );

                *c_len = split_idx as u16;
                let old_next = *c_next;
                *c_next = Some(sibling_idx);

                *sibling = Node::Leaf {
                    len: count as u16,
                    keys: s_keys,
                    vals: s_vals,
                    next: old_next,
                };

                parent.internal_insert(child_index_in_parent, median_key, sibling_idx);
            }
        }
    }

    fn insert_non_full(
        &mut self,
        token: &mut GhostToken<'brand>,
        node_idx: usize,
        key: K,
        value: V,
    ) -> Option<V>
    where
        K: Ord + Clone,
    {
        let node = self.get_node_mut(token, node_idx);

        match node {
            Node::Leaf {
                len, keys, vals, ..
            } => {
                let l = *len as usize;
                let mut idx = 0;
                while idx < l {
                    let k = unsafe { keys.get_unchecked(idx).assume_init_ref() };
                    match key.cmp(k) {
                        std::cmp::Ordering::Greater => idx += 1,
                        std::cmp::Ordering::Equal => {
                            let cell = unsafe { vals.get_unchecked_mut(idx).assume_init_mut() };
                            // We have exclusive access, so use get_mut() to swap without token
                            let val_mut = cell.get_mut();
                            let old = std::mem::replace(val_mut, value);
                            return Some(old);
                        }
                        std::cmp::Ordering::Less => break,
                    }
                }
                node.leaf_insert(idx, key, GhostCell::new(value));
                None
            }
            Node::Internal {
                len,
                keys,
                children,
            } => {
                let l = *len as usize;
                let mut idx = 0;
                while idx < l {
                    let k = unsafe { keys.get_unchecked(idx).assume_init_ref() };
                    if key < *k {
                        break;
                    }
                    idx += 1;
                }
                let child_idx = children[idx];

                if self.get_node(token, child_idx).is_full() {
                    self.split_child(token, node_idx, idx);
                    let k = unsafe { self.get_node(token, node_idx).key_at(idx) };
                    if key > *k {
                        idx += 1;
                    }
                    let new_child_idx = self.get_node(token, node_idx).child_at(idx);
                    self.insert_non_full(token, new_child_idx, key, value)
                } else {
                    self.insert_non_full(token, child_idx, key, value)
                }
            }
        }
    }
}

impl<'brand, K, V> Default for BrandedBPlusTree<'brand, K, V> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Iter<'a, 'brand, K, V> {
    tree: &'a BrandedBPlusTree<'brand, K, V>,
    token: &'a GhostToken<'brand>,
    leaf_idx: Option<usize>,
    key_idx: usize,
}

impl<'a, 'brand, K, V> Iterator for Iter<'a, 'brand, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.leaf_idx?;
        let node = self.tree.get_node(self.token, idx);
        if let Node::Leaf {
            len,
            keys,
            vals,
            next,
        } = node
        {
            if self.key_idx < *len as usize {
                let k = unsafe { keys.get_unchecked(self.key_idx).assume_init_ref() };
                let v = unsafe {
                    vals.get_unchecked(self.key_idx)
                        .assume_init_ref()
                        .borrow(self.token)
                };
                self.key_idx += 1;
                return Some((k, v));
            } else {
                self.leaf_idx = *next;
                self.key_idx = 0;
                return self.next();
            }
        } else {
            self.leaf_idx = None;
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_basic_insert_get() {
        GhostToken::new(|mut token| {
            let mut tree = BrandedBPlusTree::new();
            tree.insert(&mut token, 1, 100);
            assert_eq!(tree.get(&token, &1), Some(&100));
            assert_eq!(tree.len(), 1);

            tree.insert(&mut token, 2, 200);
            assert_eq!(tree.get(&token, &2), Some(&200));
            assert_eq!(tree.len(), 2);
        });
    }

    #[test]
    fn test_split_root() {
        GhostToken::new(|mut token| {
            let mut tree = BrandedBPlusTree::new();
            // Insert enough to split root. B=6. Max keys=11.
            // Insert 20 items.
            for i in 0..20 {
                tree.insert(&mut token, i, i * 10);
            }

            assert_eq!(tree.len(), 20);
            for i in 0..20 {
                assert_eq!(tree.get(&token, &i), Some(&(i * 10)));
            }
        });
    }

    #[test]
    fn test_iter() {
        GhostToken::new(|mut token| {
            let mut tree = BrandedBPlusTree::new();
            for i in 0..100 {
                tree.insert(&mut token, i, i);
            }

            let mut count = 0;
            for (k, v) in tree.iter(&token) {
                assert_eq!(*k, count);
                assert_eq!(*v, count);
                count += 1;
            }
            assert_eq!(count, 100);
        });
    }
}
