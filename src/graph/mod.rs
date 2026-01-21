//! Graph layouts and traversals designed to compose with Ghost-style patterns.
//!
//! Graph implementations include:
//! - Intrusive `AdjListGraph`
//! - `BrandedPoolGraph`
//! - `GhostAdjacencyGraph`
//! - `GhostBipartiteGraph`
//! - `GhostDag`
//! - Compressed formats (`compressed` module)
//! - Specialized formats (`specialized` module)

pub(crate) mod access;
pub mod adj_list;
pub mod adjacency_graph;
pub mod bipartite_graph;
pub mod compressed;
pub mod dag;
pub mod pool_graph;
pub mod specialized;
pub mod traversal;

// Re-export commonly used types from submodules
pub use adj_list::AdjListGraph;
pub use adjacency_graph::GhostAdjacencyGraph;
pub use bipartite_graph::GhostBipartiteGraph;
pub use compressed::{GhostCscGraph, GhostCsrGraph};
pub use dag::GhostDag;
pub use pool_graph::BrandedPoolGraph;
