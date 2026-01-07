//! Basic graph implementations.
//!
//! This module contains fundamental graph representations that provide
//! the core building blocks for graph algorithms.

pub mod adjacency_graph;
pub mod bipartite_graph;
pub mod dag;

pub use adjacency_graph::GhostAdjacencyGraph;
pub use bipartite_graph::GhostBipartiteGraph;
pub use dag::GhostDag;


