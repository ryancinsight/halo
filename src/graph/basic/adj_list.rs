//! Intrusive Adjacency List Graph
//!
//! A graph implementation where nodes are allocated individually (via `StaticRc`)
//! and edges are stored in a branded pool (Tripod-style linked lists).
//!
//! This design allows nodes to be managed with explicit ownership handles (`StaticRc`)
//! held by the user, ensuring that nodes cannot be used after removal from the graph,
//! while edges are compactly stored in a memory pool for cache efficiency.
//!
//! # Optimization: Structure of Arrays (SoA)
//! This implementation uses a SoA layout for node topology. The `head_outgoing` and
//! `head_incoming` edge pointers are stored in a `Vec` (wrapped in `GhostCell`) parallel
//! to the `nodes` pool, rather than in the heap-allocated `NodeData`. This ensures that
//! graph traversals (BFS/DFS) iterate over contiguous vectors and avoid pointer
//! chasing to random heap locations for each visited node.

use crate::alloc::pool::PoolSlot;
use crate::alloc::{BrandedPool, StaticRc};
use crate::cell::GhostCell;
use crate::collections::other::trusted_index::TrustedIndex;
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
pub struct NodeData<'brand, V> {
    /// The user-provided value.
    pub value: V,
    /// Index of the `StaticRc` handle in the graph's node pool.
    pub(crate) pool_idx: usize,
    /// Marker for brand.
    pub(crate) _marker: PhantomData<&'brand ()>,
}

/// Topology data for a node, stored in a dense vector.
#[derive(Copy, Clone, Default)]
struct NodeTopology<'brand> {
    /// Head of the outgoing edge list (index into edge pool).
    pub(crate) head_outgoing: Option<TrustedIndex<'brand>>,
    /// Head of the incoming edge list (index into edge pool).
    pub(crate) head_incoming: Option<TrustedIndex<'brand>>,
}

/// Internal edge data structure stored in the pool.
pub(crate) struct EdgeData<'brand, E> {
    pub weight: E,
    pub target_idx: TrustedIndex<'brand>,
    pub source_idx: TrustedIndex<'brand>,
    pub next_outgoing: Option<TrustedIndex<'brand>>,
    pub next_incoming: Option<TrustedIndex<'brand>>,
    pub _marker: PhantomData<&'brand ()>,
}

/// A handle to a graph node, representing 50% ownership.
///
/// The user holds this handle to keep the node alive and access its data.
/// To remove the node, this handle must be returned to the graph.
pub type NodeHandle<'brand, V> = StaticRc<'brand, GhostCell<'brand, NodeData<'brand, V>>, 1, 2>;

/// An intrusive adjacency list graph.
pub struct AdjListGraph<'brand, V, E, Ty = Directed> {
    /// Pool containing the graph's share of the node handles.
    nodes: BrandedPool<'brand, NodeHandle<'brand, V>>,
    /// Dense vector storing node topology (edges). Indexed by pool_idx.
    node_topology: GhostCell<'brand, Vec<NodeTopology<'brand>>>,
    /// Pool containing the edges.
    edges: BrandedPool<'brand, EdgeData<'brand, E>>,
    _marker: PhantomData<Ty>,
}

impl<'brand, V, E> AdjListGraph<'brand, V, E, Undirected> {
    /// Creates a new empty undirected graph.
    pub fn new_undirected() -> Self {
        Self {
            nodes: BrandedPool::new(),
            node_topology: GhostCell::new(Vec::new()),
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
            node_topology: GhostCell::new(Vec::new()),
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
            pool_idx: usize::MAX,
            _marker: PhantomData,
        };

        // Create the StaticRc with N=D=2.
        let full_rc: StaticRc<'brand, _, 2, 2> = StaticRc::new(GhostCell::new(node_data));

        // Split into two halves (1/2).
        let (h1, h2) = full_rc.split::<1, 1>();

        // Store one half in the graph's node pool.
        let idx = self.nodes.alloc(token, h1);

        // Update the pool_idx in the node data.
        h2.borrow_mut(token).pool_idx = idx;

        // Ensure topology storage
        let topology = self.node_topology.borrow_mut(token);
        if idx >= topology.len() {
             // We need to resize. Since alloc usually fills holes or appends 1,
             // and we want dense indexing matching pool storage, we can push default.
             // Pool index can be anything < pool capacity.
             // BrandedPool logic: if free_head, use it. Else push.
             // If push, idx == len.
             // If free_head, idx < len.
             // So we only need to push if idx == len (which is >= len).
             if idx == topology.len() {
                 topology.push(NodeTopology::default());
             } else {
                 // Should be already allocated if reusing.
                 // Ensure bounds just in case logic drifts (e.g. pool impl changes)
                 while topology.len() <= idx {
                     topology.push(NodeTopology::default());
                 }
                 // Reset the slot if it was reused (though fields are overwritten anyway)
                 topology[idx] = NodeTopology::default();
             }
        } else {
            // Reusing a slot, clear it
            topology[idx] = NodeTopology::default();
        }

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

        // 4. Clean up edges using topology.

        // Remove outgoing edges
        // We cannot iterate the linked list while holding a mutable borrow of topology (needed for update)
        // OR while holding the token for edge access if we hold topology.
        // Strategy: Iterate by reading head from topology (short borrow), then accessing edges (token),
        // then updating topology if needed (short borrow).

        // However, unlink_incoming/unlink_outgoing traverse the list on the OTHER node.
        // We are iterating THIS node's list to find which other nodes to update.

        let mut curr = self.node_topology.borrow(token)[pool_idx].head_outgoing;
        while let Some(edge_idx_trusted) = curr {
            let edge_idx = edge_idx_trusted.get();
            // Read edge data to find next and target
            // SAFETY: edge exists
            let (next_edge, target_idx) = {
                let edge_data = self.edges.get(token, edge_idx).expect("Corrupt edge list");
                (edge_data.next_outgoing, edge_data.target_idx.get())
            };

            unsafe {
                self.unlink_incoming(token, target_idx, edge_idx);
            }
            unsafe { self.edges.take(token, edge_idx) };

            curr = next_edge;
        }

        // Remove incoming edges
        let mut curr = self.node_topology.borrow(token)[pool_idx].head_incoming;
        while let Some(edge_idx_trusted) = curr {
            let edge_idx = edge_idx_trusted.get();
            let (next_edge, source_idx) = {
                let edge_data = self.edges.get(token, edge_idx).expect("Corrupt edge list");
                (edge_data.next_incoming, edge_data.source_idx.get())
            };

            unsafe {
                self.unlink_outgoing(token, source_idx, edge_idx);
            }
            unsafe { self.edges.take(token, edge_idx) };

            curr = next_edge;
        }

        // 5. Drop full_rc, which deallocates the NodeData.
        let cell = *full_rc.into_box();
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
        let source_idx = source.borrow(token).pool_idx;
        let target_idx = target.borrow(token).pool_idx;

        let source_idx_trusted = unsafe { TrustedIndex::new_unchecked(source_idx) };
        let target_idx_trusted = unsafe { TrustedIndex::new_unchecked(target_idx) };

        // Read current heads
        let (next_outgoing, next_incoming) = {
             let topo = self.node_topology.borrow(token);
             (topo[source_idx].head_outgoing, topo[target_idx].head_incoming)
        };

        // Allocate edge
        let edge = EdgeData {
            weight,
            source_idx: source_idx_trusted,
            target_idx: target_idx_trusted,
            next_outgoing,
            next_incoming,
            _marker: PhantomData,
        };

        let edge_idx = self.edges.alloc(token, edge);
        let edge_idx_trusted = unsafe { TrustedIndex::new_unchecked(edge_idx) };

        // Update heads
        let topo = self.node_topology.borrow_mut(token);
        topo[source_idx].head_outgoing = Some(edge_idx_trusted);
        topo[target_idx].head_incoming = Some(edge_idx_trusted);
    }

    // Helper to unlink an edge from a node's incoming list
    unsafe fn unlink_incoming(
        &self,
        token: &mut GhostToken<'brand>,
        node_idx: usize,
        edge_idx: usize,
    ) {
        // 1. Read head.
        let head = self.node_topology.borrow(token)[node_idx].head_incoming;

        let mut curr = head;
        let mut prev_idx: Option<usize> = None;

        while let Some(curr_idx_trusted) = curr {
            let curr_idx = curr_idx_trusted.get();
            let next = self.edges.get(token, curr_idx).unwrap().next_incoming;

            if curr_idx == edge_idx {
                if let Some(p) = prev_idx {
                    self.edges.get_mut(token, p).unwrap().next_incoming = next;
                } else {
                    // Update head in topology
                    self.node_topology.borrow_mut(token)[node_idx].head_incoming = next;
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
        node_idx: usize,
        edge_idx: usize,
    ) {
        let head = self.node_topology.borrow(token)[node_idx].head_outgoing;

        let mut curr = head;
        let mut prev_idx: Option<usize> = None;

        while let Some(curr_idx_trusted) = curr {
            let curr_idx = curr_idx_trusted.get();
            let next = self.edges.get(token, curr_idx).unwrap().next_outgoing;

            if curr_idx == edge_idx {
                if let Some(p) = prev_idx {
                    self.edges.get_mut(token, p).unwrap().next_outgoing = next;
                } else {
                    self.node_topology.borrow_mut(token)[node_idx].head_outgoing = next;
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
        node: &'a GhostCell<'brand, NodeData<'brand, V>>,
    ) -> Neighbors<'a, 'brand, V, E, Ty> {
        let pool_idx = node.borrow(token).pool_idx;
        let curr_edge = self.node_topology.borrow(token)[pool_idx].head_outgoing;
        Neighbors {
            graph: self,
            curr_edge,
            _token: token,
        }
    }

    /// Returns the unique integer ID (pool index) of a node.
    pub fn node_id(
        &self,
        token: &GhostToken<'brand>,
        handle: &NodeHandle<'brand, V>,
    ) -> usize {
        handle.borrow(token).pool_idx
    }

    /// Returns the unique integer ID (pool index) of a node from its cell.
    pub fn node_id_from_cell(
        &self,
        token: &GhostToken<'brand>,
        cell: &GhostCell<'brand, NodeData<'brand, V>>,
    ) -> usize {
        cell.borrow(token).pool_idx
    }

    /// Iterates over outgoing neighbor IDs and edge weights.
    ///
    /// This iterator yields `(target_node_idx, &weight)`. It is faster than `neighbors` because
    /// it avoids accessing the target node's memory to retrieve its data or ID.
    pub fn neighbor_indices<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node: &'a GhostCell<'brand, NodeData<'brand, V>>,
    ) -> NeighborIndices<'a, 'brand, V, E, Ty> {
        let pool_idx = node.borrow(token).pool_idx;
        let curr_edge = self.node_topology.borrow(token)[pool_idx].head_outgoing;
        NeighborIndices {
            graph: self,
            curr_edge,
            _token: token,
        }
    }

    /// Iterates over outgoing neighbor IDs and edge weights given a node ID.
    ///
    /// This method is fully SoA-optimized: it uses the graph's topology vectors
    /// and edge pool directly, avoiding all heap accesses to `NodeData`.
    pub fn neighbor_indices_by_id<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node_id: usize,
    ) -> NeighborIndices<'a, 'brand, V, E, Ty> {
        // Direct vector access, no GhostCell deref of NodeData!
        // Safety: Caller must ensure node_id is valid (allocated).
        // If out of bounds, Vec index panics (safe).
        let curr_edge = self.node_topology.borrow(token)[node_id].head_outgoing;
        NeighborIndices {
            graph: self,
            curr_edge,
            _token: token,
        }
    }

    /// Returns a reference to the node cell given its ID.
    #[inline]
    pub unsafe fn get_node_unchecked<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        node_id: usize,
    ) -> &'a GhostCell<'brand, NodeData<'brand, V>> {
        let handle = self.nodes.get_unchecked(token, node_id);
        handle.get()
    }

    /// Performs a Breadth-First Search (BFS) starting from `start_node`.
    ///
    /// This method is fully optimized for the SoA layout:
    /// - Uses `Vec<bool>` for dense visited tracking (cache friendly).
    /// - Uses `neighbor_indices_by_id` to traverse topology without heap accesses.
    /// - Returns a vector of visited node IDs in traversal order.
    pub fn bfs(
        &self,
        token: &GhostToken<'brand>,
        start_node: usize,
    ) -> Vec<usize> {
        let topology = self.node_topology.borrow(token);
        let mut visited = vec![false; topology.len()];
        let mut queue = std::collections::VecDeque::new();
        let mut result = Vec::new();

        if start_node < visited.len() {
            visited[start_node] = true;
            queue.push_back(start_node);
        }

        while let Some(u) = queue.pop_front() {
            result.push(u);

            // Use neighbor_indices_by_id manually here to avoid borrowing self immutably
            // while we might want to do other things (though here we just push to queue).
            // Actually neighbor_indices_by_id borrows self.node_topology (immutably).
            // We already borrowed topology above to get len.
            // We need to drop that borrow or reuse it.
            // neighbor_indices_by_id re-borrows. GhostCell allows multiple shared borrows.
            // But RefCell/GhostCell borrow runtime check might fail if we hold a ref?
            // GhostCell::borrow returns a reference `&T`. It does NOT use a runtime lock like RefCell!
            // It uses the compile-time token.
            // So we can have multiple references.

            // Wait, GhostCell::borrow returns `&T`.
            // `topology` variable is `&Vec<NodeTopology>`.
            // `neighbor_indices_by_id` calls `self.node_topology.borrow(token)`.
            // This returns `&Vec<NodeTopology>`.
            // This is allowed.

            for (v, _) in self.neighbor_indices_by_id(token, u) {
                if v < visited.len() && !visited[v] {
                    visited[v] = true;
                    queue.push_back(v);
                }
            }
        }

        result
    }
}

impl<'brand, V, E, Ty> Default for AdjListGraph<'brand, V, E, Ty> {
    fn default() -> Self {
        Self {
            nodes: BrandedPool::new(),
            node_topology: GhostCell::new(Vec::new()),
            edges: BrandedPool::new(),
            _marker: PhantomData,
        }
    }
}

pub struct Neighbors<'a, 'brand, V, E, Ty> {
    graph: &'a AdjListGraph<'brand, V, E, Ty>,
    curr_edge: Option<TrustedIndex<'brand>>,
    _token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, V, E, Ty> Iterator for Neighbors<'a, 'brand, V, E, Ty> {
    type Item = (&'a GhostCell<'brand, NodeData<'brand, V>>, &'a E);

    fn next(&mut self) -> Option<Self::Item> {
        let trusted_idx = self.curr_edge?;
        let idx = trusted_idx.get();

        // SAFETY: `trusted_idx` is a `TrustedIndex` valid for this brand.
        let edge = unsafe { self.graph.edges.get_unchecked(self._token, idx) };

        self.curr_edge = edge.next_outgoing;

        let target_handle = unsafe { self.graph.nodes.get_unchecked(self._token, edge.target_idx.get()) };
        let target_node = target_handle.get();
        Some((target_node, &edge.weight))
    }
}

pub struct NeighborIndices<'a, 'brand, V, E, Ty> {
    graph: &'a AdjListGraph<'brand, V, E, Ty>,
    curr_edge: Option<TrustedIndex<'brand>>,
    _token: &'a GhostToken<'brand>,
}

impl<'a, 'brand, V, E, Ty> Iterator for NeighborIndices<'a, 'brand, V, E, Ty> {
    type Item = (usize, &'a E);

    fn next(&mut self) -> Option<Self::Item> {
        let trusted_idx = self.curr_edge?;
        let idx = trusted_idx.get();

        // SAFETY: `trusted_idx` is a `TrustedIndex` valid for this brand.
        let edge = unsafe { self.graph.edges.get_unchecked(self._token, idx) };

        self.curr_edge = edge.next_outgoing;

        // Optimized: we get target_idx directly from the edge, no pointer deref!
        Some((edge.target_idx.get(), &edge.weight))
    }
}

/// A map generated during snapshotting to retrieve new handles from old ones.
pub struct SnapshotMap<'brand, V> {
    map: Vec<Option<NodeHandle<'brand, V>>>,
}

impl<'brand, V> SnapshotMap<'brand, V> {
    /// Retrieves (takes) the new handle corresponding to an old handle.
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
            let new_data = NodeData {
                value: old_data.value.clone(),
                pool_idx: old_data.pool_idx,
                _marker: PhantomData,
            };

            let full_rc: StaticRc<'new_brand, _, 2, 2> =
                StaticRc::new(GhostCell::new(new_data));
            let (h1, h2) = full_rc.split::<1, 1>();
            (h1, h2)
        });

        // 2. Clone topology
        let old_topology = self.node_topology.borrow(token);
        let new_topology_vec: Vec<NodeTopology<'new_brand>> = old_topology.iter().map(|t| {
             NodeTopology {
                 head_outgoing: t.head_outgoing.map(|i| unsafe { TrustedIndex::new_unchecked(i.get()) }),
                 head_incoming: t.head_incoming.map(|i| unsafe { TrustedIndex::new_unchecked(i.get()) }),
             }
        }).collect();

        // 3. Clone edges
        let (new_edges, _) = self.edges.clone_structure(token, |old_edge| {
            let next_outgoing = old_edge
                .next_outgoing
                .map(|i| unsafe { TrustedIndex::new_unchecked(i.get()) });
            let next_incoming = old_edge
                .next_incoming
                .map(|i| unsafe { TrustedIndex::new_unchecked(i.get()) });

            let target_idx = unsafe { TrustedIndex::new_unchecked(old_edge.target_idx.get()) };
            let source_idx = unsafe { TrustedIndex::new_unchecked(old_edge.source_idx.get()) };

            (
                EdgeData {
                    weight: old_edge.weight.clone(),
                    target_idx,
                    source_idx,
                    next_outgoing,
                    next_incoming,
                    _marker: PhantomData,
                },
                (),
            )
        });

        (
            AdjListGraph {
                nodes: new_nodes,
                node_topology: GhostCell::new(new_topology_vec),
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

    #[test]
    fn test_neighbor_indices() {
        GhostToken::new(|mut token| {
            let graph = AdjListGraph::new();
            let n1 = graph.add_node(&mut token, 10);
            let n2 = graph.add_node(&mut token, 20);
            let n3 = graph.add_node(&mut token, 30);

            graph.add_edge(&mut token, &n1, &n2, 1);
            graph.add_edge(&mut token, &n1, &n3, 2);

            let n1_id = graph.node_id(&token, &n1);
            let n2_id = graph.node_id(&token, &n2);
            let n3_id = graph.node_id(&token, &n3);

            let neighbors: Vec<_> = graph.neighbor_indices(&token, n1.get()).collect();
            assert_eq!(neighbors.len(), 2);

            // Edges are added at head, so order might be reversed (LIFO)
            assert_eq!(neighbors[0].0, n3_id);
            assert_eq!(*neighbors[0].1, 2);

            assert_eq!(neighbors[1].0, n2_id);
            assert_eq!(*neighbors[1].1, 1);

            graph.remove_node(&mut token, n1);
            graph.remove_node(&mut token, n2);
            graph.remove_node(&mut token, n3);
        });
    }
}
