use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

// Constants
pub const MAX_CPUS: usize = 128;
const SYS_RSEQ: i64 = 334;
const RSEQ_FLAG_UNREGISTER: u32 = 1;
const RSEQ_SIG: u32 = 0x53053053; // Arbitrary signature

// The Per-CPU cache structure.
// Must be repr(C) so assembly knows offsets.
#[repr(C)]
pub struct PerCpuCache {
    pub count: usize,
    pub items: [Option<NonNull<u8>>; 32], // Small cache size
}

impl PerCpuCache {
    pub const fn new() -> Self {
        Self {
            count: 0,
            items: [None; 32],
        }
    }
}

// Wrapper to allow Sync (since RSEQ ensures exclusive access per CPU)
#[repr(C)]
pub struct RseqCache {
    pub cell: UnsafeCell<PerCpuCache>,
}

unsafe impl Sync for RseqCache {}

impl RseqCache {
    pub const fn new() -> Self {
        Self {
            cell: UnsafeCell::new(PerCpuCache::new()),
        }
    }
}

// RSEQ structures
#[repr(C, align(32))]
#[derive(Debug)]
pub struct Rseq {
    pub cpu_id_start: u32,
    pub cpu_id: u32,
    pub rseq_cs: u64,
    pub flags: u32,
    pub _pad: [u8; 64],
}

impl Default for Rseq {
    fn default() -> Self {
        Self {
            cpu_id_start: 0,
            cpu_id: 0,
            rseq_cs: 0,
            flags: 0,
            _pad: [0; 64],
        }
    }
}

#[repr(C, align(32))]
#[derive(Debug, Default)]
pub struct RseqCs {
    pub version: u32,
    pub flags: u32,
    pub start_ip: u64,
    pub post_commit_offset: u64,
    pub abort_ip: u64,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod internal {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        pub static RSEQ_ABI: RefCell<Rseq> = RefCell::new(Rseq::default());
        pub static REGISTERED: bool = {
            RSEQ_ABI.with(|rseq| {
                let ptr = rseq.as_ptr();
                unsafe {
                    // Register rseq
                    let ret: i32;
                    std::arch::asm!(
                        "syscall",
                        in("rax") SYS_RSEQ,
                        in("rdi") ptr,
                        in("rsi") std::mem::size_of::<Rseq>() as u32,
                        in("rdx") 0, // flags
                        in("r10") RSEQ_SIG,
                        lateout("rax") ret,
                        options(nostack, preserves_flags)
                    );
                    // 0 = success, -1 (EBUSY) = already registered
                    ret == 0 || ret == -16 // EBUSY
                }
            })
        };
    }

    pub fn get_current_cpu() -> Option<usize> {
        // Ensure registered
        if !REGISTERED.with(|&b| b) {
            return None;
        }

        RSEQ_ABI.with(|rseq| {
            unsafe {
                let cpu_id = (*rseq.as_ptr()).cpu_id as usize;
                if cpu_id < MAX_CPUS {
                    Some(cpu_id)
                } else {
                    None
                }
            }
        })
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
mod internal {
    use super::*;
    pub fn get_current_cpu() -> Option<usize> { None }
}

pub use internal::get_current_cpu;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub unsafe fn rseq_pop_safe(
    _caches_base: *mut crate::concurrency::CachePadded<RseqCache>,
    _stride: usize,
) -> Option<NonNull<u8>> {
    use internal::REGISTERED;
    if !REGISTERED.with(|&b| b) { return None; }

    // TODO: Enable ASM implementation once toolchain supports local label relocations correctly.
    None
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub unsafe fn rseq_push_safe(
    _caches_base: *mut crate::concurrency::CachePadded<RseqCache>,
    _stride: usize,
    _ptr: NonNull<u8>,
) -> bool {
    use internal::REGISTERED;
    if !REGISTERED.with(|&b| b) { return false; }

    // TODO: Enable ASM implementation once toolchain supports local label relocations correctly.
    false
}

// Fallback stubs
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub unsafe fn rseq_pop_safe(
    _caches_base: *mut crate::concurrency::CachePadded<RseqCache>,
    _stride: usize,
) -> Option<NonNull<u8>> {
    None
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub unsafe fn rseq_push_safe(
    _caches_base: *mut crate::concurrency::CachePadded<RseqCache>,
    _stride: usize,
    _ptr: NonNull<u8>,
) -> bool {
    false
}
