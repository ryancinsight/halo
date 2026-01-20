//! A lock-free, bounded Multi-Producer Multi-Consumer (MPMC) queue.
//!
//! Based on Dmitry Vyukov's bounded MPMC queue.
//!
//! # Invariants
//!
//! - `head`: The index of the next element to pop.
//! - `tail`: The index of the next slot to push into.
//! - `buffer`: A power-of-two sized buffer of slots.
//! - `slot.sequence`:
//!   - Initialized to `index` for `buffer[index]`.
//!   - On push: `sequence` must equal `tail`. Set to `tail + 1` after write.
//!   - On pop: `sequence` must equal `head + 1`. Set to `head + mask + 1` after read.

use crate::concurrency::atomic::GhostAtomicUsize;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};

/// A slot in the ring buffer.
///
/// Aligned to 64 bytes to prevent false sharing between adjacent slots.
#[repr(C)]
#[repr(align(64))]
struct Slot<'brand, T> {
    sequence: GhostAtomicUsize<'brand>,
    data: UnsafeCell<MaybeUninit<T>>,
}

/// A lock-free, bounded MPMC queue branded with a ghost token lifetime.
///
/// Uses manual memory management and explicit alignment.
#[repr(C)]
#[repr(align(128))] // Align to 128 to ensure cache line separation on most archs
pub struct GhostRingBuffer<'brand, T> {
    head: GhostAtomicUsize<'brand>,
    _pad1: [u8; 120], // 128 - 8 (usize)
    tail: GhostAtomicUsize<'brand>,
    _pad2: [u8; 120], // 128 - 8
    buffer: NonNull<Slot<'brand, T>>,
    mask: usize,
    _marker: core::marker::PhantomData<Slot<'brand, T>>,
}

unsafe impl<'brand, T: Send> Send for GhostRingBuffer<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostRingBuffer<'brand, T> {}

impl<'brand, T> GhostRingBuffer<'brand, T> {
    /// Creates a new `GhostRingBuffer` with the specified capacity.
    ///
    /// Capacity must be a power of two.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0 && capacity.is_power_of_two(), "Capacity must be a power of two");

        // Calculate layout for the buffer
        let layout = Layout::array::<Slot<'brand, T>>(capacity).expect("Capacity overflow");

        // Allocate memory
        // SAFETY: Layout is valid.
        let ptr = unsafe { alloc(layout) } as *mut Slot<'brand, T>;

        if ptr.is_null() {
            handle_alloc_error(layout);
        }

        // Initialize slots
        for i in 0..capacity {
            unsafe {
                let slot_ptr = ptr.add(i);
                core::ptr::write(slot_ptr, Slot {
                    sequence: GhostAtomicUsize::new(i),
                    data: UnsafeCell::new(MaybeUninit::uninit()),
                });
            }
        }

        Self {
            head: GhostAtomicUsize::new(0),
            tail: GhostAtomicUsize::new(0),
            buffer: unsafe { NonNull::new_unchecked(ptr) },
            mask: capacity - 1,
            _pad1: [0; 120],
            _pad2: [0; 120],
            _marker: core::marker::PhantomData,
        }
    }

    /// Attempts to push an element into the queue.
    ///
    /// Returns `Ok(())` if successful, or `Err(T)` if the queue is full.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let mut value = value;
        let mask = self.mask;
        loop {
            let tail = self.tail.load(Ordering::Relaxed);
            let idx = tail & mask;
            let slot = unsafe { self.buffer.as_ptr().add(idx).as_ref().unwrap() };
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = (seq as isize).wrapping_sub(tail as isize);

            if diff == 0 {
                // Slot is empty and ready for this tail index
                match self.tail.compare_exchange_weak(
                    tail,
                    tail + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Success, we claimed the slot
                        unsafe {
                            slot.data.get().write(MaybeUninit::new(value));
                        }
                        slot.sequence.store(tail + 1, Ordering::Release);
                        return Ok(());
                    }
                    Err(_) => {
                        // Lost the race, retry
                        continue;
                    }
                }
            } else if diff < 0 {
                // Queue is full
                return Err(value);
            } else {
                // Tail is stale
                continue;
            }
        }
    }

    /// Attempts to pop an element from the queue.
    ///
    /// Returns `Some(T)` if successful, or `None` if the queue is empty.
    pub fn try_pop(&self) -> Option<T> {
        let mask = self.mask;
        loop {
            let head = self.head.load(Ordering::Relaxed);
            let idx = head & mask;
            let slot = unsafe { self.buffer.as_ptr().add(idx).as_ref().unwrap() };
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = (seq as isize).wrapping_sub((head + 1) as isize);

            if diff == 0 {
                // Slot has data and is ready for this head index
                match self.head.compare_exchange_weak(
                    head,
                    head + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // Success, we claimed the item
                        let value = unsafe {
                            slot.data.get().read().assume_init()
                        };
                        slot.sequence.store(head + mask + 1, Ordering::Release);
                        return Some(value);
                    }
                    Err(_) => {
                        continue;
                    }
                }
            } else if diff < 0 {
                // Queue is empty
                return None;
            } else {
                // Head is stale
                continue;
            }
        }
    }
}

impl<'brand, T> Drop for GhostRingBuffer<'brand, T> {
    fn drop(&mut self) {
        // Drain the queue to drop remaining elements
        while self.try_pop().is_some() {}

        // Deallocate buffer
        let capacity = self.mask + 1;
        let layout = Layout::array::<Slot<'brand, T>>(capacity).expect("Layout error");
        unsafe {
            dealloc(self.buffer.as_ptr() as *mut u8, layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_simple_push_pop() {
        GhostToken::new(|_token| {
            let q = GhostRingBuffer::new(2);
            assert!(q.try_push(1).is_ok());
            assert!(q.try_push(2).is_ok());
            assert!(q.try_push(3).is_err());
            assert_eq!(q.try_pop(), Some(1));
            assert_eq!(q.try_pop(), Some(2));
            assert_eq!(q.try_pop(), None);
        });
    }

    #[test]
    fn test_mpmc() {
        GhostToken::new(|_token| {
            let q = Arc::new(GhostRingBuffer::new(64));

            thread::scope(|s| {
                let q1 = q.clone();
                let q2 = q.clone();
                let q3 = q.clone();

                s.spawn(move || {
                    for i in 0..1000 {
                        while q1.try_push(i).is_err() {
                            thread::yield_now();
                        }
                    }
                });

                s.spawn(move || {
                    for i in 1000..2000 {
                        while q2.try_push(i).is_err() {
                            thread::yield_now();
                        }
                    }
                });

                s.spawn(move || {
                    let mut received = Vec::new();
                    while received.len() < 2000 {
                        if let Some(v) = q3.try_pop() {
                            received.push(v);
                        } else {
                            thread::yield_now();
                        }
                    }
                    received.sort();
                    let expected: Vec<_> = (0..2000).collect();
                    assert_eq!(received, expected);
                });
            });
        });
    }
}
