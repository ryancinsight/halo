//! `BrandedSkipList` — a chunked probabilistic data structure with token-gated values.
//!
//! This implementation is a "Chunked Skip List" (or Unrolled Skip List).
//! Each node stores multiple key-value pairs (up to `CHUNK_SIZE`), improving cache locality
//! and reducing the number of pointer chases. This brings performance closer to B-Trees.
//!
//! Optimization details:
//! - **Chunking**: Nodes hold up to 16 elements. Linear search within chunks is highly efficient.
//! - **Memory**: `NodeIdx` (u32) indices to reduce memory footprint.
//! - **Splitting**: When a chunk fills, it splits into two, promoting the split key to the skip list index.
//! - **Branding**: Indices are branded (`NodeIdx<'brand>`) to prevent misuse across tokens.
//!
//! Access is controlled via `GhostToken`.

use crate::collections::{BrandedCollection, ZeroCopyMapOps};
use crate::{BrandedVec, GhostToken};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::mem::MaybeUninit;

const MAX_LEVEL: usize = 16;
const CHUNK_SIZE: usize = 16;

/// A branded index into the skip list storage.
///
/// Wraps a `u32` to provide type safety and prevent mixing indices from different
/// branded contexts or raw integers.
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

/// Simple Xorshift RNG for level generation.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF_CAFE } else { seed },
        }
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

struct NodeData<'brand, K, V> {
    keys: [MaybeUninit<K>; CHUNK_SIZE],
    vals: [MaybeUninit<V>; CHUNK_SIZE],
    len: u8,
    level: u8,
    link_offset: u32,
    next_chunk: NodeIdx<'brand>, // Optimization: Direct link to next chunk (level 0)
}

impl<'brand, K, V> NodeData<'brand, K, V> {
    fn new(level: u8, link_offset: u32) -> Self {
        // Safe because MaybeUninit doesn't require initialization
        let keys = unsafe { MaybeUninit::uninit().assume_init() };
        let vals = unsafe { MaybeUninit::uninit().assume_init() };
        Self {
            keys,
            vals,
            len: 0,
            level,
            link_offset,
            next_chunk: NodeIdx::NONE,
        }
    }

    #[inline(always)]
    unsafe fn key_at(&self, index: usize) -> &K {
        self.keys.get_unchecked(index).assume_init_ref()
    }

    #[inline(always)]
    unsafe fn val_at(&self, index: usize) -> &V {
        self.vals.get_unchecked(index).assume_init_ref()
    }

    #[inline(always)]
    unsafe fn val_at_mut(&mut self, index: usize) -> &mut V {
        self.vals.get_unchecked_mut(index).assume_init_mut()
    }
}

/// A Chunked SkipList map with token-gated values.
pub struct BrandedSkipList<'brand, K, V> {
    nodes: BrandedVec<'brand, NodeData<'brand, K, V>>,
    links: BrandedVec<'brand, NodeIdx<'brand>>, // indices into `nodes`
    head_links: [NodeIdx<'brand>; MAX_LEVEL],
    len: usize,
    max_level: usize,
    rng: XorShift64,
}

impl<'brand, K, V> BrandedSkipList<'brand, K, V> {
    /// Creates a new empty SkipList.
    pub fn new() -> Self {
        Self {
            nodes: BrandedVec::new(),
            links: BrandedVec::new(),
            head_links: [NodeIdx::NONE; MAX_LEVEL],
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
            head_links: [NodeIdx::NONE; MAX_LEVEL],
            len: 0,
            max_level: 0,
            rng: XorShift64::new(seed),
        }
    }

    fn random_level(&mut self) -> usize {
        let mut level = 1;
        // p=0.25
        while level < MAX_LEVEL && (self.rng.next() & 0x3) == 0 {
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

    /// Finds the entry.
    fn find_entry<'a, Q: ?Sized>(
        &'a self,
        token: &'a GhostToken<'brand>,
        key: &Q,
    ) -> Option<(&'a K, &'a V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut curr = NodeIdx::NONE;
        let mut level = self.max_level.saturating_sub(1);

        if self.max_level == 0 {
            return None;
        }

        loop {
            // Find next chunk
            // Optimization: if level is 0, use next_chunk directly if we are at a node
            let next_idx = if level == 0 && curr.is_some() {
                unsafe { self.nodes.get_unchecked(token, curr.index()).next_chunk }
            } else {
                self.get_next_unchecked(token, curr, level)
            };

            if next_idx.is_some() {
                unsafe {
                    let next_node = self.nodes.get_unchecked(token, next_idx.index());
                    // Check first key of next node
                    // Assuming node is not empty (invariant)
                    let first_key = next_node.key_at(0);

                    if first_key.borrow() <= key {
                        curr = next_idx;
                        continue;
                    }
                }
            }

            if level == 0 {
                break;
            }
            level -= 1;
        }

        // We are at `curr`. The key should be in `curr` or it doesn't exist.
        if curr.is_some() {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr.index());
                // Linear search in chunk
                for i in 0..node.len as usize {
                    let k = node.key_at(i);
                    match k.borrow().cmp(key) {
                        Ordering::Equal => return Some((k, node.val_at(i))),
                        Ordering::Greater => return None, // Chunk is sorted
                        Ordering::Less => {}
                    }
                }
            }
        }

        None
    }

    pub fn get_mut<'a, Q: ?Sized>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        key: &Q,
    ) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        // Copy logic from find_entry but return mut ref
        let mut curr = NodeIdx::NONE;
        let mut level = self.max_level.saturating_sub(1);

        if self.max_level == 0 {
            return None;
        }

        loop {
            let next_idx = if level == 0 && curr.is_some() {
                unsafe { self.nodes.get_unchecked(token, curr.index()).next_chunk }
            } else {
                self.get_next_unchecked(token, curr, level)
            };

            if next_idx.is_some() {
                unsafe {
                    let next_node = self.nodes.get_unchecked(token, next_idx.index());
                    if next_node.key_at(0).borrow() <= key {
                        curr = next_idx;
                        continue;
                    }
                }
            }
            if level == 0 {
                break;
            }
            level -= 1;
        }

        if curr.is_some() {
            unsafe {
                let node = self.nodes.get_unchecked_mut(token, curr.index());
                for i in 0..node.len as usize {
                    if node.key_at(i).borrow() == key {
                        return Some(node.val_at_mut(i));
                    }
                    if node.key_at(i).borrow() > key {
                        return None;
                    }
                }
            }
        }
        None
    }

    // Helper
    fn get_next(
        &self,
        token: &GhostToken<'brand>,
        curr: NodeIdx<'brand>,
        level: usize,
    ) -> NodeIdx<'brand> {
        self.get_next_unchecked(token, curr, level)
    }

    fn get_next_unchecked(
        &self,
        token: &GhostToken<'brand>,
        curr: NodeIdx<'brand>,
        level: usize,
    ) -> NodeIdx<'brand> {
        if curr.is_some() {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr.index());
                let offset = node.link_offset as usize + level;
                *self.links.get_unchecked(token, offset)
            }
        } else {
            self.head_links[level]
        }
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V> {
        let mut update = [NodeIdx::NONE; MAX_LEVEL];
        let mut curr = NodeIdx::NONE;
        let mut level = self.max_level.saturating_sub(1);

        // Find predecessors
        if self.max_level > 0 {
            loop {
                // Optimization: use next_chunk for level 0
                let next_idx = if level == 0 && curr.is_some() {
                    unsafe { self.nodes.get_unchecked(token, curr.index()).next_chunk }
                } else {
                    self.get_next_unchecked(token, curr, level)
                };

                if next_idx.is_some() {
                    unsafe {
                        let next_node = self.nodes.get_unchecked(token, next_idx.index());
                        if next_node.key_at(0) <= &key {
                            curr = next_idx;
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

        // `curr` is the node where `key` belongs.
        if curr.is_some() {
            // Check if exists in `curr`
            unsafe {
                let node = self.nodes.get_unchecked_mut(token, curr.index());
                for i in 0..node.len as usize {
                    if node.key_at(i) == &key {
                        let old = std::mem::replace(node.val_at_mut(i), value);
                        return Some(old);
                    }
                }

                // Not found in `curr`. Insert into `curr`.
                if (node.len as usize) < CHUNK_SIZE {
                    self.insert_into_leaf(token, curr, key, value);
                    self.len += 1;
                    return None;
                }
            }

            // `curr` is full. Split.
            self.split_and_insert(token, curr, &mut update, key, value);
            self.len += 1;
            return None;
        }

        // List is empty or key is smaller than everything?
        // If empty:
        if self.len == 0 {
            self.create_first_node(token, key, value);
            self.len += 1;
            return None;
        }

        let first_node_idx = self.head_links[0];
        if first_node_idx.is_some() {
            // Insert into first node
            unsafe {
                let node = self.nodes.get_unchecked_mut(token, first_node_idx.index());
                if (node.len as usize) < CHUNK_SIZE {
                    self.insert_into_leaf(token, first_node_idx, key, value);
                    self.len += 1;
                    return None;
                }
            }
            self.split_and_insert(token, first_node_idx, &mut update, key, value);
            self.len += 1;
            return None;
        }

        self.create_first_node(token, key, value);
        self.len += 1;
        None
    }

    fn create_first_node(&mut self, _token: &mut GhostToken<'brand>, key: K, value: V) {
        let level = self.random_level();
        if level > self.max_level {
            self.max_level = level;
        }

        let link_offset = self.links.len() as u32;
        let node_idx = NodeIdx::new(self.nodes.len());

        for _ in 0..level {
            self.links.push(NodeIdx::NONE);
        }
        for i in 0..level {
            self.head_links[i] = node_idx;
        }

        let mut node = NodeData::new(level as u8, link_offset);
        node.keys[0].write(key);
        node.vals[0].write(value);
        node.len = 1;
        node.next_chunk = NodeIdx::NONE;
        self.nodes.push(node);
    }

    fn insert_into_leaf(
        &mut self,
        token: &mut GhostToken<'brand>,
        node_idx: NodeIdx<'brand>,
        key: K,
        value: V,
    ) {
        unsafe {
            let node = self.nodes.get_unchecked_mut(token, node_idx.index());
            // Find position
            let mut pos = node.len as usize;
            for i in 0..node.len as usize {
                if node.key_at(i) > &key {
                    pos = i;
                    break;
                }
            }

            // Shift
            if pos < node.len as usize {
                std::ptr::copy(
                    node.keys.as_ptr().add(pos),
                    node.keys.as_mut_ptr().add(pos + 1),
                    node.len as usize - pos,
                );
                std::ptr::copy(
                    node.vals.as_ptr().add(pos),
                    node.vals.as_mut_ptr().add(pos + 1),
                    node.len as usize - pos,
                );
            }

            node.keys[pos].write(key);
            node.vals[pos].write(value);
            node.len += 1;
        }
    }

    fn split_and_insert(
        &mut self,
        token: &mut GhostToken<'brand>,
        node_idx: NodeIdx<'brand>,
        update: &mut [NodeIdx<'brand>],
        key: K,
        value: V,
    ) {
        // 1. Create new node
        let new_level = self.random_level();
        if new_level > self.max_level {
            for i in self.max_level..new_level {
                update[i] = NodeIdx::NONE;
            }
            self.max_level = new_level;
        }

        let new_link_offset = self.links.len() as u32;
        let new_node_idx = NodeIdx::new(self.nodes.len());
        for _ in 0..new_level {
            self.links.push(NodeIdx::NONE);
        }

        let mut new_node = NodeData::new(new_level as u8, new_link_offset);

        // 2. Distribute keys
        unsafe {
            let node = self.nodes.get_unchecked_mut(token, node_idx.index());

            // Update next_chunk
            new_node.next_chunk = node.next_chunk;
            node.next_chunk = new_node_idx;

            // Find insert pos
            let mut pos = node.len as usize;
            for i in 0..node.len as usize {
                if node.key_at(i) > &key {
                    pos = i;
                    break;
                }
            }

            let split_idx = CHUNK_SIZE / 2;

            if pos < split_idx {
                let move_count = CHUNK_SIZE - (split_idx - 1);
                let src_start = split_idx - 1;

                std::ptr::copy_nonoverlapping(
                    node.keys.as_ptr().add(src_start),
                    new_node.keys.as_mut_ptr(),
                    move_count,
                );
                std::ptr::copy_nonoverlapping(
                    node.vals.as_ptr().add(src_start),
                    new_node.vals.as_mut_ptr(),
                    move_count,
                );
                new_node.len = move_count as u8;
                node.len = src_start as u8;

                // Insert key into node
                self.insert_into_leaf(token, node_idx, key, value);
            } else {
                let move_count = CHUNK_SIZE - split_idx;
                std::ptr::copy_nonoverlapping(
                    node.keys.as_ptr().add(split_idx),
                    new_node.keys.as_mut_ptr(),
                    move_count,
                );
                std::ptr::copy_nonoverlapping(
                    node.vals.as_ptr().add(split_idx),
                    new_node.vals.as_mut_ptr(),
                    move_count,
                );
                new_node.len = move_count as u8;
                node.len = split_idx as u8;

                // Insert key into new_node
                let rel_pos = pos - split_idx;
                if rel_pos < new_node.len as usize {
                    std::ptr::copy(
                        new_node.keys.as_ptr().add(rel_pos),
                        new_node.keys.as_mut_ptr().add(rel_pos + 1),
                        new_node.len as usize - rel_pos,
                    );
                    std::ptr::copy(
                        new_node.vals.as_ptr().add(rel_pos),
                        new_node.vals.as_mut_ptr().add(rel_pos + 1),
                        new_node.len as usize - rel_pos,
                    );
                }
                new_node.keys[rel_pos].write(key);
                new_node.vals[rel_pos].write(value);
                new_node.len += 1;
            }
        }

        self.nodes.push(new_node);

        // 3. Update links
        for i in 0..new_level {
            let pred_idx = update[i];

            if pred_idx.is_none() {
                let old_head = self.head_links[i];
                unsafe {
                    *self
                        .links
                        .get_unchecked_mut(token, new_link_offset as usize + i) = old_head;
                }
                self.head_links[i] = new_node_idx;
            } else {
                unsafe {
                    let pred_node = self.nodes.get_unchecked(token, pred_idx.index());
                    let offset = pred_node.link_offset as usize + i;
                    let old_next = *self.links.get_unchecked(token, offset);

                    *self
                        .links
                        .get_unchecked_mut(token, new_link_offset as usize + i) = old_next;
                    *self.links.get_unchecked_mut(token, offset) = new_node_idx;
                }
            }
        }
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
        let mut curr = self.head_links[0];
        while curr.is_some() {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr.index());
                for i in 0..node.len as usize {
                    let k = node.key_at(i);
                    let v = node.val_at(i);
                    if f(k, v) {
                        return Some((k, v));
                    }
                }
                curr = node.next_chunk; // Optimization: use next_chunk
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
        while curr.is_some() {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr.index());
                for i in 0..node.len as usize {
                    if !f(node.key_at(i), node.val_at(i)) {
                        return false;
                    }
                }
                curr = node.next_chunk; // Optimization
            }
        }
        true
    }
}

// Iterators
pub struct Iter<'a, 'brand, K, V> {
    list: &'a BrandedSkipList<'brand, K, V>,
    token: &'a GhostToken<'brand>,
    curr: NodeIdx<'brand>,
    idx: usize,
}

impl<'a, 'brand, K, V> Iterator for Iter<'a, 'brand, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr.is_none() {
            return None;
        }

        unsafe {
            let node = self.list.nodes.get_unchecked(self.token, self.curr.index());
            if self.idx < node.len as usize {
                let k = node.key_at(self.idx);
                let v = node.val_at(self.idx);
                self.idx += 1;
                return Some((k, v));
            } else {
                self.curr = node.next_chunk; // Optimization
                self.idx = 0;
                return self.next();
            }
        }
    }
}

pub struct IterMut<'a, 'brand, K, V> {
    list: &'a BrandedSkipList<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
    curr: NodeIdx<'brand>,
    idx: usize,
}

impl<'a, 'brand, K, V> Iterator for IterMut<'a, 'brand, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr.is_none() {
            return None;
        }

        unsafe {
            let node = self
                .list
                .nodes
                .get_unchecked_mut(self.token, self.curr.index());

            if self.idx < node.len as usize {
                let k_ptr = node.key_at(self.idx) as *const K;
                let v_ptr = node.val_at_mut(self.idx) as *mut V;

                self.idx += 1;

                return Some((&*k_ptr, &mut *v_ptr));
            } else {
                let next_curr = node.next_chunk; // Optimization

                self.curr = next_curr;
                self.idx = 0;
                return self.next();
            }
        }
    }
}

impl<'brand, K, V> BrandedSkipList<'brand, K, V> {
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> Iter<'a, 'brand, K, V> {
        Iter {
            list: self,
            token,
            curr: self.head_links[0],
            idx: 0,
        }
    }

    pub fn iter_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> IterMut<'a, 'brand, K, V> {
        IterMut {
            list: self,
            curr: self.head_links[0],
            token,
            idx: 0,
        }
    }
}

impl<'brand, K, V> Default for BrandedSkipList<'brand, K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_skip_list_chunked_basic() {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();

            // Insert 20 items (more than one chunk)
            for i in 0..20 {
                list.insert(&mut token, i, i * 10);
            }

            assert_eq!(list.len(), 20);

            for i in 0..20 {
                assert_eq!(*list.get(&token, &i).unwrap(), i * 10);
            }

            // Check iterator
            let vec: Vec<_> = list.iter(&token).map(|(k, v)| (*k, *v)).collect();
            assert_eq!(vec.len(), 20);
            for i in 0..20 {
                assert_eq!(vec[i], (i, i * 10));
            }
        });
    }

    #[test]
    fn test_skip_list_large_insert() {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            // Insert enough to force multiple splits and levels
            for i in 0..100 {
                list.insert(&mut token, i, i);
            }
            assert_eq!(list.len(), 100);
            for i in 0..100 {
                assert_eq!(*list.get(&token, &i).unwrap(), i);
            }
        });
    }

    #[test]
    fn test_skip_list_iter_mut() {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            for i in 0..20 {
                list.insert(&mut token, i, i);
            }

            for (_, v) in list.iter_mut(&mut token) {
                *v += 1;
            }

            for i in 0..20 {
                assert_eq!(*list.get(&token, &i).unwrap(), i + 1);
            }
        });
    }

    #[test]
    fn test_skip_list_chunk_splits() {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            // CHUNK_SIZE is 16.
            // Insert 17 items. Should split.
            for i in 0..17 {
                list.insert(&mut token, i, i);
            }
            assert_eq!(list.len(), 17);

            // Check order
            let keys: Vec<_> = list.iter(&token).map(|(k, _)| *k).collect();
            assert_eq!(keys.len(), 17);
            for i in 0..17 {
                assert_eq!(keys[i], i);
            }
        });
    }
}
