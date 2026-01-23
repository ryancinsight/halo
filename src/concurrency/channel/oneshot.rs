//! A branded oneshot channel.
//!
//! Designed for single-producer, single-consumer communication of a single value.

use crate::GhostToken;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

const STATE_EMPTY: u8 = 0;
const STATE_READY: u8 = 1;
const STATE_DISCONNECTED: u8 = 2;
const STATE_CONSUMED: u8 = 3;

/// The internal state of the channel.
struct ChannelState<'brand, T> {
    data: UnsafeCell<MaybeUninit<T>>,
    state: AtomicU8,
    _brand: PhantomData<&'brand ()>,
}

unsafe impl<'brand, T: Send> Send for ChannelState<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for ChannelState<'brand, T> {}

/// The sending-half of the oneshot channel.
pub struct Sender<'brand, T> {
    state: Arc<ChannelState<'brand, T>>,
}

unsafe impl<'brand, T: Send> Send for Sender<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for Sender<'brand, T> {}

/// The receiving-half of the oneshot channel.
pub struct Receiver<'brand, T> {
    state: Arc<ChannelState<'brand, T>>,
}

unsafe impl<'brand, T: Send> Send for Receiver<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for Receiver<'brand, T> {}

/// Error returned by `Receiver::recv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;

/// Error returned by `Receiver::try_recv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// The channel is empty.
    Empty,
    /// The sender has disconnected.
    Disconnected,
}

/// Creates a new branded oneshot channel.
pub fn channel<'brand, T>(_token: &GhostToken<'brand>) -> (Sender<'brand, T>, Receiver<'brand, T>) {
    let state = Arc::new(ChannelState {
        data: UnsafeCell::new(MaybeUninit::uninit()),
        state: AtomicU8::new(STATE_EMPTY),
        _brand: PhantomData,
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
    ///
    /// Consumes the sender.
    pub fn send(self, _token: &GhostToken<'brand>, value: T) -> Result<(), T> {
        // Safety: We have exclusive ownership of Sender.
        unsafe {
            (*self.state.data.get()).write(value);
        }

        // Transition to READY
        // We use compare_exchange to ensure we don't overwrite if the receiver has dropped (STATE_CONSUMED).
        match self.state.state.compare_exchange(
            STATE_EMPTY,
            STATE_READY,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                // Success. Prevent Drop from setting Disconnected.
                std::mem::forget(self);
                Ok(())
            }
            Err(_) => {
                // Failed to send (Receiver likely dropped).
                // We must retrieve the value to return it.
                let value = unsafe { (*self.state.data.get()).assume_init_read() };
                Err(value)
            }
        }
    }
}

impl<'brand, T> Drop for Sender<'brand, T> {
    fn drop(&mut self) {
        // If we are dropping and haven't sent (state is EMPTY), mark as DISCONNECTED.
        let _ = self.state.state.compare_exchange(
            STATE_EMPTY,
            STATE_DISCONNECTED,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }
}

impl<'brand, T> Receiver<'brand, T> {
    /// Waits for a value to be received.
    ///
    /// Consumes the receiver.
    pub fn recv(self, _token: &GhostToken<'brand>) -> Result<T, RecvError> {
        loop {
            let s = self.state.state.load(Ordering::Acquire);
            match s {
                STATE_READY => {
                    // Mark as consumed to prevent Drop from freeing it again
                    self.state.state.store(STATE_CONSUMED, Ordering::Relaxed);
                    let val = unsafe { (*self.state.data.get()).assume_init_read() };
                    // Prevent Drop from running logic
                    std::mem::forget(self);
                    return Ok(val);
                }
                STATE_DISCONNECTED => return Err(RecvError),
                STATE_EMPTY => std::thread::yield_now(), // Spin/yield
                _ => return Err(RecvError),
            }
        }
    }

    /// Attempts to receive a value without blocking.
    pub fn try_recv(self, _token: &GhostToken<'brand>) -> Result<T, TryRecvError> {
        let s = self.state.state.load(Ordering::Acquire);
        match s {
            STATE_READY => {
                self.state.state.store(STATE_CONSUMED, Ordering::Relaxed);
                let val = unsafe { (*self.state.data.get()).assume_init_read() };
                std::mem::forget(self);
                Ok(val)
            }
            STATE_DISCONNECTED => Err(TryRecvError::Disconnected),
            STATE_EMPTY => Err(TryRecvError::Empty),
            _ => Err(TryRecvError::Disconnected),
        }
    }
}

impl<'brand, T> Drop for Receiver<'brand, T> {
    fn drop(&mut self) {
        // If dropped while READY, we own the data and must drop it.
        // We atomic swap to CONSUMED to ensure we claim it.
        let s = self.state.state.swap(STATE_CONSUMED, Ordering::Acquire);
        if s == STATE_READY {
            unsafe {
                let ptr = self.state.data.get();
                (*ptr).assume_init_drop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_oneshot_basic() {
        GhostToken::new(|token| {
            let (tx, rx) = channel(&token);
            tx.send(&token, 42).unwrap();
            assert_eq!(rx.recv(&token), Ok(42));
        });
    }

    #[test]
    fn test_oneshot_threaded() {
        use crate::SharedGhostToken;
        GhostToken::new(|token| {
            let shared = Arc::new(SharedGhostToken::new(token));
            let (tx, rx) = channel(&shared.read());

            let st = shared.clone();
            thread::scope(|s| {
                s.spawn(move || {
                    let guard = st.read();
                    tx.send(&guard, 99).unwrap();
                });

                let guard = shared.read();
                assert_eq!(rx.recv(&guard), Ok(99));
            });
        });
    }

    #[test]
    fn test_sender_drop_disconnect() {
        GhostToken::new(|token| {
            let (tx, rx) = channel::<i32>(&token);
            drop(tx);
            assert_eq!(rx.recv(&token), Err(RecvError));
        });
    }

    #[test]
    fn test_receiver_drop_cleanup() {
        GhostToken::new(|token| {
            let (tx, rx) = channel(&token);
            tx.send(&token, vec![1, 2, 3]).unwrap();
            drop(rx); // Should drop the vector without leaks
        });
    }

    #[test]
    fn test_send_fail_when_receiver_dropped() {
        GhostToken::new(|token| {
            let (tx, rx) = channel::<i32>(&token);
            drop(rx);
            match tx.send(&token, 100) {
                Err(val) => assert_eq!(val, 100),
                Ok(_) => panic!("Should have failed to send"),
            }
        });
    }
}
