#![cfg(unix)]

use libc::{c_void, mmap, munmap, mprotect, MAP_PRIVATE, MAP_ANONYMOUS, PROT_READ, PROT_WRITE, MAP_FAILED};
use std::ptr;

/// Allocates a memory region of `size` bytes.
/// Returns a pointer to the start of the region, or None if allocation failed.
pub unsafe fn allocate_region(size: usize) -> Option<*mut u8> {
    let ptr = mmap(
        ptr::null_mut(),
        size,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        -1,
        0,
    );

    if ptr == MAP_FAILED {
        None
    } else {
        Some(ptr as *mut u8)
    }
}

pub unsafe fn free_region(ptr: *mut u8, size: usize) {
    munmap(ptr as *mut c_void, size);
}

pub unsafe fn protect_region(ptr: *mut u8, size: usize, readonly: bool) -> bool {
    let prot = if readonly { PROT_READ } else { PROT_READ | PROT_WRITE };
    mprotect(ptr as *mut c_void, size, prot) == 0
}
