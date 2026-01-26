use core::sync::atomic::{AtomicUsize, Ordering};

pub struct AllocatorMetrics {
    pub allocated_bytes: AtomicUsize,
    pub allocated_count: AtomicUsize,
    pub deallocated_bytes: AtomicUsize,
    pub deallocated_count: AtomicUsize,
}

pub static METRICS: AllocatorMetrics = AllocatorMetrics {
    allocated_bytes: AtomicUsize::new(0),
    allocated_count: AtomicUsize::new(0),
    deallocated_bytes: AtomicUsize::new(0),
    deallocated_count: AtomicUsize::new(0),
};

impl AllocatorMetrics {
    #[inline(always)]
    pub fn on_alloc(&self, size: usize) {
        self.allocated_count.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn on_dealloc(&self, size: usize) {
        self.deallocated_count.fetch_add(1, Ordering::Relaxed);
        self.deallocated_bytes.fetch_add(size, Ordering::Relaxed);
    }
}
