use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::Ordering;
use core::ptr::{self, NonNull};
use core::alloc::Layout;
use std::alloc::{alloc, dealloc, handle_alloc_error};
use crate::concurrency::atomic::GhostAtomicUsize;

/// A slot in the ring buffer.
struct Slot<T> {
    sequence: GhostAtomicUsize<'static>,
    data: UnsafeCell<MaybeUninit<T>>,
}

/// A lock-free, bounded, Multi-Producer Multi-Consumer (MPMC) queue.
///
/// Based on Dmitry Vyukov's bounded MPMC queue.
///
/// # Safety
/// - `T` must be `Send` because it is moved between threads.
pub struct GhostRingBuffer<'brand, T> {
    buffer: NonNull<Slot<T>>,
    mask: usize,
    head: GhostAtomicUsize<'brand>,
    tail: GhostAtomicUsize<'brand>,
}

unsafe impl<'brand, T: Send> Send for GhostRingBuffer<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostRingBuffer<'brand, T> {}

impl<'brand, T> GhostRingBuffer<'brand, T> {
    /// Creates a new ring buffer with the specified capacity.
    ///
    /// The capacity will be rounded up to the next power of two.
    pub fn new(capacity: usize) -> Self {
        if capacity == 0 {
            panic!("Capacity must be greater than 0");
        }

        // Round up to power of 2
        let capacity = capacity.next_power_of_two();
        let mask = capacity - 1;

        // Allocate buffer
        let layout = Layout::array::<Slot<T>>(capacity).unwrap();
        // Safety: layout is valid
        let ptr = unsafe { alloc(layout) } as *mut Slot<T>;

        if ptr.is_null() {
            handle_alloc_error(layout);
        }

        let buffer = NonNull::new(ptr).unwrap();

        // Initialize slots
        for i in 0..capacity {
            unsafe {
                let slot = ptr.add(i);
                // The sequence number is initialized to `i` for the first round.
                // This means the slot is ready for the first write (tail = i).
                ptr::write(&mut (*slot).sequence, GhostAtomicUsize::new(i));
                // data is MaybeUninit, no init needed
            }
        }

        Self {
            buffer,
            mask,
            head: GhostAtomicUsize::new(0),
            tail: GhostAtomicUsize::new(0),
        }
    }

    /// Pushes an item into the queue.
    ///
    /// Returns `Ok(())` if successful, or `Err(item)` if the queue is full.
    pub fn push(&self, item: T) -> Result<(), T> {
        let mut backoff = 0;
        loop {
            let tail = self.tail.load(Ordering::Relaxed);
            let idx = tail & self.mask;

            let slot = unsafe { self.buffer.as_ptr().add(idx) };
            let seq = unsafe { (*slot).sequence.load(Ordering::Acquire) };

            let diff = (seq as isize).wrapping_sub(tail as isize);

            if diff == 0 {
                // Slot is ready for writing.
                // Try to claim the slot by incrementing tail.
                if self.tail.compare_exchange_weak_cas(tail, tail.wrapping_add(1)).is_ok() {
                    // Success, we claimed the slot.
                    unsafe {
                        (*slot).data.get().write(MaybeUninit::new(item));
                        // Set sequence to tail + 1 to indicate it's ready for reading.
                        (*slot).sequence.store(tail.wrapping_add(1), Ordering::Release);
                    }
                    return Ok(());
                }
            } else if diff < 0 {
                // Queue is full (seq < tail) or we wrapped around wrong?
                // Actually diff < 0 means seq < tail.
                // In Vyukov's algo:
                // if seq == tail: empty, can write.
                // if seq == tail + 1: full, filled by current lap.
                // Wait, logic:
                // Initial: seq = i. tail = 0.
                // i=0: seq=0, tail=0 => diff=0. Write. seq becomes 1.
                // Next lap: tail wraps.

                // If diff < 0, it means the slot sequence is behind the tail.
                // This happens if the queue is full and we wrapped, but the slot hasn't been popped yet.
                // tail has advanced, but slot seq is still from previous lap (or current lap but occupied).
                // Actually, if queue is full, seq should be `tail + 1` (from the write) but we are at `tail`?
                // No, if full, we cannot write.
                return Err(item);
            } else {
                // diff > 0: tail is behind seq.
                // This means we are seeing a slot from the future?
                // Or tail was incremented by another thread but we loaded old tail.
                // Just retry.
                if backoff > 20 {
                     std::thread::yield_now();
                } else {
                     std::hint::spin_loop();
                }
                backoff += 1;
            }
        }
    }

    /// Pops an item from the queue.
    ///
    /// Returns `Some(item)` if successful, or `None` if the queue is empty.
    pub fn pop(&self) -> Option<T> {
        let mut backoff = 0;
        loop {
            let head = self.head.load(Ordering::Relaxed);
            let idx = head & self.mask;

            let slot = unsafe { self.buffer.as_ptr().add(idx) };
            let seq = unsafe { (*slot).sequence.load(Ordering::Acquire) };

            let diff = (seq as isize).wrapping_sub(head.wrapping_add(1) as isize);

            if diff == 0 {
                // Slot is ready for reading (seq == head + 1).
                // Try to claim slot by incrementing head.
                if self.head.compare_exchange_weak_cas(head, head.wrapping_add(1)).is_ok() {
                    // Success.
                    unsafe {
                        let data = (*slot).data.get().read().assume_init();
                        // Set sequence to head + mask + 1 (next lap).
                        (*slot).sequence.store(head.wrapping_add(self.mask).wrapping_add(1), Ordering::Release);
                        return Some(data);
                    }
                }
            } else if diff < 0 {
                // seq < head + 1.
                // Slot is empty (not yet written).
                return None;
            } else {
                // diff > 0. head behind.
                 if backoff > 20 {
                     std::thread::yield_now();
                } else {
                     std::hint::spin_loop();
                }
                backoff += 1;
            }
        }
    }
}

impl<'brand, T> Drop for GhostRingBuffer<'brand, T> {
    fn drop(&mut self) {
        // Drain queue
        while self.pop().is_some() {}

        let capacity = self.mask + 1;
        let layout = Layout::array::<Slot<T>>(capacity).unwrap();
        unsafe {
            // We need to drop the slots themselves if they weren't popped?
            // self.pop() drains valid items.
            // Remaining items: none should be remaining if we drained.
            // But we need to deallocate the buffer.
            dealloc(self.buffer.as_ptr() as *mut u8, layout);
        }
    }
}
