//! Collections optimized for Ghost-style usage.
//!
//! Collections are organized by data structure type:
//! - `vec`: Vector and vector-like collections
//! - `hash`: Hash-based collections (maps and sets)
//! - `other`: Specialized collections (deques, arenas)

pub mod vec;
pub mod hash;
pub mod other;

// Re-export commonly used types from submodules
pub use vec::{BrandedVec, BrandedVecDeque, BrandedChunkedVec, ChunkedVec};
pub use hash::{BrandedHashMap, BrandedHashSet};
pub use other::{BrandedDeque, BrandedArena};






