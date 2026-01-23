use core::ops::Deref;
use std::boxed::Box;
use std::vec::Vec;

/// Max size for inline prefix. 15 bytes leaves 1 byte for length in a 16-byte alignment.
const INLINE_PREFIX_CAP: usize = 15;

/// Optimized prefix storage.
///
/// Stores short prefixes inline to avoid heap allocation and indirection.
/// - Inline: up to 15 bytes.
/// - Heap: arbitrarily long, stored in a `Box<[u8]>`.
#[derive(Debug, Clone)]
pub enum NodePrefix {
    Inline {
        len: u8,
        data: [u8; INLINE_PREFIX_CAP],
    },
    Heap(Box<[u8]>),
}

impl NodePrefix {
    /// Creates a new prefix from a slice.
    pub fn new(slice: &[u8]) -> Self {
        if slice.len() <= INLINE_PREFIX_CAP {
            let mut data = [0u8; INLINE_PREFIX_CAP];
            data[..slice.len()].copy_from_slice(slice);
            NodePrefix::Inline {
                len: slice.len() as u8,
                data,
            }
        } else {
            NodePrefix::Heap(Box::from(slice))
        }
    }

    /// Returns the prefix as a slice.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            NodePrefix::Inline { len, data } => &data[..*len as usize],
            NodePrefix::Heap(b) => b,
        }
    }

    /// Returns the length of the prefix.
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            NodePrefix::Inline { len, .. } => *len as usize,
            NodePrefix::Heap(b) => b.len(),
        }
    }
}

impl Deref for NodePrefix {
    type Target = [u8];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Default for NodePrefix {
    fn default() -> Self {
        NodePrefix::new(&[])
    }
}

impl AsRef<[u8]> for NodePrefix {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

/// A slot in the Trie arena.
/// Can be either an occupied node or a pointer to the next free slot.
#[derive(Debug, Clone)]
pub enum NodeSlot<V> {
    Occupied(Node<V>),
    Free(usize),
}

/// A node in the Radix Trie.
///
/// Each node contains:
/// - A prefix of keys that leads to this node (edge label from parent).
/// - An optional value (if this node represents a key).
/// - A list of children (edges to other nodes), sorted by the first byte of the edge label.
///
/// We use `usize` for links to other nodes within the arena (`BrandedVec`).
#[derive(Debug, Clone)]
pub struct Node<V> {
    /// The common prefix for this node relative to its parent.
    pub prefix: NodePrefix,
    /// The value stored at this node, if any.
    pub value: Option<V>,
    /// Children nodes, sorted by the first byte of the edge.
    /// Maps `first_byte` -> `node_index`.
    pub children: Vec<(u8, usize)>,
}

impl<V> Node<V> {
    /// Creates a new empty node.
    pub fn new() -> Self {
        Self {
            prefix: NodePrefix::new(&[]),
            value: None,
            children: Vec::new(),
        }
    }

    /// Creates a new node with a value.
    pub fn new_with_value(value: V) -> Self {
        Self {
            prefix: NodePrefix::new(&[]),
            value: Some(value),
            children: Vec::new(),
        }
    }

    /// Adds a child to the node.
    /// Maintains the sorted order of children.
    pub fn add_child(&mut self, byte: u8, child_idx: usize) {
        match self.children.binary_search_by_key(&byte, |&(b, _)| b) {
            Ok(pos) => self.children[pos] = (byte, child_idx), // Should not happen in normal insertion if checked
            Err(pos) => self.children.insert(pos, (byte, child_idx)),
        }
    }

    /// Finds the child index for a given byte.
    pub fn get_child(&self, byte: u8) -> Option<usize> {
        self.children
            .binary_search_by_key(&byte, |&(b, _)| b)
            .ok()
            .map(|pos| self.children[pos].1)
    }

    /// Removes a child by byte.
    pub fn remove_child(&mut self, byte: u8) {
        if let Ok(pos) = self.children.binary_search_by_key(&byte, |&(b, _)| b) {
            self.children.remove(pos);
        }
    }
}
