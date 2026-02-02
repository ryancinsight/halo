use crate::alloc::segregated::manager::ThreadLocalCache;
use super::super::constants::*;
use core::cell::RefCell;

pub struct GlobalCaches {
    pub sc16: RefCell<ThreadLocalCache<'static, SC16>>,
    pub sc32: RefCell<ThreadLocalCache<'static, SC32>>,
    pub sc64: RefCell<ThreadLocalCache<'static, SC64>>,
    pub sc128: RefCell<ThreadLocalCache<'static, SC128>>,
    pub sc256: RefCell<ThreadLocalCache<'static, SC256>>,
    pub sc512: RefCell<ThreadLocalCache<'static, SC512>>,
    pub sc1024: RefCell<ThreadLocalCache<'static, SC1024>>,
    pub sc2048: RefCell<ThreadLocalCache<'static, SC2048>>,
}

thread_local! {
    pub static CACHES: GlobalCaches = GlobalCaches::new();
}

impl GlobalCaches {
    pub fn new() -> Self {
        // Start with 0 capacity to prevent allocation during initialization.
        // Capacity will grow on first use (protected by ReentrancyGuard).
        Self {
            sc16: RefCell::new(ThreadLocalCache::new(0)),
            sc32: RefCell::new(ThreadLocalCache::new(0)),
            sc64: RefCell::new(ThreadLocalCache::new(0)),
            sc128: RefCell::new(ThreadLocalCache::new(0)),
            sc256: RefCell::new(ThreadLocalCache::new(0)),
            sc512: RefCell::new(ThreadLocalCache::new(0)),
            sc1024: RefCell::new(ThreadLocalCache::new(0)),
            sc2048: RefCell::new(ThreadLocalCache::new(0)),
        }
    }
}
