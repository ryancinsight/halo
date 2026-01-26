use crate::alloc::segregated::size_class::SC;

// Re-export size classes for convenience
pub type SC16 = SC<16>;
pub type SC32 = SC<32>;
pub type SC64 = SC<64>;
pub type SC128 = SC<128>;
pub type SC256 = SC<256>;
pub type SC512 = SC<512>;
pub type SC1024 = SC<1024>;
pub type SC2048 = SC<2048>;

// Estimate header size.
// BrandedSlab header:
// next_slab: 8
// next_all: 8
// freelist: 8
// bump_index: 8
// alloc_cnt: 8
// _marker: 0
// Total ~40 bytes + padding. Let's be safe and assume 64 bytes.
pub const SLAB_HEADER_SIZE: usize = 64;
pub const PAGE_SIZE: usize = 4096;
pub const AVAILABLE_BYTES: usize = PAGE_SIZE - SLAB_HEADER_SIZE;

pub const N16: usize = AVAILABLE_BYTES / 16;
pub const N32: usize = AVAILABLE_BYTES / 32;
pub const N64: usize = AVAILABLE_BYTES / 64;
pub const N128: usize = AVAILABLE_BYTES / 128;
pub const N256: usize = AVAILABLE_BYTES / 256;
pub const N512: usize = AVAILABLE_BYTES / 512;
pub const N1024: usize = AVAILABLE_BYTES / 1024;
pub const N2048: usize = AVAILABLE_BYTES / 2048;

pub const MAX_SMALL_SIZE: usize = 2048;

// Bootstrap constants
pub const BOOTSTRAP_RESERVE_SIZE: usize = 64 * 1024 * 1024;
