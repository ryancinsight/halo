use crate::alloc::segregated::manager::SizeClassManager;
use crate::alloc::segregated::size_class::{get_size_class_index, SLAB_CLASS_COUNT};
use crate::alloc::page::{SyscallPageAlloc, PAGE_SIZE};
use crate::token::static_token;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::ptr;
use super::constants::*;
use super::integration::thread_cache::CACHES;
use super::stats::metrics::METRICS;

const FILL_COUNTS: [usize; SLAB_CLASS_COUNT] = [16, 16, 16, 16, 8, 8, 4, 2];

thread_local! {
    static IN_ALLOCATOR: Cell<bool> = const { Cell::new(false) };
}

struct ReentrancyGuard;

impl ReentrancyGuard {
    fn enter() -> Option<Self> {
        let in_alloc = IN_ALLOCATOR.with(|f| f.get());
        if in_alloc {
            None
        } else {
            IN_ALLOCATOR.with(|f| f.set(true));
            Some(Self)
        }
    }
}

impl Drop for ReentrancyGuard {
    fn drop(&mut self) {
        IN_ALLOCATOR.with(|f| f.set(false));
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
        let class_idx = get_size_class_index(size);

        let _guard = match ReentrancyGuard::enter() {
            Some(g) => g,
            None => {
                match class_idx {
                    Some(0) => return MANAGERS.sc16.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(1) => return MANAGERS.sc32.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(2) => return MANAGERS.sc64.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(3) => return MANAGERS.sc128.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(4) => return MANAGERS.sc256.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(5) => return MANAGERS.sc512.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(6) => return MANAGERS.sc1024.alloc(token).unwrap_or(ptr::null_mut()),
                    Some(7) => return MANAGERS.sc2048.alloc(token).unwrap_or(ptr::null_mut()),
                    _ => {
                        let new_size = size.next_power_of_two().max(PAGE_SIZE);
                        return super::syscall::allocate_region(new_size).unwrap_or(ptr::null_mut());
                    }
                }
            }
        };

        macro_rules! alloc_fast {
            ($field:ident, $manager:expr, $fill:expr) => {
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

        match class_idx {
            Some(0) => {
                alloc_fast!(sc16, MANAGERS.sc16, FILL_COUNTS[0]);
                return MANAGERS.sc16.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(1) => {
                alloc_fast!(sc32, MANAGERS.sc32, FILL_COUNTS[1]);
                return MANAGERS.sc32.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(2) => {
                alloc_fast!(sc64, MANAGERS.sc64, FILL_COUNTS[2]);
                return MANAGERS.sc64.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(3) => {
                alloc_fast!(sc128, MANAGERS.sc128, FILL_COUNTS[3]);
                return MANAGERS.sc128.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(4) => {
                alloc_fast!(sc256, MANAGERS.sc256, FILL_COUNTS[4]);
                return MANAGERS.sc256.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(5) => {
                alloc_fast!(sc512, MANAGERS.sc512, FILL_COUNTS[5]);
                return MANAGERS.sc512.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(6) => {
                alloc_fast!(sc1024, MANAGERS.sc1024, FILL_COUNTS[6]);
                return MANAGERS.sc1024.alloc(token).unwrap_or(ptr::null_mut());
            }
            Some(7) => {
                alloc_fast!(sc2048, MANAGERS.sc2048, FILL_COUNTS[7]);
                return MANAGERS.sc2048.alloc(token).unwrap_or(ptr::null_mut());
            }
            _ => {
                let new_size = size.next_power_of_two().max(PAGE_SIZE);
                let ptr = super::syscall::allocate_region(new_size);
                if let Some(p) = ptr {
                    METRICS.on_alloc(size);
                    p
                } else {
                    ptr::null_mut()
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(layout.align());
        let token = static_token();
        let class_idx = get_size_class_index(size);

        METRICS.on_dealloc(size);

        let _guard = match ReentrancyGuard::enter() {
            Some(g) => g,
            None => {
                match class_idx {
                    Some(0) => return MANAGERS.sc16.free(token, ptr),
                    Some(1) => return MANAGERS.sc32.free(token, ptr),
                    Some(2) => return MANAGERS.sc64.free(token, ptr),
                    Some(3) => return MANAGERS.sc128.free(token, ptr),
                    Some(4) => return MANAGERS.sc256.free(token, ptr),
                    Some(5) => return MANAGERS.sc512.free(token, ptr),
                    Some(6) => return MANAGERS.sc1024.free(token, ptr),
                    Some(7) => return MANAGERS.sc2048.free(token, ptr),
                    _ => {
                        let new_size = size.next_power_of_two().max(PAGE_SIZE);
                        super::syscall::free_region(ptr, new_size);
                        return;
                    }
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

        match class_idx {
            Some(0) => {
                dealloc_fast!(sc16, MANAGERS.sc16);
                return MANAGERS.sc16.free(token, ptr);
            }
            Some(1) => {
                dealloc_fast!(sc32, MANAGERS.sc32);
                return MANAGERS.sc32.free(token, ptr);
            }
            Some(2) => {
                dealloc_fast!(sc64, MANAGERS.sc64);
                return MANAGERS.sc64.free(token, ptr);
            }
            Some(3) => {
                dealloc_fast!(sc128, MANAGERS.sc128);
                return MANAGERS.sc128.free(token, ptr);
            }
            Some(4) => {
                dealloc_fast!(sc256, MANAGERS.sc256);
                return MANAGERS.sc256.free(token, ptr);
            }
            Some(5) => {
                dealloc_fast!(sc512, MANAGERS.sc512);
                return MANAGERS.sc512.free(token, ptr);
            }
            Some(6) => {
                dealloc_fast!(sc1024, MANAGERS.sc1024);
                return MANAGERS.sc1024.free(token, ptr);
            }
            Some(7) => {
                dealloc_fast!(sc2048, MANAGERS.sc2048);
                return MANAGERS.sc2048.free(token, ptr);
            }
            _ => {
                let new_size = size.next_power_of_two().max(PAGE_SIZE);
                super::syscall::free_region(ptr, new_size);
            }
        }
    }
}
