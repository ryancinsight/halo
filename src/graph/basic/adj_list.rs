//! Intrusive Adjacency List Graph
//!
//! A graph implementation where nodes are allocated individually (via `StaticRc`)
//! and edges are stored in a branded pool (Tripod-style linked lists).
//!
//! This design allows nodes to be managed with explicit ownership handles (`StaticRc`)
//! held by the user, ensuring that nodes cannot be used after removal from the graph,
//! while edges are compactly stored in a memory pool for cache efficiency.

use crate::alloc::pool::PoolSlot;
use crate::alloc::{BrandedPool, StaticRc};
use crate::cell::GhostCell;
use crate::GhostToken;
use std::marker::PhantomData;
use std::ptr::NonNull;

/// Internal node data structure.
///
/// This is wrapped in a `GhostCell` and managed by `StaticRc`.
pub struct NodeData<V> {
    /// The user-provided value.
    pub value: V,
    /// Head of the outgoing edge list (index into edge pool).
    pub(crate) head_outgoing: Option<usize>,
    /// Head of the incoming edge list (index into edge pool).
    pub(crate) head_incoming: Option<usize>,
    /// Index of the `StaticRc` handle in the graph's node pool.
    pub(crate) pool_idx: usize,
}

/// Internal edge data structure stored in the pool.
pub(crate) struct EdgeData<'brand, E, V> {
    pub weight: E,
    pub target: NonNull<GhostCell<'brand, NodeData<V>>>,
    pub source: NonNull<GhostCell<'brand, NodeData<V>>>,
    pub next_outgoing: Option<usize>,
    pub next_incoming: Option<usize>,
    pub _marker: PhantomData<&'brand ()>,
}

/// A handle to a graph node, representing 50% ownership.
///
/// The user holds this handle to keep the node alive and access its data.
/// To remove the node, this handle must be returned to the graph.
pub type NodeHandle<'brand, V> = StaticRc<'brand, GhostCell<'brand, NodeData<V>>, 1, 2>;

/// An intrusive adjacency list graph.
pub struct AdjListGraph<'brand, V, E> {
    /// Pool containing the graph's share of the node handles.
    nodes: BrandedPool<'brand, NodeHandle<'brand, V>>,
    /// Pool containing the edges.
    edges: BrandedPool<'brand, EdgeData<'brand, E, V>>,
}

impl<'brand, V, E> AdjListGraph<'brand, V, E> {
    /// Creates a new empty graph.
    pub fn new() -> Self {
        Self {
            nodes: BrandedPool::new(),
            edges: BrandedPool::new(),
        }
    }

    /// Adds a node to the graph and returns a handle to it.
    ///
    /// The returned `NodeHandle` represents partial ownership of the node.
    /// The node remains in the graph until `remove_node` is called with this handle.
    pub fn add_node(&self, token: &mut GhostToken<'brand>, value: V) -> NodeHandle<'brand, V> {
        // Create the node data, initially with invalid pool_idx (will set below).
        let node_data = NodeData {
            value,
            head_outgoing: None,
            head_incoming: None,
            pool_idx: usize::MAX,
        };

        // Create the StaticRc with N=D=2.
        // We use StaticRc::new which creates N=D.
        // But we want generic N=D=2.
        // StaticRc::new infers N, D from return type.
        // We need `StaticRc<..., 2, 2>`.
        let full_rc: StaticRc<'brand, _, 2, 2> = StaticRc::new(GhostCell::new(node_data));

        // Split into two halves (1/2).
        let (h1, h2) = full_rc.split::<1, 1>();

        // Store one half in the graph's node pool.
        let idx = self.nodes.alloc(token, h1);

        // Update the pool_idx in the node data.
        // We can access it via h2 (which we hold).
        h2.borrow_mut(token).pool_idx = idx;

        h2
    }

    /// Removes a node from the graph.
    ///
    /// Requires the user to surrender their `NodeHandle`.
    /// Returns the value stored in the node.
    pub fn remove_node(
        &self,
        token: &mut GhostToken<'brand>,
        handle: NodeHandle<'brand, V>,
    ) -> V {
        // 1. Get the pool index from the handle.
        let pool_idx = handle.borrow(token).pool_idx;

        // 2. Retrieve the graph's share of the handle.
        // Since we are inside remove_node, we assume validity.
        // If the handle is from this graph, pool_idx should be valid and contain the other half.
        // SAFETY: We rely on the invariant that pool_idx points to the matching handle.
        let other_half = unsafe { self.nodes.take(token, pool_idx) };

        // 3. Join the handles to regain full ownership.
        // This panics if pointers don't match (i.e. handle is from wrong graph or corrupted).
        let full_rc = handle.join(other_half);

        // 4. Clean up edges.
        // We need to remove all edges connected to this node.
        // We can use the head_outgoing/head_incoming pointers.

        let node_ptr = NonNull::from(full_rc.get());

        // Remove outgoing edges
        let mut curr = full_rc.borrow(token).head_outgoing;
        while let Some(edge_idx) = curr {
            // Unlink from target's incoming list
            // We need to access edge data.
            // Be careful about aliasing: we have &mut token, so we can access everything.

            // We need to read edge data, find target, remove from target's incoming.
            // And free edge slot.

            // Note: We are iterating a list we are destroying.
            // Read next before destroying.
            let edge_data = self.edges.get(token, edge_idx).expect("Corrupt edge list");
            let next_edge = edge_data.next_outgoing;
            let target_ptr = edge_data.target;

            // Remove from target's incoming list.
            unsafe {
                self.unlink_incoming(token, target_ptr, edge_idx);
            }

            // Free the edge.
            unsafe { self.edges.take(token, edge_idx) };

            curr = next_edge;
        }

        // Remove incoming edges
        let mut curr = full_rc.borrow(token).head_incoming;
        while let Some(edge_idx) = curr {
            // Unlink from source's outgoing list
            let edge_data = self.edges.get(token, edge_idx).expect("Corrupt edge list");
            let next_edge = edge_data.next_incoming;
            let source_ptr = edge_data.source;

            unsafe {
                self.unlink_outgoing(token, source_ptr, edge_idx);
            }

            unsafe { self.edges.take(token, edge_idx) };

            curr = next_edge;
        }

        // 5. Drop full_rc, which deallocates the NodeData.
        // We extract the value first?
        // StaticRc::into_box -> Box -> into_inner?
        // StaticRc owns GhostCell.
        // GhostCell owns T.
        // We can consume StaticRc.
        // But StaticRc::into_box gives Box<GhostCell<...>>.
        // Box::into_inner gives GhostCell.
        // GhostCell::into_inner gives T.

        let cell = *full_rc.into_box(); // Box<GhostCell> deref to GhostCell? No, Box<GhostCell>
        cell.into_inner().value
    }

    /// Adds a directed edge between two nodes.
    pub fn add_edge(
        &self,
        token: &mut GhostToken<'brand>,
        source: &NodeHandle<'brand, V>,
        target: &NodeHandle<'brand, V>,
        weight: E,
    ) {
        let source_ptr = NonNull::from(source.get());
        let target_ptr = NonNull::from(target.get());

        // Allocate edge
        let edge = EdgeData {
            weight,
            source: source_ptr,
            target: target_ptr,
            next_outgoing: source.borrow(token).head_outgoing,
            next_incoming: target.borrow(token).head_incoming,
            _marker: PhantomData,
        };

        let edge_idx = self.edges.alloc(token, edge);

        // Update heads
        source.borrow_mut(token).head_outgoing = Some(edge_idx);
        target.borrow_mut(token).head_incoming = Some(edge_idx);
    }

    // Helper to unlink an edge from a node's incoming list
    unsafe fn unlink_incoming(
        &self,
        token: &mut GhostToken<'brand>,
        node_ptr: NonNull<GhostCell<'brand, NodeData<V>>>,
        edge_idx: usize,
    ) {
        let node = &*node_ptr.as_ptr();

        // Read head without holding borrow
        let head = node.borrow(token).head_incoming;
        let mut curr = head;
        let mut prev_idx: Option<usize> = None;

        while let Some(curr_idx) = curr {
            // Read next from edges
            let next = self.edges.get(token, curr_idx).unwrap().next_incoming;

            if curr_idx == edge_idx {
                if let Some(p) = prev_idx {
                    self.edges.get_mut(token, p).unwrap().next_incoming = next;
                } else {
                    node.borrow_mut(token).head_incoming = next;
                }
                return;
            }

            prev_idx = Some(curr_idx);
            curr = next;
        }
    }

    // Helper to unlink an edge from a node's outgoing list
    unsafe fn unlink_outgoing(
        &self,
        token: &mut GhostToken<'brand>,
        node_ptr: NonNull<GhostCell<'brand, NodeData<V>>>,
        edge_idx: usize,
    ) {
        let node = &*node_ptr.as_ptr();

        let head = node.borrow(token).head_outgoing;
        let mut curr = head;
        let mut prev_idx: Option<usize> = None;

        while let Some(curr_idx) = curr {
            let next = self.edges.get(token, curr_idx).unwrap().next_outgoing;

            if curr_idx == edge_idx {
                if let Some(p) = prev_idx {
                    self.edges.get_mut(token, p).unwrap().next_outgoing = next;
                } else {
                    node.borrow_mut(token).head_outgoing = next;
                }
                return;
            }

            prev_idx = Some(curr_idx);
            curr = next;
        }
    }

    /// Iterates over outgoing neighbors of a node.
    pub fn neighbors<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node: &'a GhostCell<'brand, NodeData<V>>,
    ) -> Neighbors<'a, 'brand, V, E> {
        Neighbors {
            graph: self,
            curr_edge: node.borrow(token).head_outgoing,
            _token: token,
        }
    }
}

impl<'brand, V, E> Default for AdjListGraph<'brand, V, E> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Neighbors<'a, 'brand, V, E> {
    graph: &'a AdjListGraph<'brand, V, E>,
    curr_edge: Option<usize>,
    _token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, V, E> Iterator for Neighbors<'a, 'brand, V, E> {
    type Item = (&'a GhostCell<'brand, NodeData<V>>, &'a E);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.curr_edge?;
        let edge = self.graph.edges.get(self._token, idx)?;
        self.curr_edge = edge.next_outgoing;

        let target_node = unsafe { &*edge.target.as_ptr() };
        Some((target_node, &edge.weight))
    }
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_adj_graph_basic() {
        GhostToken::new(|mut token| {
            let graph = AdjListGraph::new();

            let n1 = graph.add_node(&mut token, 1);
            let n2 = graph.add_node(&mut token, 2);

            graph.add_edge(&mut token, &n1, &n2, 100);

            let neighbors: Vec<_> = graph.neighbors(&token, &n1).collect();
            assert_eq!(neighbors.len(), 1);
            assert_eq!(*neighbors[0].1, 100);

            // Remove node
            let val = graph.remove_node(&mut token, n1);
            assert_eq!(val, 1);

            // Must remove n2 to satisfy StaticRc linearity
            graph.remove_node(&mut token, n2);
        });
    }
}
