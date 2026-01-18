use std::vec::Vec;
use crate::GhostToken;
use super::map::BrandedRadixTrieMap;
use super::node::NodeSlot;

/// Iterator over key-value pairs of `BrandedRadixTrieMap`.
/// Yields `(Vec<u8>, &V)`.
pub struct Iter<'a, 'brand, K, V> {
    map: &'a BrandedRadixTrieMap<'brand, K, V>,
    token: &'a GhostToken<'brand>,
    // Stack of (node_idx, child_pos_index)
    stack: Vec<(usize, usize)>,
    // Current constructed key
    key_buf: Vec<u8>,
}

impl<'a, 'brand, K, V> Iter<'a, 'brand, K, V> {
    pub fn new(map: &'a BrandedRadixTrieMap<'brand, K, V>, token: &'a GhostToken<'brand>) -> Self {
        let mut stack = Vec::new();
        let mut key_buf = Vec::new();

        if let Some(root_idx) = map.root {
             if let Some(slot) = map.nodes.get(token, root_idx) {
                 if let NodeSlot::Occupied(node) = slot {
                     key_buf.extend_from_slice(&node.prefix);
                     stack.push((root_idx, 0));
                 }
             }
        }

        Self {
            map,
            token,
            stack,
            key_buf,
        }
    }
}

impl<'a, 'brand, K, V> Iterator for Iter<'a, 'brand, K, V> {
    type Item = (Vec<u8>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.stack.is_empty() {
                return None;
            }

            let last_idx = self.stack.len() - 1;
            let (node_idx, action) = self.stack[last_idx];

            let slot = self.map.nodes.get(self.token, node_idx).expect("Corrupted");
            let node = if let NodeSlot::Occupied(n) = slot { n } else { panic!("Iterating free slot") };

            if action == 0 {
                // Try to yield value
                self.stack[last_idx].1 += 1;

                if let Some(val) = &node.value {
                    return Some((self.key_buf.clone(), val));
                }
                continue;
            }

            let child_vec_idx = action - 1;
            if child_vec_idx < node.children.len() {
                // Descend to child
                let (_, next_node_idx) = node.children[child_vec_idx];

                // Advance parent so next time we visit next child
                self.stack[last_idx].1 += 1;

                let child_slot = self.map.nodes.get(self.token, next_node_idx).expect("Corrupted");
                if let NodeSlot::Occupied(child_node) = child_slot {
                    self.key_buf.extend_from_slice(&child_node.prefix);
                    self.stack.push((next_node_idx, 0));
                } else {
                    panic!("Child is free slot");
                }
                continue;
            } else {
                // Done with this node
                self.stack.pop();
                let popped_len = node.prefix.len();
                if self.key_buf.len() >= popped_len {
                    let new_len = self.key_buf.len() - popped_len;
                    self.key_buf.truncate(new_len);
                }
            }
        }
    }
}
