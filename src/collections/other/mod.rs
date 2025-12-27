//! Other collections optimized for Ghost-style usage.
//!
//! This module contains specialized collections like deques and arenas
//! that are branded for safe concurrent access patterns.

pub mod deque;
pub mod arena;

pub use deque::BrandedDeque;
pub use arena::BrandedArena;
