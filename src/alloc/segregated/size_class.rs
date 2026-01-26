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

/// Returns the size class index for a given size.
/// Supports sizes from 1 to 4096 bytes.
/// Classes: 16, 32, 64, 128, 256, 512, 1024, 2048, 4096.
/// Indices: 0, 1, 2, 3, 4, 5, 6, 7, 8.
#[inline]
pub const fn get_size_class_index(size: usize) -> Option<usize> {
    if size == 0 { return None; }
    if size > 4096 { return None; }

    // Minimum size 16
    let size = if size < 16 { 16 } else { size };

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
    16 << index
}
