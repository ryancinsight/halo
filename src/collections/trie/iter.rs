use super::map::BrandedRadixTrieMap;
use super::node::NodeSlot;
use crate::alloc::BrandedRc;
use crate::collections::BrandedVec;
use crate::token::traits::GhostBorrow;
use crate::GhostToken;
use std::vec::Vec;

/// Iterator over key-value pairs of `BrandedRadixTrieMap`.
/// Yields `(BrandedRc<BrandedVec<u8>>, &V)`.
pub struct Iter<'a, 'brand, K, V, Token = GhostToken<'brand>>
where
    Token: GhostBorrow<'brand>,
{
    map: &'a BrandedRadixTrieMap<'brand, K, V>,
    token: &'a Token,
    // Stack of (node_idx, child_pos_index)
    stack: Vec<(usize, usize)>,
    // Current constructed key
    key_buf: BrandedRc<'brand, BrandedVec<'brand, u8>>,
}

impl<'a, 'brand, K, V, Token> Iter<'a, 'brand, K, V, Token>
where
    Token: GhostBorrow<'brand>,
{
    /// Creates a new iterator over the map.
    pub fn new(map: &'a BrandedRadixTrieMap<'brand, K, V>, token: &'a Token) -> Self {
        let mut stack = Vec::new();
        let mut key_buf = BrandedVec::new();

        if let Some(root_idx) = map.root {
            if let Some(NodeSlot::Occupied(node)) = map.nodes.get(token, root_idx) {
                key_buf.extend(node.prefix.iter().copied());
                stack.push((root_idx, 0));
            }
        }

        Self {
            map,
            token,
            stack,
            key_buf: BrandedRc::new(key_buf),
        }
    }
}

impl<'a, 'brand, K, V, Token> Iterator for Iter<'a, 'brand, K, V, Token>
where
    Token: GhostBorrow<'brand>,
{
    type Item = (BrandedRc<'brand, BrandedVec<'brand, u8>>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.stack.is_empty() {
                return None;
            }

            let last_idx = self.stack.len() - 1;
            let (node_idx, action) = self.stack[last_idx];

            let slot = self.map.nodes.get(self.token, node_idx).expect("Corrupted");
            let NodeSlot::Occupied(node) = slot else {
                panic!("Iterating free slot");
            };

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

                let child_slot = self
                    .map
                    .nodes
                    .get(self.token, next_node_idx)
                    .expect("Corrupted");
                if let NodeSlot::Occupied(child_node) = child_slot {
                    let buf = self
                        .key_buf
                        .make_mut(|v| v.clone_with_token(self.token));
                    buf.extend(child_node.prefix.iter().copied());
                    self.stack.push((next_node_idx, 0));
                } else {
                    panic!("Child is free slot");
                }
                continue;
            }

            // Done with this node
            self.stack.pop();
            let popped_len = node.prefix.len();
            let buf = self
                .key_buf
                .make_mut(|v| v.clone_with_token(self.token));
            if buf.len() >= popped_len {
                let new_len = buf.len() - popped_len;
                buf.truncate(new_len);
            }
        }
    }
}
