//! `BrandedRope` — a mutable rope data structure for efficient string editing.
//!
//! A Rope is a tree-based data structure where leaf nodes contain string chunks.
//! It supports efficient insertion and deletion at arbitrary positions, outperforming
//! standard strings for large text manipulation.
//!
//! This implementation uses a `BrandedVec` arena to store nodes, leveraging `GhostToken`
//! for safe, zero-cost access control.

use crate::{GhostToken, GhostCell};
use crate::collections::{BrandedVec, BrandedCollection};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::cmp;

/// Max size for a leaf node before it splits.
const CHUNK_SIZE: usize = 2048;

/// A branded index into the rope node arena.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIdx<'brand>(u32, PhantomData<fn(&'brand ()) -> &'brand ()>);

impl<'brand> NodeIdx<'brand> {
    const NONE: Self = Self(u32::MAX, PhantomData);

    #[inline(always)]
    fn new(idx: usize) -> Self {
        Self(idx as u32, PhantomData)
    }

    #[inline(always)]
    fn index(self) -> usize {
        self.0 as usize
    }

    #[inline(always)]
    fn is_some(self) -> bool {
        self.0 != u32::MAX
    }
}

/// A node in the Rope tree.
pub enum Node<'brand> {
    Leaf {
        text: Vec<u8>,
    },
    Internal {
        left: NodeIdx<'brand>,
        right: NodeIdx<'brand>,
        weight: usize, // Byte length of the left subtree
    },
}

/// A Branded Rope.
pub struct BrandedRope<'brand> {
    nodes: BrandedVec<'brand, Node<'brand>>,
    root: NodeIdx<'brand>,
    len: usize, // Total byte length
    last_leaf: NodeIdx<'brand>, // Cache for the right-most leaf for fast appends
}

impl<'brand> BrandedRope<'brand> {
    /// Creates a new empty Rope.
    pub fn new() -> Self {
        Self {
            nodes: BrandedVec::new(),
            root: NodeIdx::NONE,
            len: 0,
            last_leaf: NodeIdx::NONE,
        }
    }

    /// Creates a new Rope with specified node capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: BrandedVec::with_capacity(capacity),
            root: NodeIdx::NONE,
            len: 0,
            last_leaf: NodeIdx::NONE,
        }
    }

    /// Reserves capacity for at least `additional` more nodes.
    pub fn reserve_nodes(&mut self, additional: usize) {
        self.nodes.reserve(additional);
    }

    /// Creates a Rope from a string slice.
    pub fn from_str(token: &mut GhostToken<'brand>, s: &str) -> Self {
        let mut rope = Self::new();
        rope.append(token, s);
        rope
    }

    /// Returns the total byte length of the rope.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Clears the rope.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.root = NodeIdx::NONE;
        self.last_leaf = NodeIdx::NONE;
        self.len = 0;
    }

    /// Appends a string to the end of the rope.
    pub fn append(&mut self, token: &mut GhostToken<'brand>, s: &str) {
        if s.is_empty() { return; }

        // Fast path: try to append to last_leaf
        if self.last_leaf.is_some() {
            // We need to access last_leaf.
            // Warning: `last_leaf` might have become Internal if it was split and we didn't update cache?
            // `insert_recursive` handles cache update if we split.
            // But we need to check if it's a Leaf.

            // Unsafe check to avoid full lookup? No, we have direct index.
            // Note: We cannot hold borrow of self.nodes.

            let can_append = {
                 let node = self.nodes.get(token, self.last_leaf.index()).unwrap();
                 if let Node::Leaf { text } = node {
                     text.len() + s.len() <= CHUNK_SIZE
                 } else {
                     false
                 }
            };

            if can_append {
                let node = self.nodes.get_mut(token, self.last_leaf.index()).unwrap();
                if let Node::Leaf { text } = node {
                    text.extend_from_slice(s.as_bytes());
                    self.len += s.len();
                    return;
                }
            }
        }

        // If s is large, build a balanced tree and append it?
        // Or if tree is empty, build balanced.
        if self.root == NodeIdx::NONE {
            self.build_balanced(token, s.as_bytes());
            return;
        }

        // Fallback to insert.
        self.insert(token, self.len, s);
    }

    fn build_balanced(&mut self, token: &mut GhostToken<'brand>, s: &[u8]) {
        if s.is_empty() { return; }

        // Chunkify
        let mut chunks = Vec::new();
        for chunk in s.chunks(CHUNK_SIZE) {
            let node = Node::Leaf { text: chunk.to_vec() };
            chunks.push(self.alloc_node(node));
        }

        // Combine chunks into tree
        let root = self.build_tree_recursive(token, &chunks);
        self.root = root;
        self.len = s.len();

        // Find last leaf
        let mut curr = root;
        loop {
             let node = self.nodes.get(token, curr.index()).unwrap();
             match node {
                 Node::Internal { right, .. } => curr = *right,
                 Node::Leaf { .. } => {
                     self.last_leaf = curr;
                     break;
                 }
             }
        }
    }

    fn build_tree_recursive(&mut self, token: &mut GhostToken<'brand>, indices: &[NodeIdx<'brand>]) -> NodeIdx<'brand> {
        if indices.len() == 1 {
            return indices[0];
        }

        let mid = indices.len() / 2;
        let left = self.build_tree_recursive(token, &indices[..mid]);
        let right = self.build_tree_recursive(token, &indices[mid..]);

        // Calculate weight (left len)
        let weight = self.get_node_len(token, left);

        let internal = Node::Internal { left, right, weight };
        self.alloc_node(internal)
    }

    fn get_node_len(&self, token: &GhostToken<'brand>, idx: NodeIdx<'brand>) -> usize {
        let node = self.nodes.get(token, idx.index()).unwrap();
        match node {
            Node::Leaf { text } => text.len(),
            Node::Internal { left, right, weight } => {
                // Total len = weight + right len
                weight + self.get_node_len(token, *right)
            }
        }
    }

    /// Inserts a string at the given byte index.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, idx: usize, s: &str) {
        if s.is_empty() {
            return;
        }
        if idx > self.len {
            panic!("Index out of bounds");
        }

        let bytes = s.as_bytes();

        // If empty, just create a leaf
        if self.root == NodeIdx::NONE {
             self.build_balanced(token, bytes);
             return;
        }

        let root = self.root;
        let (new_root, added_len) = self.insert_recursive(token, root, idx, bytes);
        self.root = new_root;
        self.len += added_len;
    }

    // Returns (new_node_idx, added_length)
    fn insert_recursive(&mut self, token: &mut GhostToken<'brand>, node_idx: NodeIdx<'brand>, idx: usize, s: &[u8]) -> (NodeIdx<'brand>, usize) {
        let node_data = {
            let node = self.nodes.get(token, node_idx.index()).unwrap();
            match node {
                Node::Leaf { text } => {
                    // It's a leaf.
                    if text.len() + s.len() <= CHUNK_SIZE {
                        let mut new_text = text.clone();
                        new_text.splice(idx..idx, s.iter().cloned());
                        return (self.update_node(token, node_idx, Node::Leaf { text: new_text }), s.len());
                    } else {
                        // Optimized Split
                        // Reuse `text` vector for one part if possible?
                        // `text` is cloned anyway in current `Node` structure (we don't have ownership of `text` here, only reference).
                        // To take ownership, we would need to replace the Node in `nodes` with dummy first?
                        // `self.nodes.get_mut` gives `&mut Node`.
                        // We can `mem::take` the text if we want.

                        // Let's grab the text out.
                        // We need `&mut token`. We have it.
                        let text = {
                             let node_mut = self.nodes.get_mut(token, node_idx.index()).unwrap();
                             if let Node::Leaf { ref mut text } = node_mut {
                                 std::mem::take(text)
                             } else { unreachable!() }
                        };

                        // Construct new leaves
                        // Layout: text[..idx] + s + text[idx..]
                        // We want to split this into chunks.
                        // For simplicity, just split in half? Or fill left node?
                        // Filling left node (Chunk Size) is better for density.

                        let total_len = text.len() + s.len();
                        let mut combined = Vec::with_capacity(total_len);
                        combined.extend_from_slice(&text[..idx]);
                        combined.extend_from_slice(s);
                        combined.extend_from_slice(&text[idx..]);

                        let mid = total_len / 2;
                        let left_text = combined[..mid].to_vec();
                        let right_text = combined[mid..].to_vec();

                        let left_len = left_text.len();

                        let left_node = Node::Leaf { text: left_text };
                        let right_node = Node::Leaf { text: right_text };

                        let left_idx = self.alloc_node(left_node);
                        let right_idx = self.alloc_node(right_node);

                        let internal = Node::Internal {
                            left: left_idx,
                            right: right_idx,
                            weight: left_len,
                        };

                        // If we split the `last_leaf`, we must update it.
                        // Since we just split `node_idx`, if `last_leaf == node_idx`, then `last_leaf` is now invalid (pointing to Internal).
                        // The new last leaf (of this subtree) is `right_idx`.
                        if self.last_leaf == node_idx {
                            self.last_leaf = right_idx;
                        }

                        // Overwrite node_idx
                        let _ = self.update_node(token, node_idx, internal);
                        return (node_idx, s.len());
                    }
                },
                Node::Internal { left, right, weight } => {
                    (*left, *right, *weight)
                }
            }
        };

        let (left, right, weight) = node_data;

        if idx < weight {
            // Insert in left
            let (new_left, added) = self.insert_recursive(token, left, idx, s);
             let new_internal = Node::Internal {
                left: new_left,
                right,
                weight: weight + added,
            };
            (self.update_node(token, node_idx, new_internal), added)
        } else {
            // Insert in right
            let (new_right, added) = self.insert_recursive(token, right, idx - weight, s);
             let new_internal = Node::Internal {
                left,
                right: new_right,
                weight: weight,
            };
            (self.update_node(token, node_idx, new_internal), added)
        }
    }

    fn alloc_node(&mut self, node: Node<'brand>) -> NodeIdx<'brand> {
        let idx = self.nodes.len();
        self.nodes.push(node);
        NodeIdx::new(idx)
    }

    fn update_node(&mut self, token: &mut GhostToken<'brand>, idx: NodeIdx<'brand>, node: Node<'brand>) -> NodeIdx<'brand> {
        *self.nodes.get_mut(token, idx.index()).unwrap() = node;
        idx
    }

    /// Gets the byte at index.
    pub fn get_byte(&self, token: &GhostToken<'brand>, index: usize) -> Option<u8> {
        if index >= self.len {
            return None;
        }
        self.get_byte_recursive(token, self.root, index)
    }

    fn get_byte_recursive(&self, token: &GhostToken<'brand>, node_idx: NodeIdx<'brand>, index: usize) -> Option<u8> {
        let node = self.nodes.get(token, node_idx.index())?;
        match node {
            Node::Leaf { text } => {
                if index < text.len() {
                    Some(text[index])
                } else {
                    None
                }
            },
            Node::Internal { left, right, weight } => {
                if index < *weight {
                    self.get_byte_recursive(token, *left, index)
                } else {
                    self.get_byte_recursive(token, *right, index - *weight)
                }
            }
        }
    }

    /// Iterator over bytes.
    pub fn bytes<'a>(&'a self, token: &'a GhostToken<'brand>) -> BytesIter<'a, 'brand> {
        BytesIter::new(self, token)
    }

    /// Iterator over chars.
    pub fn chars<'a>(&'a self, token: &'a GhostToken<'brand>) -> CharsIter<'a, 'brand> {
        CharsIter::new(self, token)
    }
}

impl<'brand> BrandedCollection<'brand> for BrandedRope<'brand> {
    fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn len(&self) -> usize {
        self.len
    }
}

// Iterators

pub struct BytesIter<'a, 'brand> {
    rope: &'a BrandedRope<'brand>,
    token: &'a GhostToken<'brand>,
    stack: Vec<NodeIdx<'brand>>,
    current_leaf: Option<&'a [u8]>,
    leaf_idx: usize,
}

impl<'a, 'brand> BytesIter<'a, 'brand> {
    fn new(rope: &'a BrandedRope<'brand>, token: &'a GhostToken<'brand>) -> Self {
        let mut stack = Vec::with_capacity(32); // Pre-allocate stack
        if rope.root.is_some() {
            stack.push(rope.root);
        }
        let mut iter = Self {
            rope,
            token,
            stack,
            current_leaf: None,
            leaf_idx: 0,
        };
        iter.next_leaf();
        iter
    }

    fn next_leaf(&mut self) {
        while let Some(idx) = self.stack.pop() {
            let node = unsafe { self.rope.nodes.get_unchecked(self.token, idx.index()) };
            match node {
                Node::Leaf { text } => {
                    self.current_leaf = Some(text);
                    self.leaf_idx = 0;
                    return;
                },
                Node::Internal { left, right, .. } => {
                    self.stack.push(*right);
                    self.stack.push(*left);
                }
            }
        }
        self.current_leaf = None;
    }
}

impl<'a, 'brand> Iterator for BytesIter<'a, 'brand> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(leaf) = self.current_leaf {
            if self.leaf_idx < leaf.len() {
                let byte = leaf[self.leaf_idx];
                self.leaf_idx += 1;
                return Some(byte);
            } else {
                self.next_leaf();
                return self.next();
            }
        }
        None
    }
}

pub struct CharsIter<'a, 'brand> {
    bytes: BytesIter<'a, 'brand>,
}

impl<'a, 'brand> CharsIter<'a, 'brand> {
    fn new(rope: &'a BrandedRope<'brand>, token: &'a GhostToken<'brand>) -> Self {
        Self {
            bytes: BytesIter::new(rope, token),
        }
    }
}

impl<'a, 'brand> Iterator for CharsIter<'a, 'brand> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        let first = self.bytes.next()?;
        let width = utf8_width(first);
        if width == 1 {
            return Some(first as char);
        }

        let mut buf = [0u8; 4];
        buf[0] = first;

        for i in 1..width {
             if let Some(b) = self.bytes.next() {
                 buf[i] = b;
             } else {
                 return Some(std::char::REPLACEMENT_CHARACTER);
             }
        }

        std::str::from_utf8(&buf[..width])
            .ok()
            .and_then(|s| s.chars().next())
            .or(Some(std::char::REPLACEMENT_CHARACTER))
    }
}

fn utf8_width(b: u8) -> usize {
    if b & 0b10000000 == 0 { 1 }
    else if b & 0b11100000 == 0b11000000 { 2 }
    else if b & 0b11110000 == 0b11100000 { 3 }
    else if b & 0b11111000 == 0b11110000 { 4 }
    else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_rope_basic() {
        GhostToken::new(|mut token| {
            let mut rope = BrandedRope::new();
            assert!(rope.is_empty());

            rope.append(&mut token, "Hello");
            assert_eq!(rope.len(), 5);

            rope.append(&mut token, " World");
            assert_eq!(rope.len(), 11);

            let s: String = rope.chars(&token).collect();
            assert_eq!(s, "Hello World");
        });
    }

    #[test]
    fn test_rope_insert_middle() {
        GhostToken::new(|mut token| {
            let mut rope = BrandedRope::from_str(&mut token, "HelloWorld");
            rope.insert(&mut token, 5, " ");

            let s: String = rope.chars(&token).collect();
            assert_eq!(s, "Hello World");
        });
    }

    #[test]
    fn test_rope_balanced_build() {
         GhostToken::new(|mut token| {
             // Create string larger than CHUNK_SIZE * 2
             let s = "a".repeat(CHUNK_SIZE * 3);
             let rope = BrandedRope::from_str(&mut token, &s);

             assert_eq!(rope.len(), CHUNK_SIZE * 3);

             // Root should be Internal if balanced
             let node = rope.nodes.get(&token, rope.root.index()).unwrap();
             assert!(matches!(node, Node::Internal { .. }));
         });
    }

    #[test]
    fn test_rope_fast_append() {
         GhostToken::new(|mut token| {
            let mut rope = BrandedRope::new();
            rope.append(&mut token, "start");

            let last_leaf_idx = rope.last_leaf;
            assert!(last_leaf_idx.is_some());

            rope.append(&mut token, "end");

            // Should be same leaf if it fits
            assert_eq!(rope.last_leaf, last_leaf_idx);

            let s: String = rope.chars(&token).collect();
            assert_eq!(s, "startend");
         });
    }
}
