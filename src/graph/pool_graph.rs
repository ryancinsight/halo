//! `BrandedPoolGraph` — a dynamic graph where nodes are allocated in a shared pool.
//!
//! This implementation uses `BrandedPool` to store nodes, allowing O(1) node allocation and
//! deallocation (amortized). It maintains both outgoing and incoming edges to allow
//! efficient node and edge removal (O(degree)).
//!
//! # Performance
//! - `add_node`: O(1)
//! - `remove_node`: O(degree) (updates neighbors' adjacency lists)
//! - `add_edge`: O(1) (append to lists)
//! - `remove_edge`: O(degree) (scan adjacency lists)
//! - `neighbors`: O(1) to get iterator

use crate::alloc::pool::BrandedPool;
use crate::{GhostCell, GhostToken};
use std::marker::PhantomData;

/// A strongly-typed index for a node in a specific branded graph.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIdx<'brand>(usize, PhantomData<fn(&'brand ()) -> &'brand ()>);

impl<'brand> NodeIdx<'brand> {
    #[inline(always)]
    fn new(idx: usize) -> Self {
        Self(idx, PhantomData)
    }

    #[inline(always)]
    pub fn index(self) -> usize {
        self.0
    }
}

/// Internal node structure.
struct NodeData<V, E> {
    value: V,
    outgoing: Vec<(usize, E)>, // (target_idx, edge_data)
    incoming: Vec<usize>,      // source_idx
}

/// A dynamic graph backed by a branded pool.
pub struct BrandedPoolGraph<'brand, V, E> {
    pool: BrandedPool<'brand, NodeData<V, E>>,
}

impl<'brand, V, E> BrandedPoolGraph<'brand, V, E> {
    /// Creates a new empty graph.
    pub fn new() -> Self {
        Self {
            pool: BrandedPool::new(),
        }
    }

    /// Creates a graph with estimated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pool: BrandedPool::with_capacity(capacity),
        }
    }

    /// Adds a node to the graph.
    pub fn add_node(&self, token: &mut GhostToken<'brand>, value: V) -> NodeIdx<'brand> {
        let node = NodeData {
            value,
            outgoing: Vec::new(),
            incoming: Vec::new(),
        };
        let idx = self.pool.alloc(token, node);
        NodeIdx::new(idx)
    }

    /// Adds a directed edge from `source` to `target` with `weight`.
    ///
    /// # Panics
    /// Panics if source or target nodes do not exist.
    pub fn add_edge(
        &self,
        token: &mut GhostToken<'brand>,
        source: NodeIdx<'brand>,
        target: NodeIdx<'brand>,
        weight: E,
    ) where
        E: Clone,
    {
        let u = source.index();
        let v = target.index();

        let view = self.pool.view_mut(token);

        if u == v {
            unsafe {
                if u < view.storage.len() {
                    let word_idx = u >> 6;
                    let bit_idx = u & 63;
                    if (view.occupied[word_idx] & (1 << bit_idx)) != 0 {
                        let node = &mut view.storage[u].occupied;
                        node.outgoing.push((v, weight));
                        node.incoming.push(u);
                    } else {
                        panic!("Node invalid");
                    }
                } else {
                    panic!("Node invalid");
                }
            }
        } else {
            unsafe {
                assert!(u < view.storage.len());
                assert!(v < view.storage.len());

                let word_idx_u = u >> 6;
                let bit_idx_u = u & 63;
                let occupied_u = (view.occupied[word_idx_u] & (1 << bit_idx_u)) != 0;

                let word_idx_v = v >> 6;
                let bit_idx_v = v & 63;
                let occupied_v = (view.occupied[word_idx_v] & (1 << bit_idx_v)) != 0;

                if occupied_u && occupied_v {
                    let ptr = view.storage.as_mut_ptr();
                    let node_u = &mut (*ptr.add(u)).occupied;
                    let node_v = &mut (*ptr.add(v)).occupied;

                    node_u.outgoing.push((v, weight));
                    node_v.incoming.push(u);
                } else {
                    panic!("Node invalid");
                }
            }
        }
    }

    /// Removes a node and all incident edges.
    pub fn remove_node(
        &self,
        token: &mut GhostToken<'brand>,
        node_idx: NodeIdx<'brand>,
    ) -> Option<V> {
        let u = node_idx.index();

        if self.pool.get(token, u).is_none() {
            return None;
        }

        let node_data = unsafe { self.pool.take(token, u) };

        let view = self.pool.view_mut(token);
        let ptr = view.storage.as_mut_ptr();

        // Remove `u` from incoming neighbors' outgoing lists
        for &inc_idx in &node_data.incoming {
            if inc_idx == u {
                continue;
            }
            unsafe {
                let word_idx = inc_idx >> 6;
                let bit_idx = inc_idx & 63;
                if (view.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    let data = &mut (*ptr.add(inc_idx)).occupied;
                    if let Some(pos) = data.outgoing.iter().position(|(target, _)| *target == u) {
                        data.outgoing.swap_remove(pos);
                    }
                }
            }
        }

        // Remove `u` from outgoing neighbors' incoming lists
        for (out_idx, _) in &node_data.outgoing {
            if *out_idx == u {
                continue;
            }
            unsafe {
                let word_idx = *out_idx >> 6;
                let bit_idx = *out_idx & 63;
                if (view.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    let data = &mut (*ptr.add(*out_idx)).occupied;
                    if let Some(pos) = data.incoming.iter().position(|&source| source == u) {
                        data.incoming.swap_remove(pos);
                    }
                }
            }
        }

        Some(node_data.value)
    }

    /// Removes an edge.
    pub fn remove_edge(
        &self,
        token: &mut GhostToken<'brand>,
        source: NodeIdx<'brand>,
        target: NodeIdx<'brand>,
    ) -> Option<E> {
        let u = source.index();
        let v = target.index();

        let view = self.pool.view_mut(token);
        let ptr = view.storage.as_mut_ptr();

        let mut removed_data = None;

        unsafe {
            // Remove from source outgoing
            if u < view.storage.len() {
                let word_idx = u >> 6;
                let bit_idx = u & 63;
                if (view.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    let data_u = &mut (*ptr.add(u)).occupied;
                    if let Some(pos) = data_u.outgoing.iter().position(|(t, _)| *t == v) {
                        removed_data = Some(data_u.outgoing.swap_remove(pos).1);
                    }
                }
            }

            // Remove from target incoming
            if removed_data.is_some() && v < view.storage.len() {
                let word_idx = v >> 6;
                let bit_idx = v & 63;
                if (view.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    let data_v = &mut (*ptr.add(v)).occupied;
                    if let Some(pos) = data_v.incoming.iter().position(|&s| s == u) {
                        data_v.incoming.swap_remove(pos);
                    }
                }
            }
        }

        removed_data
    }

    /// Get reference to node value.
    pub fn get<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node: NodeIdx<'brand>,
    ) -> Option<&'a V> {
        self.pool.get(token, node.index()).map(|n| &n.value)
    }

    /// Get mutable reference to node value.
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        node: NodeIdx<'brand>,
    ) -> Option<&'a mut V> {
        self.pool.get_mut(token, node.index()).map(|n| &mut n.value)
    }

    /// Get neighbors (outgoing edges).
    pub fn neighbors<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node: NodeIdx<'brand>,
    ) -> impl Iterator<Item = (NodeIdx<'brand>, &'a E)> + 'a {
        self.pool
            .get(token, node.index())
            .map(|n| n.outgoing.iter().map(|(idx, w)| (NodeIdx::new(*idx), w)))
            .into_iter()
            .flatten()
    }

    /// Get neighbors (outgoing edges) assuming the node exists.
    ///
    /// # Safety
    /// Caller must ensure `node` is valid (allocated and not removed).
    pub unsafe fn neighbors_unchecked<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node: NodeIdx<'brand>,
    ) -> impl Iterator<Item = (NodeIdx<'brand>, &'a E)> + 'a {
        let n = self.pool.get_unchecked(token, node.index());
        n.outgoing.iter().map(|(idx, w)| (NodeIdx::new(*idx), w))
    }

    /// Get incoming neighbors.
    pub fn incoming_neighbors<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node: NodeIdx<'brand>,
    ) -> impl Iterator<Item = NodeIdx<'brand>> + 'a {
        self.pool
            .get(token, node.index())
            .map(|n| n.incoming.iter().map(|idx| NodeIdx::new(*idx)))
            .into_iter()
            .flatten()
    }

    /// Returns number of nodes (active).
    pub fn node_count(&self, token: &GhostToken<'brand>) -> usize {
        self.pool.len(token)
    }

    /// Iterates over all active nodes.
    pub fn iter_nodes<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = (NodeIdx<'brand>, &'a V)> + 'a {
        let view = self.pool.view(token);
        view.storage
            .iter()
            .enumerate()
            .filter_map(move |(i, slot)| {
                let word_idx = i >> 6;
                let bit_idx = i & 63;
                if (view.occupied[word_idx] & (1 << bit_idx)) != 0 {
                    unsafe {
                        let data = &slot.occupied;
                        Some((NodeIdx::new(i), &data.value))
                    }
                } else {
                    None
                }
            })
    }
}

impl<'brand, V, E> Default for BrandedPoolGraph<'brand, V, E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_pool_graph_basic() {
        GhostToken::new(|mut token| {
            let graph = BrandedPoolGraph::new();

            let n0 = graph.add_node(&mut token, "A");
            let n1 = graph.add_node(&mut token, "B");

            graph.add_edge(&mut token, n0, n1, 10);

            assert_eq!(graph.node_count(&token), 2);
            assert_eq!(*graph.get(&token, n0).unwrap(), "A");

            let neighbors: Vec<_> = graph.neighbors(&token, n0).collect();
            assert_eq!(neighbors.len(), 1);
            assert_eq!(neighbors[0].0, n1);
            assert_eq!(*neighbors[0].1, 10);

            // Remove edge
            let weight = graph.remove_edge(&mut token, n0, n1);
            assert_eq!(weight, Some(10));
            assert_eq!(graph.neighbors(&token, n0).count(), 0);
        });
    }

    #[test]
    fn test_pool_graph_remove_node() {
        GhostToken::new(|mut token| {
            let graph = BrandedPoolGraph::new();

            let n0 = graph.add_node(&mut token, 0);
            let n1 = graph.add_node(&mut token, 1);
            let n2 = graph.add_node(&mut token, 2);

            graph.add_edge(&mut token, n0, n1, ());
            graph.add_edge(&mut token, n1, n2, ());
            graph.add_edge(&mut token, n2, n0, ());

            // Remove middle node
            let val = graph.remove_node(&mut token, n1);
            assert_eq!(val, Some(1));

            assert_eq!(graph.node_count(&token), 2);

            // Check edges
            // n0->n1 should be gone.
            assert_eq!(graph.neighbors(&token, n0).count(), 0);

            // n1->n2 should be gone (n1 gone).

            // n2->n0 should remain.
            let neighbors: Vec<_> = graph.neighbors(&token, n2).collect();
            assert_eq!(neighbors.len(), 1);
            assert_eq!(neighbors[0].0, n0);
        });
    }
}
