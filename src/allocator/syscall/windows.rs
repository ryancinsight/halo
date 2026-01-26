#![cfg(windows)]

use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualFree, VirtualProtect, MEM_COMMIT, MEM_RESERVE, MEM_RELEASE, PAGE_READWRITE, PAGE_READONLY,
};
use std::ptr;

pub unsafe fn allocate_region(size: usize) -> Option<*mut u8> {
    let ptr = VirtualAlloc(
        ptr::null_mut(),
        size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );
    if ptr.is_null() {
        None
    } else {
        Some(ptr as *mut u8)
    }
}

pub unsafe fn free_region(ptr: *mut u8, _size: usize) {
    // MEM_RELEASE frees the entire region reserved by VirtualAlloc. Size must be 0.
    VirtualFree(ptr as *mut _, 0, MEM_RELEASE);
}

pub unsafe fn protect_region(ptr: *mut u8, size: usize, readonly: bool) -> bool {
    let prot = if readonly { PAGE_READONLY } else { PAGE_READWRITE };
    let mut old_prot = 0;
    VirtualProtect(ptr as *mut _, size, prot, &mut old_prot) != 0
}
