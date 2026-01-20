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

/// Marker trait for graph edge directionality.
pub trait EdgeType {
    /// Returns true if the graph is directed.
    fn is_directed() -> bool;
}

/// Marker for directed graphs.
pub struct Directed;
/// Marker for undirected graphs.
pub struct Undirected;

impl EdgeType for Directed {
    fn is_directed() -> bool {
        true
    }
}
impl EdgeType for Undirected {
    fn is_directed() -> bool {
        false
    }
}

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
pub struct AdjListGraph<'brand, V, E, Ty = Directed> {
    /// Pool containing the graph's share of the node handles.
    nodes: BrandedPool<'brand, NodeHandle<'brand, V>>,
    /// Pool containing the edges.
    edges: BrandedPool<'brand, EdgeData<'brand, E, V>>,
    _marker: PhantomData<Ty>,
}

impl<'brand, V, E> AdjListGraph<'brand, V, E, Undirected> {
    /// Creates a new empty undirected graph.
    pub fn new_undirected() -> Self {
        Self {
            nodes: BrandedPool::new(),
            edges: BrandedPool::new(),
            _marker: PhantomData,
        }
    }

    /// Adds an undirected edge (two directed edges) between two nodes.
    pub fn add_undirected_edge(
        &self,
        token: &mut GhostToken<'brand>,
        u: &NodeHandle<'brand, V>,
        v: &NodeHandle<'brand, V>,
        weight: E,
    ) where
        E: Clone,
    {
        self.add_edge(token, u, v, weight.clone());
        self.add_edge(token, v, u, weight);
    }
}

impl<'brand, V, E> AdjListGraph<'brand, V, E, Directed> {
    /// Creates a new empty directed graph.
    pub fn new() -> Self {
        Self {
            nodes: BrandedPool::new(),
            edges: BrandedPool::new(),
            _marker: PhantomData,
        }
    }
}

impl<'brand, V, E, Ty> AdjListGraph<'brand, V, E, Ty> {
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
        let other_half = unsafe { self.nodes.take(token, pool_idx) };

        // 3. Join the handles to regain full ownership.
        let full_rc = handle.join(other_half);

        // 4. Clean up edges.
        let node_ptr = NonNull::from(full_rc.get());
        let _node_ptr_val = node_ptr.as_ptr(); // avoid unused variable warning properly later if needed

        // Remove outgoing edges
        let mut curr = full_rc.borrow(token).head_outgoing;
        while let Some(edge_idx) = curr {
            let edge_data = self.edges.get(token, edge_idx).expect("Corrupt edge list");
            let next_edge = edge_data.next_outgoing;
            let target_ptr = edge_data.target;

            unsafe {
                self.unlink_incoming(token, target_ptr, edge_idx);
            }
            unsafe { self.edges.take(token, edge_idx) };

            curr = next_edge;
        }

        // Remove incoming edges
        let mut curr = full_rc.borrow(token).head_incoming;
        while let Some(edge_idx) = curr {
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
        let cell = *full_rc.into_box();
        cell.into_inner().value
    }

    /// Adds a directed edge between two nodes.
    ///
    /// If the graph is undirected, use `add_undirected_edge` (TODO) or this method
    /// might be adapted. Currently this adds a single directed edge.
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
    ) -> Neighbors<'a, 'brand, V, E, Ty> {
        Neighbors {
            graph: self,
            curr_edge: node.borrow(token).head_outgoing,
            _token: token,
        }
    }
}

impl<'brand, V, E, Ty> Default for AdjListGraph<'brand, V, E, Ty> {
    fn default() -> Self {
        Self {
            nodes: BrandedPool::new(),
            edges: BrandedPool::new(),
            _marker: PhantomData,
        }
    }
}

pub struct Neighbors<'a, 'brand, V, E, Ty> {
    graph: &'a AdjListGraph<'brand, V, E, Ty>,
    curr_edge: Option<usize>,
    _token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, V, E, Ty> Iterator for Neighbors<'a, 'brand, V, E, Ty> {
    type Item = (&'a GhostCell<'brand, NodeData<V>>, &'a E);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.curr_edge?;
        let edge = self.graph.edges.get(self._token, idx)?;
        self.curr_edge = edge.next_outgoing;

        let target_node = unsafe { &*edge.target.as_ptr() };
        Some((target_node, &edge.weight))
    }
}

/// A map generated during snapshotting to retrieve new handles from old ones.
pub struct SnapshotMap<'brand, V> {
    map: Vec<Option<NodeHandle<'brand, V>>>,
}

impl<'brand, V> SnapshotMap<'brand, V> {
    /// Retrieves (takes) the new handle corresponding to an old handle.
    ///
    /// This consumes the handle from the map, so it can only be called once per node.
    pub fn take_new_handle<'old_brand, OLD_V>(
        &mut self,
        token: &GhostToken<'old_brand>,
        old_handle: &NodeHandle<'old_brand, OLD_V>,
    ) -> Option<NodeHandle<'brand, V>> {
        let idx = old_handle.borrow(token).pool_idx;
        self.map.get_mut(idx).and_then(|opt| opt.take())
    }
}

impl<'brand, V, E, Ty> AdjListGraph<'brand, V, E, Ty> {
    /// Creates a deep copy (snapshot) of the graph in a new branding scope.
    ///
    /// Returns the new graph and a `SnapshotMap` to retrieve the new handles.
    pub fn snapshot<'new_brand>(
        &self,
        token: &GhostToken<'brand>,
        _new_token: &mut GhostToken<'new_brand>,
    ) -> (
        AdjListGraph<'new_brand, V, E, Ty>,
        SnapshotMap<'new_brand, V>,
    )
    where
        V: Clone,
        E: Clone,
    {
        // 1. Clone nodes
        let (new_nodes, handle_map_vec) = self.nodes.clone_structure(token, |old_handle| {
            let old_data = old_handle.borrow(token);

            // Clone data
            let new_data = NodeData {
                value: old_data.value.clone(),
                head_outgoing: old_data.head_outgoing,
                head_incoming: old_data.head_incoming,
                pool_idx: old_data.pool_idx,
            };

            // Create new handle
            let full_rc: StaticRc<'new_brand, _, 2, 2> =
                StaticRc::new(GhostCell::new(new_data));
            let (h1, h2) = full_rc.split::<1, 1>();

            // Return graph handle (h1) and user handle (h2)
            (h1, h2)
        });

        // 2. Clone edges
        let (new_edges, _) = self.edges.clone_structure(token, |old_edge| {
            // Fix pointers
            let target_node = unsafe { &*old_edge.target.as_ptr() };
            let target_idx = target_node.borrow(token).pool_idx;

            let source_node = unsafe { &*old_edge.source.as_ptr() };
            let source_idx = source_node.borrow(token).pool_idx;

            // Look up new handles to get new pointers
            let target_handle = handle_map_vec[target_idx]
                .as_ref()
                .expect("Target node missing in snapshot");
            let source_handle = handle_map_vec[source_idx]
                .as_ref()
                .expect("Source node missing in snapshot");

            let new_target_ptr = NonNull::from(target_handle.get());
            let new_source_ptr = NonNull::from(source_handle.get());

            (
                EdgeData {
                    weight: old_edge.weight.clone(),
                    target: new_target_ptr,
                    source: new_source_ptr,
                    next_outgoing: old_edge.next_outgoing,
                    next_incoming: old_edge.next_incoming,
                    _marker: PhantomData,
                },
                (),
            )
        });

        (
            AdjListGraph {
                nodes: new_nodes,
                edges: new_edges,
                _marker: PhantomData,
            },
            SnapshotMap {
                map: handle_map_vec,
            },
        )
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

    #[test]
    fn test_adj_graph_undirected() {
        GhostToken::new(|mut token| {
            let graph = AdjListGraph::new_undirected();
            let n1 = graph.add_node(&mut token, 1);
            let n2 = graph.add_node(&mut token, 2);

            graph.add_undirected_edge(&mut token, &n1, &n2, 100);

            // Check n1 -> n2
            let neighbors1: Vec<_> = graph.neighbors(&token, &n1).collect();
            assert_eq!(neighbors1.len(), 1);
            assert_eq!(neighbors1[0].0.borrow(&token).value, 2);

            // Check n2 -> n1
            let neighbors2: Vec<_> = graph.neighbors(&token, &n2).collect();
            assert_eq!(neighbors2.len(), 1);
            assert_eq!(neighbors2[0].0.borrow(&token).value, 1);

            // Clean up
            graph.remove_node(&mut token, n1);
            graph.remove_node(&mut token, n2);
        });
    }

    #[test]
    fn test_adj_graph_snapshot() {
        GhostToken::new(|mut token| {
            let graph = AdjListGraph::new();
            let n1 = graph.add_node(&mut token, 1);
            let n2 = graph.add_node(&mut token, 2);
            graph.add_edge(&mut token, &n1, &n2, 100);

            // Create snapshot
            GhostToken::new(|mut new_token| {
                let (new_graph, mut map) = graph.snapshot(&token, &mut new_token);

                // Retrieve new handles
                let new_n1 = map.take_new_handle(&token, &n1).unwrap();
                let new_n2 = map.take_new_handle(&token, &n2).unwrap();

                // Check values
                assert_eq!(new_n1.borrow(&new_token).value, 1);
                assert_eq!(new_n2.borrow(&new_token).value, 2);

                // Check edge
                let neighbors: Vec<_> = new_graph.neighbors(&new_token, &new_n1).collect();
                assert_eq!(neighbors.len(), 1);
                assert_eq!(neighbors[0].0.borrow(&new_token).value, 2);
                assert_eq!(*neighbors[0].1, 100);

                // Modify new graph
                new_graph.remove_node(&mut new_token, new_n1);

                // Verify old graph is untouched
                let old_neighbors: Vec<_> = graph.neighbors(&token, &n1).collect();
                assert_eq!(old_neighbors.len(), 1);

                // Cleanup new
                new_graph.remove_node(&mut new_token, new_n2);
            });

            // Cleanup old
            graph.remove_node(&mut token, n1);
            graph.remove_node(&mut token, n2);
        });
    }
}
