//! Fused graph algorithms and iterators for `AdjListGraph`.
//!
//! This module provides iterator-based graph traversals (BFS, DFS) and
//! other algorithms like connected components, designed for zero-copy
//! efficiency and direct integration with `GhostToken` scopes.

use crate::collections::{ActiveDisjointSet, BrandedDisjointSet};
use crate::graph::basic::adj_list::FastAdjListGraph;
use crate::GhostToken;
use std::collections::VecDeque;

/// An iterator for Breadth-First Search (BFS).
///
/// This iterator yields node IDs (`usize`) in BFS order.
/// It uses an internal `VecDeque` and `Vec<bool>` for state management.
pub struct Bfs<'a, 'brand, E> {
    graph: FastAdjListGraph<'a, 'brand, E>,
    visited: Vec<bool>,
    queue: VecDeque<usize>,
}

impl<'a, 'brand, E> Bfs<'a, 'brand, E> {
    /// Creates a new BFS iterator starting from `start_node`.
    pub fn new(graph: FastAdjListGraph<'a, 'brand, E>, start_node: usize) -> Self {
        let len = graph.node_count();
        let mut visited = vec![false; len];
        let mut queue = VecDeque::new();

        if start_node < len {
            visited[start_node] = true;
            queue.push_back(start_node);
        }

        Self {
            graph,
            visited,
            queue,
        }
    }
}

impl<'a, 'brand, E> Iterator for Bfs<'a, 'brand, E> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let u = self.queue.pop_front()?;

        for (v, _) in self.graph.neighbor_indices(u) {
            if v < self.visited.len() && !self.visited[v] {
                self.visited[v] = true;
                self.queue.push_back(v);
            }
        }

        Some(u)
    }
}

/// An iterator for Depth-First Search (DFS).
///
/// This iterator yields node IDs (`usize`) in DFS order.
/// It uses an internal `Vec` (stack) and `Vec<bool>` for state management.
pub struct Dfs<'a, 'brand, E> {
    graph: FastAdjListGraph<'a, 'brand, E>,
    visited: Vec<bool>,
    stack: Vec<usize>,
}

impl<'a, 'brand, E> Dfs<'a, 'brand, E> {
    /// Creates a new DFS iterator starting from `start_node`.
    pub fn new(graph: FastAdjListGraph<'a, 'brand, E>, start_node: usize) -> Self {
        let len = graph.node_count();
        let mut visited = vec![false; len];
        let mut stack = Vec::new();

        if start_node < len {
            visited[start_node] = true;
            stack.push(start_node);
        }

        Self {
            graph,
            visited,
            stack,
        }
    }
}

impl<'a, 'brand, E> Iterator for Dfs<'a, 'brand, E> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let u = self.stack.pop()?;

        for (v, _) in self.graph.neighbor_indices(u) {
            if v < self.visited.len() && !self.visited[v] {
                self.visited[v] = true;
                self.stack.push(v);
            }
        }

        Some(u)
    }
}

/// Computes the connected components of the graph.
///
/// Returns a vector where the index corresponds to the node ID,
/// and the value is the component ID (representative node ID).
///
/// This function uses `BrandedDisjointSet` internally for efficiency.
pub fn connected_components<'a, 'brand, E>(
    graph: FastAdjListGraph<'a, 'brand, E>,
) -> Vec<usize> {
    let len = graph.node_count();

    // Create a new branded scope for the disjoint set
    GhostToken::new(|mut ds_token| {
        let mut ds = BrandedDisjointSet::with_capacity(len);
        let mut active_ds = ActiveDisjointSet::new(&mut ds, &mut ds_token);

        // Initialize sets for all nodes
        for _ in 0..len {
            active_ds.make_set();
        }

        // Iterate over all nodes and their edges
        for u in 0..len {
            for (v, _) in graph.neighbor_indices(u) {
                // Union the sets containing u and v
                active_ds.union(u, v);
            }
        }

        // Extract component IDs
        // We use find() to get the representative for each node
        let mut components = Vec::with_capacity(len);
        for u in 0..len {
            components.push(active_ds.find(u));
        }
        components
    })
}
