//! Specialized graph implementations.
//!
//! This module contains advanced graph representations designed for
//! specific use cases and performance characteristics.

pub mod amt_graph;
pub mod lel_graph;

pub use amt_graph::GhostAmtGraph;
pub use lel_graph::GhostLelGraph;


