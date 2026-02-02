//! A lock-free, bounded, Multi-Producer Multi-Consumer (MPMC) queue.
//!
//! Based on Dmitry Vyukov's bounded MPMC queue.
//!
//! This implementation is "branded" with a `'brand` lifetime, tying it to a `GhostToken` context
//! conceptually, though it relies on atomics for synchronization and does not require
//! the token for `try_push`/`try_pop` operations (making it fully concurrent).

use crate::concurrency::atomic::GhostAtomicUsize;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::Ordering;

/// A slot in the ring buffer.
struct Slot<'brand, T> {
    /// The sequence number for this slot.
    /// - `sequence == index`: Slot is empty and ready for enqueue.
    /// - `sequence == index + 1`: Slot is full and ready for dequeue.
    sequence: GhostAtomicUsize<'brand>,
    /// The data.
    data: UnsafeCell<MaybeUninit<T>>,
}

/// A lock-free, bounded MPMC queue.
#[repr(C)]
#[repr(align(64))]
pub struct GhostRingBuffer<'brand, T> {
    /// The head index (enqueue position).
    head: GhostAtomicUsize<'brand>,
    /// Padding to prevent false sharing.
    _pad1: [u8; 56],
    /// The tail index (dequeue position).
    tail: GhostAtomicUsize<'brand>,
    /// Padding to prevent false sharing.
    _pad2: [u8; 56],
    /// The buffer.
    buffer: Box<[Slot<'brand, T>]>,
    /// Capacity mask (capacity - 1).
    mask: usize,
}

unsafe impl<'brand, T: Send> Send for GhostRingBuffer<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostRingBuffer<'brand, T> {}

impl<'brand, T> GhostRingBuffer<'brand, T> {
    /// Creates a new ring buffer with the specified capacity.
    ///
    /// Capacity will be rounded up to the next power of two.
    pub fn new(capacity: usize) -> Self {
        let capacity = if capacity < 2 {
            2
        } else {
            capacity.next_power_of_two()
        };
        let mask = capacity - 1;

        let mut buffer = Vec::with_capacity(capacity);
        for i in 0..capacity {
            buffer.push(Slot {
                sequence: GhostAtomicUsize::new(i),
                data: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }

        Self {
            head: GhostAtomicUsize::new(0),
            _pad1: [0; 56],
            tail: GhostAtomicUsize::new(0),
            _pad2: [0; 56],
            buffer: buffer.into_boxed_slice(),
            mask,
        }
    }

    /// Attempts to push an element into the queue.
    ///
    /// Returns `Ok(())` if successful, or `Err(value)` if the queue is full.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let mut head = self.head.load(Ordering::Relaxed);

        loop {
            let index = head & self.mask;
            // SAFETY: index is within bounds (mask = cap - 1)
            let slot = unsafe { self.buffer.get_unchecked(index) };
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = seq.wrapping_sub(head) as isize;

            if diff == 0 {
                // Slot is empty. Try to claim it.
                match self.head.compare_exchange(
                    head,
                    head.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Claimed. Write data.
                        unsafe {
                            (*slot.data.get()).write(value);
                        }
                        // Commit sequence.
                        slot.sequence
                            .store(head.wrapping_add(1), Ordering::Release);
                        return Ok(());
                    }
                    Err(h) => {
                        // CAS failed, retry.
                        head = h;
                    }
                }
            } else if diff < 0 {
                // Full.
                return Err(value);
            } else {
                // Lagging/contention. Reload head.
                head = self.head.load(Ordering::Relaxed);
            }
        }
    }

    /// Attempts to pop an element from the queue.
    ///
    /// Returns `Some(value)` if successful, or `None` if the queue is empty.
    pub fn try_pop(&self) -> Option<T> {
        let mut tail = self.tail.load(Ordering::Relaxed);

        loop {
            let index = tail & self.mask;
            // SAFETY: index is within bounds
            let slot = unsafe { self.buffer.get_unchecked(index) };
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = seq.wrapping_sub(tail.wrapping_add(1)) as isize;

            if diff == 0 {
                // Slot has data. Try to claim it.
                match self.tail.compare_exchange(
                    tail,
                    tail.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Claimed. Read data.
                        let value = unsafe { (*slot.data.get()).assume_init_read() };
                        // Commit sequence.
                        slot.sequence.store(
                            tail.wrapping_add(self.mask).wrapping_add(1),
                            Ordering::Release,
                        );
                        return Some(value);
                    }
                    Err(t) => {
                        // CAS failed, retry.
                        tail = t;
                    }
                }
            } else if diff < 0 {
                // Empty.
                return None;
            } else {
                // Contention.
                tail = self.tail.load(Ordering::Relaxed);
            }
        }
    }

    /// Returns the capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Returns `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        head.wrapping_sub(tail) == 0
    }

    /// Returns `true` if the queue is full.
    pub fn is_full(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        head.wrapping_sub(tail) >= self.capacity()
    }
}

impl<'brand, T> Drop for GhostRingBuffer<'brand, T> {
    fn drop(&mut self) {
        while self.try_pop().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_ring_buffer_basic() {
        // GhostAtomicUsize doesn't strictly require a token instance to run,
        // but we respect the branding.
        GhostToken::new(|_token| {
            let queue = GhostRingBuffer::new(4);
            assert!(queue.is_empty());

            assert!(queue.try_push(1).is_ok());
            assert!(queue.try_push(2).is_ok());
            assert!(queue.try_push(3).is_ok());
            assert!(queue.try_push(4).is_ok());

            assert!(queue.is_full());
            assert_eq!(queue.try_push(5), Err(5));

            assert_eq!(queue.try_pop(), Some(1));
            assert_eq!(queue.try_pop(), Some(2));
            assert_eq!(queue.try_pop(), Some(3));
            assert_eq!(queue.try_pop(), Some(4));
            assert_eq!(queue.try_pop(), None);
            assert!(queue.is_empty());
        });
    }

    #[test]
    fn test_ring_buffer_concurrent() {
        use std::sync::Arc;
        use std::thread;

        GhostToken::new(|_token| {
            let queue = Arc::new(GhostRingBuffer::new(1024));
            let q1 = queue.clone();
            let q2 = queue.clone();

            let p = thread::spawn(move || {
                for i in 0..1000 {
                    while q1.try_push(i).is_err() {
                        thread::yield_now();
                    }
                }
            });

            let c = thread::spawn(move || {
                let mut sum = 0;
                for _ in 0..1000 {
                    loop {
                        if let Some(v) = q2.try_pop() {
                            sum += v;
                            break;
                        }
                        thread::yield_now();
                    }
                }
                sum
            });

            p.join().unwrap();
            let sum = c.join().unwrap();
            assert_eq!(sum, (0..1000).sum::<i32>());
        });
    }
}
