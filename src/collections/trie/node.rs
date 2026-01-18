use std::vec::Vec;
use std::boxed::Box;

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
    pub prefix: Box<[u8]>,
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
            prefix: Box::new([]),
            value: None,
            children: Vec::new(),
        }
    }

    /// Creates a new node with a value.
    pub fn new_with_value(value: V) -> Self {
        Self {
            prefix: Box::new([]),
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
        self.children.binary_search_by_key(&byte, |&(b, _)| b)
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
