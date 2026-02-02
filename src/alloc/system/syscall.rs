use core::ptr;
use crate::alloc::page::PAGE_SIZE;

use crate::alloc::page::align_up;

#[cfg(unix)]
pub unsafe fn allocate_region(size: usize) -> Option<*mut u8> {
    if size == 0 {
        return None;
    }
    let size = align_up(size, PAGE_SIZE);
    let ptr = libc::mmap(
        ptr::null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANON,
        -1,
        0,
    );
    if ptr == libc::MAP_FAILED {
        None
    } else {
        Some(ptr as *mut u8)
    }
}

#[cfg(unix)]
pub unsafe fn free_region(ptr: *mut u8, size: usize) {
    if ptr.is_null() || size == 0 {
        return;
    }
    let size = align_up(size, PAGE_SIZE);
    libc::munmap(ptr as *mut libc::c_void, size);
}

#[cfg(windows)]
pub unsafe fn allocate_region(size: usize) -> Option<*mut u8> {
    use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE};
    if size == 0 {
        return None;
    }
    let size = align_up(size, PAGE_SIZE);
    let ptr = VirtualAlloc(ptr::null_mut(), size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if ptr.is_null() {
        None
    } else {
        Some(ptr as *mut u8)
    }
}

#[cfg(windows)]
pub unsafe fn free_region(ptr: *mut u8, _size: usize) {
    use windows_sys::Win32::System::Memory::{VirtualFree, MEM_RELEASE};
    if ptr.is_null() {
        return;
    }
    VirtualFree(ptr as *mut core::ffi::c_void, 0, MEM_RELEASE);
}
