//! Graph layouts and traversals designed to compose with Ghost-style patterns.
//!
//! Graph implementations are organized into categories:
//! - `basic`: Fundamental graph representations
//! - `compressed`: Memory-efficient compressed formats
//! - `specialized`: Advanced representations for specific use cases

pub(crate) mod access;
pub mod basic;
pub mod compressed;
pub mod specialized;

// Re-export commonly used types from submodules
pub use basic::{BrandedPoolGraph, GhostAdjacencyGraph, GhostBipartiteGraph, GhostDag};
pub use compressed::{GhostCscGraph, GhostCsrGraph};
