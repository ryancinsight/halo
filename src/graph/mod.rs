//! Graph layouts and traversals designed to compose with Ghost-style patterns.

pub mod ghost_adjacency_graph;
pub mod ghost_bipartite_graph;
pub mod ghost_csc_graph;
pub mod ghost_csr_graph;
pub mod ghost_dag;

pub use ghost_adjacency_graph::GhostAdjacencyGraph;
pub use ghost_bipartite_graph::GhostBipartiteGraph;
pub use ghost_csc_graph::GhostCscGraph;
pub use ghost_csr_graph::GhostCsrGraph;
pub use ghost_dag::GhostDag;






