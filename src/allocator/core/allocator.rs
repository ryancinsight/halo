use crate::alloc::segregated::manager::SizeClassManager;
use crate::allocator::core::page::SyscallPageAlloc;
use crate::allocator::constants::*;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use crate::token::static_token;
use crate::allocator::integration::thread_cache::CACHES;
use crate::allocator::stats::metrics::METRICS;
use core::cell::Cell;

thread_local! {
    static IN_ALLOCATOR: Cell<bool> = const { Cell::new(false) };
}

struct ReentrancyGuard;

impl ReentrancyGuard {
    fn enter() -> Option<Self> {
        // Use try_with to allow fallback if TLS is not accessible (e.g., during destruction).
        // If try_with fails, we assume we should use the fallback (direct/syscall) path.
        if let Ok(flag_ref) = IN_ALLOCATOR.try_with(|f| f.get()) {
            if flag_ref {
                return None;
            }
        } else {
            // Cannot access TLS (e.g. recursion during init or destruction).
            // Fall back to direct manager/syscall.
            return None;
        }

        // Mark as entered.
        let _ = IN_ALLOCATOR.try_with(|f| f.set(true));
        Some(Self)
    }
}

impl Drop for ReentrancyGuard {
    fn drop(&mut self) {
        let _ = IN_ALLOCATOR.try_with(|f| f.set(false));
    }
}

/// The Global Managers state.
/// Holds the SizeClassManager for each size class.
pub struct GlobalManagers {
    pub sc16: SizeClassManager<'static, SC16, SyscallPageAlloc, 16, N16>,
    pub sc32: SizeClassManager<'static, SC32, SyscallPageAlloc, 32, N32>,
    pub sc64: SizeClassManager<'static, SC64, SyscallPageAlloc, 64, N64>,
    pub sc128: SizeClassManager<'static, SC128, SyscallPageAlloc, 128, N128>,
    pub sc256: SizeClassManager<'static, SC256, SyscallPageAlloc, 256, N256>,
    pub sc512: SizeClassManager<'static, SC512, SyscallPageAlloc, 512, N512>,
    pub sc1024: SizeClassManager<'static, SC1024, SyscallPageAlloc, 1024, N1024>,
    pub sc2048: SizeClassManager<'static, SC2048, SyscallPageAlloc, 2048, N2048>,
}

impl GlobalManagers {
    pub const fn new() -> Self {
        Self {
            sc16: SizeClassManager::new(),
            sc32: SizeClassManager::new(),
            sc64: SizeClassManager::new(),
            sc128: SizeClassManager::new(),
            sc256: SizeClassManager::new(),
            sc512: SizeClassManager::new(),
            sc1024: SizeClassManager::new(),
            sc2048: SizeClassManager::new(),
        }
    }
}

pub static MANAGERS: GlobalManagers = GlobalManagers::new();

/// The Halo Global Allocator.
///
/// Implements `GlobalAlloc` using `SizeClassManager`s and `SyscallPageAlloc`.
/// Uses thread-local caching for high performance.
pub struct HaloAllocator;

unsafe impl GlobalAlloc for HaloAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(layout.align());
        let token = static_token();

        // Recursion guard
        let _guard = match ReentrancyGuard::enter() {
            Some(g) => g,
            None => {
                // We are recursing. Bypass TLS and allocate directly from manager.
                if size <= 16 { return MANAGERS.sc16.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 32 { return MANAGERS.sc32.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 64 { return MANAGERS.sc64.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 128 { return MANAGERS.sc128.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 256 { return MANAGERS.sc256.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 512 { return MANAGERS.sc512.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 1024 { return MANAGERS.sc1024.alloc(token).unwrap_or(ptr::null_mut()); }
                else if size <= 2048 { return MANAGERS.sc2048.alloc(token).unwrap_or(ptr::null_mut()); }
                else {
                    let new_size = size.next_power_of_two().max(4096);
                    return crate::allocator::syscall::allocate_region(new_size).unwrap_or(ptr::null_mut());
                }
            }
        };

        macro_rules! alloc_fast {
            ($field:ident, $manager:expr, $fill:expr) => {
                // Use try_with to handle TLS init/destruction gracefully
                let cache_res = CACHES.try_with(|caches| {
                    let mut cache = caches.$field.borrow_mut();
                    if let Some(ptr) = cache.pop() {
                        METRICS.on_alloc(size);
                        return ptr;
                    }
                    cache.fill(&$manager, token, $fill);
                    let ptr = cache.pop().unwrap_or(ptr::null_mut());
                    if !ptr.is_null() {
                        METRICS.on_alloc(size);
                    }
                    ptr
                });

                if let Ok(ptr) = cache_res {
                    return ptr;
                }
            };
        }

        if size <= 16 {
            alloc_fast!(sc16, MANAGERS.sc16, 16);
            return MANAGERS.sc16.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 32 {
            alloc_fast!(sc32, MANAGERS.sc32, 16);
            return MANAGERS.sc32.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 64 {
            alloc_fast!(sc64, MANAGERS.sc64, 16);
            return MANAGERS.sc64.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 128 {
            alloc_fast!(sc128, MANAGERS.sc128, 16);
            return MANAGERS.sc128.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 256 {
            alloc_fast!(sc256, MANAGERS.sc256, 8);
            return MANAGERS.sc256.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 512 {
            alloc_fast!(sc512, MANAGERS.sc512, 8);
            return MANAGERS.sc512.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 1024 {
            alloc_fast!(sc1024, MANAGERS.sc1024, 4);
            return MANAGERS.sc1024.alloc(token).unwrap_or(ptr::null_mut());
        } else if size <= 2048 {
            alloc_fast!(sc2048, MANAGERS.sc2048, 2);
            return MANAGERS.sc2048.alloc(token).unwrap_or(ptr::null_mut());
        } else {
            // Large allocation
            // Align up to 4KB
            let new_size = size.next_power_of_two().max(4096);
            let ptr = crate::allocator::syscall::allocate_region(new_size);
            if let Some(p) = ptr {
                METRICS.on_alloc(size);
                p
            } else {
                ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(layout.align());
        let token = static_token();

        METRICS.on_dealloc(size);

        let _guard = match ReentrancyGuard::enter() {
             Some(g) => g,
             None => {
                 // Recursing during deallocation (unlikely unless dropping thread local?)
                 if size <= 16 { return MANAGERS.sc16.free(token, ptr); }
                 else if size <= 32 { return MANAGERS.sc32.free(token, ptr); }
                 else if size <= 64 { return MANAGERS.sc64.free(token, ptr); }
                 else if size <= 128 { return MANAGERS.sc128.free(token, ptr); }
                 else if size <= 256 { return MANAGERS.sc256.free(token, ptr); }
                 else if size <= 512 { return MANAGERS.sc512.free(token, ptr); }
                 else if size <= 1024 { return MANAGERS.sc1024.free(token, ptr); }
                 else if size <= 2048 { return MANAGERS.sc2048.free(token, ptr); }
                 else {
                     let new_size = size.next_power_of_two().max(4096);
                     crate::allocator::syscall::free_region(ptr, new_size);
                     return;
                 }
             }
        };

        macro_rules! dealloc_fast {
            ($field:ident, $manager:expr) => {
                let res = CACHES.try_with(|caches| {
                    let mut cache = caches.$field.borrow_mut();
                    cache.push(ptr);
                    if cache.len() >= cache.capacity() {
                        cache.flush(&$manager, token);
                    }
                });
                if res.is_ok() {
                    return;
                }
            };
        }

        if size <= 16 {
            dealloc_fast!(sc16, MANAGERS.sc16);
            return MANAGERS.sc16.free(token, ptr);
        } else if size <= 32 {
            dealloc_fast!(sc32, MANAGERS.sc32);
            return MANAGERS.sc32.free(token, ptr);
        } else if size <= 64 {
            dealloc_fast!(sc64, MANAGERS.sc64);
            return MANAGERS.sc64.free(token, ptr);
        } else if size <= 128 {
            dealloc_fast!(sc128, MANAGERS.sc128);
            return MANAGERS.sc128.free(token, ptr);
        } else if size <= 256 {
            dealloc_fast!(sc256, MANAGERS.sc256);
            return MANAGERS.sc256.free(token, ptr);
        } else if size <= 512 {
            dealloc_fast!(sc512, MANAGERS.sc512);
            return MANAGERS.sc512.free(token, ptr);
        } else if size <= 1024 {
            dealloc_fast!(sc1024, MANAGERS.sc1024);
            return MANAGERS.sc1024.free(token, ptr);
        } else if size <= 2048 {
            dealloc_fast!(sc2048, MANAGERS.sc2048);
            return MANAGERS.sc2048.free(token, ptr);
        } else {
            // Large allocation
            let new_size = size.next_power_of_two().max(4096);
            crate::allocator::syscall::free_region(ptr, new_size);
        }
    }
}
