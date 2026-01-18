//! Branded Radix Trie implementation.
//!
//! A high-performance radix trie (prefix tree) optimized for branded usage.
//! It uses `BrandedVec` as a node arena to ensure cache locality and
//! supports safe interior mutability via `GhostToken`.

pub mod node;
pub mod map;
pub mod set;
pub mod iter;

pub use map::BrandedRadixTrieMap;
pub use set::BrandedRadixTrieSet;
