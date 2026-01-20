//! Basic graph implementations.
//!
//! This module contains fundamental graph representations that provide
//! the core building blocks for graph algorithms.

pub mod adjacency_graph;
pub mod adj_list;
pub mod bipartite_graph;
pub mod dag;
pub mod pool_graph;

pub use adj_list::AdjListGraph;
pub use adjacency_graph::GhostAdjacencyGraph;
pub use bipartite_graph::GhostBipartiteGraph;
pub use dag::GhostDag;
pub use pool_graph::BrandedPoolGraph;
