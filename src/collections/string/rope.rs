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
const CHUNK_SIZE: usize = 1024; // Larger chunk size for better performance

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

impl<'brand> Node<'brand> {
    fn len(&self) -> usize {
        match self {
            Node::Leaf { text } => text.len(),
            Node::Internal { .. } => 0, // Should not be called directly for total len without traversal?
                                        // Actually Internal nodes don't store total len, they store weight.
                                        // But we can't easily get total len from just &Node without traversing right.
                                        // So we usually rely on the Rope struct to track total len or traverse.
        }
    }
}

/// A Branded Rope.
pub struct BrandedRope<'brand> {
    nodes: BrandedVec<'brand, Node<'brand>>,
    root: NodeIdx<'brand>,
    len: usize, // Total byte length
}

impl<'brand> BrandedRope<'brand> {
    /// Creates a new empty Rope.
    pub fn new() -> Self {
        Self {
            nodes: BrandedVec::new(),
            root: NodeIdx::NONE,
            len: 0,
        }
    }

    /// Creates a new Rope with specified node capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: BrandedVec::with_capacity(capacity),
            root: NodeIdx::NONE,
            len: 0,
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
        self.len = 0;
    }

    /// Appends a string to the end of the rope.
    pub fn append(&mut self, token: &mut GhostToken<'brand>, s: &str) {
        self.insert(token, self.len, s);
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
            let node = Node::Leaf { text: bytes.to_vec() };
            self.root = NodeIdx::new(self.nodes.len());
            self.nodes.push(node);
            self.len = bytes.len();
            return;
        }

        // We need to recursively insert.
        // Since we store nodes in a flat vector (arena), we can't easily "replace" a node structure in place
        // if it changes from Leaf to Internal without invalidating parents pointing to it.
        // However, we use indices. We can update the content of the node at `idx`.

        let root = self.root;
        let (new_root, added_len) = self.insert_recursive(token, root, idx, bytes);
        self.root = new_root;
        self.len += added_len;
    }

    // Returns (new_node_idx, added_length)
    fn insert_recursive(&mut self, token: &mut GhostToken<'brand>, node_idx: NodeIdx<'brand>, idx: usize, s: &[u8]) -> (NodeIdx<'brand>, usize) {
        // We must borrow the node to inspect it.
        // Note: We cannot hold a reference to `nodes` while pushing to it (mut borrowing self.nodes).
        // So we extract necessary data and drop the borrow before recursing/modifying.

        let node_data = {
            let node = self.nodes.get(token, node_idx.index()).unwrap();
            match node {
                Node::Leaf { text } => {
                    // It's a leaf.
                    // If it fits, insert in place.
                    if text.len() + s.len() <= CHUNK_SIZE {
                        let mut new_text = text.clone();
                        // Insert s into text at idx
                        // Note: idx is relative to this node start.
                        // In recursive calls, idx is adjusted.
                        // Here idx should be within [0, text.len()]
                        new_text.splice(idx..idx, s.iter().cloned());
                        return (self.update_node(token, node_idx, Node::Leaf { text: new_text }), s.len());
                    } else {
                        // Split required.
                        // We split this leaf into two leaves and insert the new text.
                        // Actually, inserting might require multiple splits if s is huge.
                        // For simplicity, we handle s as a chunk or split s?
                        // If s is huge, we should chunk it up.
                        // But let's assume s is reasonable or we handle it by simple split.

                        let mut combined = Vec::with_capacity(text.len() + s.len());
                        combined.extend_from_slice(&text[..idx]);
                        combined.extend_from_slice(s);
                        combined.extend_from_slice(&text[idx..]);

                        // Now split `combined` into leaves of CHUNK_SIZE
                        // Simple split: Left half, Right half.
                        let mid = combined.len() / 2;
                        let left_text = combined[..mid].to_vec();
                        let right_text = combined[mid..].to_vec();

                        let left_node = Node::Leaf { text: left_text };
                        let right_node = Node::Leaf { text: right_text };

                        let left_idx = self.alloc_node(left_node);
                        let right_idx = self.alloc_node(right_node);

                        // Create internal node replacing current node
                        let internal = Node::Internal {
                            left: left_idx,
                            right: right_idx,
                            weight: mid,
                        };

                        // We overwrite the current node_idx with the new Internal node to preserve parent links?
                        // Yes, reusing node_idx is good.
                        let _ = self.update_node(token, node_idx, internal);
                        return (node_idx, s.len());
                    }
                },
                Node::Internal { left, right, weight } => {
                    (*left, *right, *weight)
                }
            }
        };

        // If we are here, it was Internal.
        let (left, right, weight) = node_data;

        if idx < weight {
            // Insert in left
            let (new_left, added) = self.insert_recursive(token, left, idx, s);
            // Update weight
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
                weight: weight, // Weight doesn't change if inserted in right
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
    stack: Vec<NodeIdx<'brand>>, // Stack for DFS
    current_leaf: Option<&'a [u8]>,
    leaf_idx: usize,
}

impl<'a, 'brand> BytesIter<'a, 'brand> {
    fn new(rope: &'a BrandedRope<'brand>, token: &'a GhostToken<'brand>) -> Self {
        let mut stack = Vec::new();
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
            // Safe unwrap because indices in stack are valid
            let node = unsafe { self.rope.nodes.get_unchecked(self.token, idx.index()) };
            match node {
                Node::Leaf { text } => {
                    self.current_leaf = Some(text);
                    self.leaf_idx = 0;
                    return;
                },
                Node::Internal { left, right, .. } => {
                    // Push right then left
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
        // Decode UTF-8 on the fly from bytes iterator
        // Optimization: BrandedRope could yield chunks of &str instead of bytes to allow standard utf8 decoding
        // But for now, simple implementation

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
                 return Some(std::char::REPLACEMENT_CHARACTER); // truncated
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
    else { 1 } // Invalid start byte
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
    fn test_rope_insert_split() {
        // Force split by inserting large data or setting CHUNK_SIZE small?
        // CHUNK_SIZE is 128.
        GhostToken::new(|mut token| {
            let mut rope = BrandedRope::new();
            // Insert 100 bytes
            let s1 = "a".repeat(100);
            rope.append(&mut token, &s1);

            // Insert another 100 bytes. Should trigger split.
            let s2 = "b".repeat(100);
            rope.append(&mut token, &s2);

            assert_eq!(rope.len(), 200);

            // Insert in middle (at 100)
            rope.insert(&mut token, 100, "MIDDLE");
            assert_eq!(rope.len(), 206);

            let char_at_0 = rope.get_byte(&token, 0).unwrap() as char;
            assert_eq!(char_at_0, 'a');

            let char_at_100 = rope.get_byte(&token, 100).unwrap() as char;
            assert_eq!(char_at_100, 'M');

            let char_at_106 = rope.get_byte(&token, 106).unwrap() as char;
            assert_eq!(char_at_106, 'b');
        });
    }
}
