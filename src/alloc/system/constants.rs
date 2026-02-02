pub use crate::alloc::page::{PAGE_SIZE, align_up};
pub use crate::alloc::segregated::size_class::{SC16, SC32, SC64, SC128, SC256, SC512, SC1024, SC2048};
use crate::alloc::segregated::slab::SegregatedSlab;

const fn slab_header_size() -> usize {
    core::mem::size_of::<SegregatedSlab<'static, 16, 1>>()
}

const fn objects_per_slab(object_size: usize) -> usize {
    let header = slab_header_size();
    let start = align_up(header, object_size);
    if start >= PAGE_SIZE {
        0
    } else {
        (PAGE_SIZE - start) / object_size
    }
}

pub const N16: usize = objects_per_slab(16);
pub const N32: usize = objects_per_slab(32);
pub const N64: usize = objects_per_slab(64);
pub const N128: usize = objects_per_slab(128);
pub const N256: usize = objects_per_slab(256);
pub const N512: usize = objects_per_slab(512);
pub const N1024: usize = objects_per_slab(1024);
pub const N2048: usize = objects_per_slab(2048);
