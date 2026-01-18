//! `BrandedSkipList` — a chunked probabilistic data structure with token-gated values.
//!
//! This implementation is a "Chunked Skip List" (or Unrolled Skip List).
//! Each node stores multiple key-value pairs (up to `CHUNK_SIZE`), improving cache locality
//! and reducing the number of pointer chases. This brings performance closer to B-Trees.
//!
//! Optimization details:
//! - **Chunking**: Nodes hold up to 16 elements. Linear search within chunks is highly efficient.
//! - **Memory**: `u32` indices, `MaybeUninit` for lazy initialization.
//! - **Splitting**: When a chunk fills, it splits into two, promoting the split key to the skip list index.
//!
//! Access is controlled via `GhostToken`.

use crate::{GhostCell, GhostToken, BrandedVec};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::mem::MaybeUninit;
use crate::collections::{BrandedCollection, ZeroCopyMapOps};

const MAX_LEVEL: usize = 16;
const NONE: u32 = u32::MAX;
const CHUNK_SIZE: usize = 16;
// Split threshold: typically half full, but we can fill up to CHUNK_SIZE
const SPLIT_SIZE: usize = CHUNK_SIZE;

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
    keys: [MaybeUninit<K>; CHUNK_SIZE],
    vals: [MaybeUninit<V>; CHUNK_SIZE],
    len: u8,
    level: u8,
    link_offset: u32,
}

impl<K, V> NodeData<K, V> {
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
    nodes: BrandedVec<'brand, NodeData<K, V>>,
    links: BrandedVec<'brand, u32>, // indices into `nodes`
    head_links: [u32; MAX_LEVEL],
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
            head_links: [NONE; MAX_LEVEL],
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
            head_links: [NONE; MAX_LEVEL],
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
    fn find_entry<'a, Q: ?Sized>(&'a self, token: &'a GhostToken<'brand>, key: &Q) -> Option<(&'a K, &'a V)>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        let mut curr: u32 = NONE;
        let mut level = self.max_level.saturating_sub(1);

        if self.max_level == 0 {
            return None;
        }

        loop {
            // Find next chunk
            let next_idx = self.get_next(token, curr, level);

            if next_idx != NONE {
                unsafe {
                    let next_node = self.nodes.get_unchecked(token, next_idx as usize);
                    // Check first key of next node
                    // Assuming node is not empty (invariant)
                    let first_key = next_node.key_at(0);

                    if first_key.borrow() <= key {
                        // Move to next node if key is potentially inside or after
                        // However, strictly speaking, in a chunked list, we index by the *last* key or *first* key?
                        // Usually, the index points to chunks where all keys >= index key.
                        // Let's use simplified logic:
                        // Scan forward at this level as long as next_node.max_key < key?
                        // Or typical skip list: next_node.key < key.
                        // Since next_node contains a range, we should check if `key` could be in `next_node` or after.

                        // We need to look at the LAST key of next_node to decide if we skip over it?
                        // Or simpler: The skip list indexes the FIRST key of each chunk.
                        // So if `key >= next_node.first_key`, we *might* go there.
                        // We should go to the rightmost node such that `node.first_key <= key`.

                        // So:
                        if first_key.borrow() <= key {
                            curr = next_idx;
                            continue;
                        }
                    }
                }
            }

            if level == 0 {
                break;
            }
            level -= 1;
        }

        // We are at `curr`. The key should be in `curr` or it doesn't exist.
        if curr != NONE {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr as usize);
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

    pub fn get_mut<'a, Q: ?Sized>(&'a self, token: &'a mut GhostToken<'brand>, key: &Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        // Copy logic from find_entry but return mut ref
        let mut curr: u32 = NONE;
        let mut level = self.max_level.saturating_sub(1);

        if self.max_level == 0 {
            return None;
        }

        loop {
            let next_idx = self.get_next_unchecked(token, curr, level);
            if next_idx != NONE {
                unsafe {
                    let next_node = self.nodes.get_unchecked(token, next_idx as usize);
                    if next_node.key_at(0).borrow() <= key {
                        curr = next_idx;
                        continue;
                    }
                }
            }
            if level == 0 { break; }
            level -= 1;
        }

        if curr != NONE {
            unsafe {
                let node = self.nodes.get_unchecked_mut(token, curr as usize);
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
    fn get_next(&self, token: &GhostToken<'brand>, curr: u32, level: usize) -> u32 {
        self.get_next_unchecked(token, curr, level)
    }

    fn get_next_unchecked(&self, token: &GhostToken<'brand>, curr: u32, level: usize) -> u32 {
        if curr != NONE {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr as usize);
                let offset = node.link_offset as usize + level;
                *self.links.get_unchecked(token, offset)
            }
        } else {
            self.head_links[level]
        }
    }

    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, key: K, value: V) -> Option<V> {
        let mut update = [NONE; MAX_LEVEL];
        let mut curr: u32 = NONE;
        let mut level = self.max_level.saturating_sub(1);

        // Find predecessors
        if self.max_level > 0 {
            loop {
                let next_idx = self.get_next_unchecked(token, curr, level);
                if next_idx != NONE {
                    unsafe {
                        let next_node = self.nodes.get_unchecked(token, next_idx as usize);
                        // We move to next_node if its first key <= key.
                        // This finds the rightmost node starting before or at `key`.
                        if next_node.key_at(0) <= &key {
                            curr = next_idx;
                            continue;
                        }
                    }
                }
                update[level] = curr;
                if level == 0 { break; }
                level -= 1;
            }
        }

        // `curr` is the node where `key` belongs.
        if curr != NONE {
            // Check if exists in `curr`
            unsafe {
                let node = self.nodes.get_unchecked_mut(token, curr as usize);
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

        // If we are here, `curr` is NONE, meaning `key` is smaller than first node's first key?
        // Wait, if `key` < first node's first key, `curr` would remain NONE (head).
        // But we should insert into the first node (head's next).
        // The loop condition `next_node.key_at(0) <= &key` skips nodes starting after key.
        // So `curr` is the node starting <= key.

        // If `curr` is NONE, it means `key` < `head_links[0].key_at(0)`.
        // So we should insert into `head_links[0]`.
        // Or if list is empty.

        let first_node_idx = self.head_links[0];
        if first_node_idx != NONE {
             // Insert into first node
             unsafe {
                 let node = self.nodes.get_unchecked_mut(token, first_node_idx as usize);
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

        // Should be covered by len == 0 check, but safe fallback
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
        let node_idx = self.nodes.len() as u32;

        for _ in 0..level {
            self.links.push(NONE);
        }
        for i in 0..level {
            self.head_links[i] = node_idx;
        }

        let mut node = NodeData::new(level as u8, link_offset);
        unsafe {
            node.keys[0].write(key);
            node.vals[0].write(value);
        }
        node.len = 1;
        self.nodes.push(node);
    }

    fn insert_into_leaf(&mut self, token: &mut GhostToken<'brand>, node_idx: u32, key: K, value: V) {
        unsafe {
            let node = self.nodes.get_unchecked_mut(token, node_idx as usize);
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
                    node.len as usize - pos
                );
                std::ptr::copy(
                    node.vals.as_ptr().add(pos),
                    node.vals.as_mut_ptr().add(pos + 1),
                    node.len as usize - pos
                );
            }

            node.keys[pos].write(key);
            node.vals[pos].write(value);
            node.len += 1;
        }
    }

    fn split_and_insert(&mut self, token: &mut GhostToken<'brand>, node_idx: u32, update: &mut [u32], key: K, value: V) {
        // 1. Create new node
        let new_level = self.random_level();
        if new_level > self.max_level {
            for i in self.max_level..new_level {
                update[i] = NONE;
            }
            self.max_level = new_level;
        }

        let new_link_offset = self.links.len() as u32;
        let new_node_idx = self.nodes.len() as u32;
        for _ in 0..new_level {
            self.links.push(NONE);
        }

        let mut new_node = NodeData::new(new_level as u8, new_link_offset);

        // 2. Distribute keys between `node` and `new_node`
        // Also insert `key`
        unsafe {
            let node = self.nodes.get_unchecked_mut(token, node_idx as usize);

            // Create a temporary buffer to sort/split keys
            // Because CHUNK_SIZE is small (16), we can stack allocate or just shuffle.
            // Simplified:
            // - Determine where key goes.
            // - Total items = CHUNK_SIZE + 1.
            // - Split index = (CHUNK_SIZE + 1) / 2 = 8.
            // - First 8 go to `node`, rest to `new_node`.

            // To avoid allocs, we shift elements from `node` to `new_node`.
            // But we need to insert `key` too.

            // Find insert pos
            let mut pos = node.len as usize;
            for i in 0..node.len as usize {
                if node.key_at(i) > &key {
                    pos = i;
                    break;
                }
            }

            // We have `node.keys[0..16]` and `key`.
            // We want `node` to have `0..8`, `new_node` to have `9..17`.
            // Split point 8.

            let split_idx = CHUNK_SIZE / 2;

            // Move items to new_node
            // Case 1: Insert in first half
            // Case 2: Insert in second half

            if pos < split_idx {
                // key goes to left node.
                // Move [split_idx - 1 .. end] to new_node
                // Shift [pos .. split_idx - 1] in node
                // Insert key at pos

                let move_count = CHUNK_SIZE - (split_idx - 1); // e.g. 16 - 7 = 9 items
                let src_start = split_idx - 1;

                std::ptr::copy_nonoverlapping(
                    node.keys.as_ptr().add(src_start),
                    new_node.keys.as_mut_ptr(),
                    move_count
                );
                std::ptr::copy_nonoverlapping(
                    node.vals.as_ptr().add(src_start),
                    new_node.vals.as_mut_ptr(),
                    move_count
                );
                new_node.len = move_count as u8;
                node.len = src_start as u8;

                // Insert key into node
                self.insert_into_leaf(token, node_idx, key, value);

            } else {
                // key goes to right node (or exactly at split)
                // Move [split_idx .. end] to new_node.
                // Insert key into new_node.

                let move_count = CHUNK_SIZE - split_idx;
                std::ptr::copy_nonoverlapping(
                    node.keys.as_ptr().add(split_idx),
                    new_node.keys.as_mut_ptr(),
                    move_count
                );
                std::ptr::copy_nonoverlapping(
                    node.vals.as_ptr().add(split_idx),
                    new_node.vals.as_mut_ptr(),
                    move_count
                );
                new_node.len = move_count as u8;
                node.len = split_idx as u8;

                // Insert key into new_node
                // new_node is not in `nodes` yet, pass ref?
                // insert_into_leaf takes index.
                // We handle it manually here since new_node is local.

                let rel_pos = pos - split_idx;
                if rel_pos < new_node.len as usize {
                     std::ptr::copy(
                        new_node.keys.as_ptr().add(rel_pos),
                        new_node.keys.as_mut_ptr().add(rel_pos + 1),
                        new_node.len as usize - rel_pos
                    );
                    std::ptr::copy(
                        new_node.vals.as_ptr().add(rel_pos),
                        new_node.vals.as_mut_ptr().add(rel_pos + 1),
                        new_node.len as usize - rel_pos
                    );
                }
                new_node.keys[rel_pos].write(key);
                new_node.vals[rel_pos].write(value);
                new_node.len += 1;
            }
        }

        self.nodes.push(new_node);

        // 3. Update links
        // new_node should be inserted after `node_idx`.
        // BUT `node_idx` might not be the predecessor at all levels!
        // `update` array contains predecessors for `key`.
        // Since `new_node` contains `key` (or keys > `key`), `update` is correct for `new_node`.

        // Wait, `update` points to nodes where `key` would be inserted.
        // `node_idx` is one of them (likely `curr` from find).
        // For levels where `update[i] == node_idx`, we link `node -> new_node`.
        // For levels where `update[i]` is something else (above `node`'s level), we link `pred -> new_node`.

        // Correct logic: `new_node` is inserted in the list. `update[i]` are its predecessors.

        for i in 0..new_level {
            let pred_idx = update[i];

            // If pred_idx is NONE, we update head
            if pred_idx == NONE {
                let old_head = self.head_links[i];
                unsafe {
                    *self.links.get_unchecked_mut(token, new_link_offset as usize + i) = old_head;
                }
                self.head_links[i] = new_node_idx;
            } else {
                unsafe {
                    let pred_node = self.nodes.get_unchecked(token, pred_idx as usize);
                    // Use index instead of pointer reference to avoid double mutable borrow conflict
                    // `get_unchecked` returns shared ref, but we need index to mutate via `links` vector.
                    let offset = pred_node.link_offset as usize + i;

                    // Read old next
                    let old_next = *self.links.get_unchecked(token, offset);

                    // Write new next to new node's links
                    *self.links.get_unchecked_mut(token, new_link_offset as usize + i) = old_next;

                    // Update predecessor's link
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

// ZeroCopyOps implementation needs to traverse chunks
impl<'brand, K, V> ZeroCopyMapOps<'brand, K, V> for BrandedSkipList<'brand, K, V> {
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<(&'a K, &'a V)>
    where
        F: Fn(&K, &V) -> bool,
    {
        let mut curr = self.head_links[0];
        while curr != NONE {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr as usize);
                for i in 0..node.len as usize {
                    let k = node.key_at(i);
                    let v = node.val_at(i);
                    if f(k, v) {
                        return Some((k, v));
                    }
                }
                let offset = node.link_offset as usize;
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
        while curr != NONE {
            unsafe {
                let node = self.nodes.get_unchecked(token, curr as usize);
                for i in 0..node.len as usize {
                    if !f(node.key_at(i), node.val_at(i)) {
                        return false;
                    }
                }
                let offset = node.link_offset as usize;
                curr = *self.links.get_unchecked(token, offset);
            }
        }
        true
    }
}

// Iterators
pub struct Iter<'a, 'brand, K, V> {
    list: &'a BrandedSkipList<'brand, K, V>,
    token: &'a GhostToken<'brand>,
    curr: u32,
    idx: usize,
}

impl<'a, 'brand, K, V> Iterator for Iter<'a, 'brand, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr == NONE {
            return None;
        }

        unsafe {
            let node = self.list.nodes.get_unchecked(self.token, self.curr as usize);
            if self.idx < node.len as usize {
                let k = node.key_at(self.idx);
                let v = node.val_at(self.idx);
                self.idx += 1;
                return Some((k, v));
            } else {
                // Move to next chunk
                let offset = node.link_offset as usize;
                self.curr = *self.list.links.get_unchecked(self.token, offset);
                self.idx = 0;
                return self.next();
            }
        }
    }
}

pub struct IterMut<'a, 'brand, K, V> {
    list: &'a BrandedSkipList<'brand, K, V>,
    token: &'a mut GhostToken<'brand>,
    curr: u32,
    idx: usize,
}

impl<'a, 'brand, K, V> Iterator for IterMut<'a, 'brand, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr == NONE {
            return None;
        }

        unsafe {
            // Need to re-borrow node to get mutable reference
            // This is safe because we only yield one element at a time
            // and `GhostToken` linearity is maintained by `&'a mut GhostToken` in struct.
            // But we need to use `get_unchecked_mut` which requires `&mut Token`.
            // We have it.

            // To avoid borrow checker issues with `self.token`, we use raw pointers or unsafe reborrow.
            // Standard pattern: split borrow.

            let node = self.list.nodes.get_unchecked_mut(self.token, self.curr as usize);

            if self.idx < node.len as usize {
                let k_ptr = node.key_at(self.idx) as *const K;
                let v_ptr = node.val_at_mut(self.idx) as *mut V;

                self.idx += 1;

                return Some((&*k_ptr, &mut *v_ptr));
            } else {
                // Move next
                // Read next link using shared access (safe)
                let offset = node.link_offset as usize;
                let next_curr = *self.list.links.get_unchecked(self.token, offset);

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
}
