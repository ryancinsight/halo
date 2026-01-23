//! A branded, unbounded, Multi-Producer Single-Consumer (MPSC) channel.
//!
//! Uses `BrandedSlab` for node allocation to improve performance and cache locality.

use crate::alloc::{BrandedSlab, GhostAlloc};
use crate::GhostToken;
use crate::concurrency::CachePadded;
use std::alloc::Layout;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;

/// A node in the channel.
struct Node<T> {
    next: AtomicPtr<Node<T>>,
    value: Option<T>,
}

/// The internal state of the channel.
struct ChannelState<'brand, T> {
    head: CachePadded<AtomicPtr<Node<T>>>,
    tail: CachePadded<AtomicPtr<Node<T>>>,
    slab: BrandedSlab<'brand>,
    closed: AtomicBool,
}

unsafe impl<'brand, T: Send> Send for ChannelState<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for ChannelState<'brand, T> {}

/// Error returned by `Sender::send`.
#[derive(Debug)]
pub enum SendError<T> {
    /// Allocation failed.
    AllocError(T),
    /// The receiver has disconnected.
    Disconnected(T),
}

/// The sending-half of the channel.
pub struct Sender<'brand, T> {
    state: Arc<ChannelState<'brand, T>>,
}

unsafe impl<'brand, T: Send> Send for Sender<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for Sender<'brand, T> {}

impl<'brand, T> Clone for Sender<'brand, T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

/// The receiving-half of the channel.
pub struct Receiver<'brand, T> {
    state: Arc<ChannelState<'brand, T>>,
}

unsafe impl<'brand, T: Send> Send for Receiver<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for Receiver<'brand, T> {}

/// Creates a new unbounded MPSC channel.
pub fn channel<'brand, T>(token: &GhostToken<'brand>) -> (Sender<'brand, T>, Receiver<'brand, T>) {
    let slab = BrandedSlab::new();

    // Allocate dummy node
    let layout = Layout::new::<Node<T>>();
    let ptr = slab
        .allocate(token, layout)
        .expect("Failed to allocate channel initialization node");

    let node_ptr = ptr.as_ptr() as *mut Node<T>;

    unsafe {
        ptr::write(node_ptr, Node {
            next: AtomicPtr::new(ptr::null_mut()),
            value: None,
        });
    }

    let state = Arc::new(ChannelState {
        head: CachePadded::new(AtomicPtr::new(node_ptr)),
        tail: CachePadded::new(AtomicPtr::new(node_ptr)),
        slab,
        closed: AtomicBool::new(false),
    });

    (
        Sender {
            state: state.clone(),
        },
        Receiver { state },
    )
}

impl<'brand, T> Sender<'brand, T> {
    /// Sends a value into the channel.
    pub fn send(&self, token: &GhostToken<'brand>, value: T) -> Result<(), SendError<T>> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(SendError::Disconnected(value));
        }

        let layout = Layout::new::<Node<T>>();
        let ptr = match self.state.slab.allocate(token, layout) {
            Ok(p) => p,
            Err(_) => return Err(SendError::AllocError(value)),
        };

        let node_ptr = ptr.as_ptr() as *mut Node<T>;

        unsafe {
            ptr::write(node_ptr, Node {
                next: AtomicPtr::new(ptr::null_mut()),
                value: Some(value),
            });
        }

        // Push to tail
        let prev = self.state.tail.swap(node_ptr, Ordering::AcqRel);
        unsafe {
            (*prev).next.store(node_ptr, Ordering::Release);
        }

        Ok(())
    }
}

impl<'brand, T> Receiver<'brand, T> {
    /// Attempts to receive a value from the channel.
    ///
    /// Returns `Some(value)` if a value is available, or `None` if the channel is empty.
    ///
    /// Note: This does not block.
    pub fn try_recv(&self, token: &GhostToken<'brand>) -> Option<T> {
        let mut head = self.state.head.load(Ordering::Acquire);

        loop {
            unsafe {
                let next = (*head).next.load(Ordering::Acquire);

                if !next.is_null() {
                    // Try to move head to next
                    match self.state.head.compare_exchange(
                        head,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            // Success. We own the old head and the value in next.
                            let value = (*next).value.take();

                            // Deallocate the old dummy node (head)
                            let layout = Layout::new::<Node<T>>();
                            self.state.slab.deallocate(
                                token,
                                NonNull::new_unchecked(head as *mut u8),
                                layout,
                            );

                            return value;
                        }
                        Err(h) => {
                            // CAS failed, retry
                            head = h;
                        }
                    }
                } else {
                    // Empty
                    return None;
                }
            }
        }
    }
}

impl<'brand, T> Drop for Receiver<'brand, T> {
    fn drop(&mut self) {
        self.state.closed.store(true, Ordering::Release);
    }
}

impl<'brand, T> Drop for ChannelState<'brand, T> {
    fn drop(&mut self) {
        unsafe {
            let mut curr = self.head.load(Ordering::Relaxed);
            while !curr.is_null() {
                let next = (*curr).next.load(Ordering::Relaxed);

                // Drop value if present (it shouldn't be for the head/dummy, but check anyway)
                if let Some(val) = (*curr).value.take() {
                    drop(val);
                }

                // We just need to drop the T.

                curr = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SharedGhostToken;
    use std::thread;

    #[test]
    fn test_mpsc_basic() {
        GhostToken::new(|token| {
            let (tx, rx) = channel(&token);

            assert!(rx.try_recv(&token).is_none());

            tx.send(&token, 1).unwrap();
            tx.send(&token, 2).unwrap();

            assert_eq!(rx.try_recv(&token), Some(1));
            assert_eq!(rx.try_recv(&token), Some(2));
            assert!(rx.try_recv(&token).is_none());
        });
    }

    #[test]
    fn test_mpsc_threaded() {
        GhostToken::new(|token| {
            let shared_token = Arc::new(SharedGhostToken::new(token));
            let (tx, rx) = channel(&shared_token.read());

            let tx = Arc::new(tx);

            thread::scope(|s| {
                for i in 0..10 {
                    let tx = tx.clone();
                    let st = shared_token.clone();
                    s.spawn(move || {
                        let guard = st.read();
                        tx.send(&guard, i).unwrap();
                    });
                }

                let mut sum = 0;
                let guard = shared_token.read();
                for _ in 0..10 {
                    loop {
                        if let Some(val) = rx.try_recv(&guard) {
                            sum += val;
                            break;
                        }
                        thread::yield_now();
                    }
                }
                assert_eq!(sum, 45);
            });
        });
    }

    #[test]
    fn test_mpsc_disconnected() {
        GhostToken::new(|token| {
            let (tx, rx) = channel::<i32>(&token);
            drop(rx);
            match tx.send(&token, 1) {
                Err(SendError::Disconnected(1)) => (),
                _ => panic!("Expected Disconnected"),
            }
        });
    }
}
