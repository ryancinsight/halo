pub mod allocator;
pub mod arena;
pub mod bump;
pub mod pool;
pub mod generational_pool;
pub mod slab;

pub use allocator::{AllocError, GhostAlloc};
pub use arena::BrandedArena;
pub use bump::BrandedBumpAllocator;
pub use pool::BrandedPool;
pub use generational_pool::GenerationalPool;
pub use slab::BrandedSlab;

pub mod branded;
pub mod branded_box;
pub mod branded_rc;
pub mod heap;
pub mod static_rc;

pub use branded_box::BrandedBox;
pub use branded_rc::BrandedRc;
pub use static_rc::StaticRc;

// TODO: Investigate integrating with the `GlobalAlloc` trait.
// While brands make direct implementation difficult, we might provide a branded wrapper
// that can replace the global allocator within a specific scope or thread.

// TODO: Expand documentation (allocator comparison) with detailed benchmarks.
// Comparing allocation throughput and latency against mimalloc and snmalloc would be valuable.
