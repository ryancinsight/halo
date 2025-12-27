//! Compressed graph implementations.
//!
//! This module contains memory-efficient graph representations optimized
//! for different access patterns and computational workloads.

pub mod compressed_graph;
pub mod csc_graph;
pub mod csr_graph;
pub mod ecc_graph;

pub use compressed_graph::GhostCompressedGraph;
pub use csc_graph::GhostCscGraph;
pub use csr_graph::GhostCsrGraph;
pub use ecc_graph::GhostEccGraph;
