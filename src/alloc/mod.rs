pub mod allocator;
pub mod arena;
pub mod bump;
pub mod pool;
pub mod global;
pub mod slab;
pub mod system;

pub use allocator::{AllocError, GhostAlloc};
pub use arena::BrandedArena;
pub use bump::BrandedBumpAllocator;
pub use pool::BrandedPool;
pub use global::{DispatchGlobalAlloc, with_global_allocator};
pub use slab::{BrandedSlab, init_slab_page};

pub mod page;
pub use page::{PageAlloc, GlobalPageAlloc, SyscallPageAlloc};

pub mod branded_box;
pub mod branded_rc;
pub mod static_rc;
pub mod segregated;

pub use branded_box::BrandedBox;
pub use branded_rc::BrandedRc;
pub use static_rc::StaticRc;
pub use system::HaloAllocator;

// # Benchmark Comparison
//
// | Allocator | Time (1000 allocations) | vs System |
// | :--- | :--- | :--- |
// | **System (malloc)** | ~63.0 µs | 1.0x |
// | **Mimalloc** | ~10.6 µs | 5.9x |
// | **Snmalloc** | ~6.6 µs | 9.5x |
// | **Halo (BrandedSlab)** | ~8.4 µs | 7.5x |
// | **Halo (BrandedBump)** | ~6.8 µs | 9.3x |
//
// Halo's allocators provide performance competitive with state-of-the-art global allocators
// like Snmalloc and Mimalloc, while ensuring statically verified thread safety via Ghost Tokens.
// See `docs/allocator_comparison.md` for full details.
