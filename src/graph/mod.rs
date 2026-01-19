//! Graph layouts and traversals designed to compose with Ghost-style patterns.
//!
//! Graph implementations are organized into categories:
//! - `basic`: Fundamental graph representations
//! - `compressed`: Memory-efficient compressed formats
//! - `specialized`: Advanced representations for specific use cases

pub mod basic;
pub mod compressed;
pub mod specialized;
pub(crate) mod access;

// Re-export commonly used types from submodules
pub use basic::{GhostAdjacencyGraph, GhostBipartiteGraph, GhostDag, BrandedPoolGraph};
pub use compressed::{GhostCscGraph, GhostCsrGraph};






