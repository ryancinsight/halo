use crate::token::hierarchy::Permission;

/// A marker trait for size classes.
pub trait SizeClass: 'static + Copy + Clone + Send + Sync {
    const SIZE: usize;
}

/// A concrete size class carrying a const size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SC<const SIZE: usize>;

impl<const SIZE: usize> SizeClass for SC<SIZE> {
    const SIZE: usize = SIZE;
}

impl<const SIZE: usize> Permission for SC<SIZE> {}

// Canonical size-class aliases
pub type SC16 = SC<16>;
pub type SC32 = SC<32>;
pub type SC64 = SC<64>;
pub type SC128 = SC<128>;
pub type SC256 = SC<256>;
pub type SC512 = SC<512>;
pub type SC1024 = SC<1024>;
pub type SC2048 = SC<2048>;

pub const SLAB_CLASS_COUNT: usize = 8;

const MIN_SIZE_CLASS_SIZE: usize = 16;
const MAX_SIZE_CLASS_SIZE: usize = 2048;

/// Returns the size class index for a given size.
/// Supports sizes from 1 to 2048 bytes.
/// Classes: 16, 32, 64, 128, 256, 512, 1024, 2048.
/// Indices: 0, 1, 2, 3, 4, 5, 6, 7.
#[inline]
pub const fn get_size_class_index(size: usize) -> Option<usize> {
    if size == 0 { return None; }
    if size > MAX_SIZE_CLASS_SIZE { return None; }

    let size = if size < MIN_SIZE_CLASS_SIZE { MIN_SIZE_CLASS_SIZE } else { size };

    // Calculate power of 2
    let size = size.next_power_of_two();
    let shift = size.trailing_zeros();

    // 16 = 2^4 -> index 0
    if shift < 4 { return Some(0); }
    Some((shift - 4) as usize)
}

/// Returns the block size for a given class index.
#[inline]
pub const fn get_block_size(index: usize) -> usize {
    MIN_SIZE_CLASS_SIZE << index
}
