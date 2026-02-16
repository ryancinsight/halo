//! Branded MPSC and Oneshot channels.
//!
//! These channels mirror `std::sync::mpsc` and typical oneshot channels, but with
//! "branded" access. This means that sending and receiving operations require
//! a `GhostToken` (shared or exclusive) to prove that the operation is happening
//! within the correct "ghost" context.
//!
//! This can be useful for ensuring that communication only happens between components
//! that are authorized to participate in a specific protocol or session defined by the brand.

use crate::token::traits::GhostBorrow;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex};

// ============================================================================
// MPSC Channel
// ============================================================================

/// Error returned when the channel is disconnected (all senders dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvError;

/// Error returned when sending to a disconnected channel (receiver dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError<T>(pub T);

/// Error returned when try_recv fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// The channel is empty.
    Empty,
    /// The channel is disconnected.
    Disconnected,
}

struct MpscState<T> {
    queue: VecDeque<T>,
    senders: usize,
    receiver_alive: bool,
}

struct MpscShared<T> {
    state: Mutex<MpscState<T>>,
    condvar: Condvar,
}

/// The sending half of a branded MPSC channel.
pub struct GhostSender<'brand, T> {
    shared: Arc<MpscShared<T>>,
    _marker: PhantomData<&'brand ()>,
}

/// The receiving half of a branded MPSC channel.
pub struct GhostReceiver<'brand, T> {
    shared: Arc<MpscShared<T>>,
    _marker: PhantomData<&'brand ()>,
}

unsafe impl<'brand, T: Send> Send for GhostSender<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostSender<'brand, T> {}
unsafe impl<'brand, T: Send> Send for GhostReceiver<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostReceiver<'brand, T> {}

/// Creates a new branded asynchronous (unbounded) channel, returning the sender/receiver halves.
pub fn ghost_channel<'brand, T>() -> (GhostSender<'brand, T>, GhostReceiver<'brand, T>) {
    let shared = Arc::new(MpscShared {
        state: Mutex::new(MpscState {
            queue: VecDeque::new(),
            senders: 1,
            receiver_alive: true,
        }),
        condvar: Condvar::new(),
    });

    (
        GhostSender {
            shared: shared.clone(),
            _marker: PhantomData,
        },
        GhostReceiver {
            shared,
            _marker: PhantomData,
        },
    )
}

impl<'brand, T> Clone for GhostSender<'brand, T> {
    fn clone(&self) -> Self {
        let mut state = self.shared.state.lock().unwrap();
        state.senders += 1;
        drop(state);

        Self {
            shared: self.shared.clone(),
            _marker: PhantomData,
        }
    }
}

impl<'brand, T> Drop for GhostSender<'brand, T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.senders -= 1;
        if state.senders == 0 {
            self.shared.condvar.notify_all();
        }
    }
}

impl<'brand, T> Drop for GhostReceiver<'brand, T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.receiver_alive = false;
        // No need to notify senders as sending is non-blocking (unless buffer full, but this is unbounded).
        // SendError check handles it.
    }
}

impl<'brand, T> GhostSender<'brand, T> {
    /// Sends a value on this channel.
    ///
    /// This function will never block the current thread, as the channel is unbounded.
    /// However, it requires a token to prove branding context.
    pub fn send(&self, t: T, _token: &impl GhostBorrow<'brand>) -> Result<(), SendError<T>> {
        let mut state = self.shared.state.lock().unwrap();
        if !state.receiver_alive {
            return Err(SendError(t));
        }
        state.queue.push_back(t);
        self.shared.condvar.notify_one();
        Ok(())
    }
}

impl<'brand, T> GhostReceiver<'brand, T> {
    /// Attempts to wait for a value on this receiver, returning an error if the
    /// corresponding channel has hung up.
    ///
    /// This function will block if there is no data available.
    pub fn recv(&self, _token: &impl GhostBorrow<'brand>) -> Result<T, RecvError> {
        let mut state = self.shared.state.lock().unwrap();
        loop {
            if let Some(t) = state.queue.pop_front() {
                return Ok(t);
            }
            if state.senders == 0 {
                return Err(RecvError);
            }
            state = self.shared.condvar.wait(state).unwrap();
        }
    }

    /// Attempts to return a pending value on this receiver without blocking.
    pub fn try_recv(&self, _token: &impl GhostBorrow<'brand>) -> Result<T, TryRecvError> {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(t) = state.queue.pop_front() {
            Ok(t)
        } else if state.senders == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}

// ============================================================================
// Oneshot Channel
// ============================================================================

/// Error returned when the oneshot channel is closed (receiver dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneshotSendError<T>(pub T);

/// Error returned when the oneshot channel is empty (sender dropped without sending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneshotRecvError;

struct OneshotState<T> {
    value: Option<T>,
    sender_alive: bool,
    receiver_alive: bool,
}

struct OneshotShared<T> {
    state: Mutex<OneshotState<T>>,
    condvar: Condvar,
}

/// The sending half of a branded oneshot channel.
pub struct GhostOneshotSender<'brand, T> {
    shared: Arc<OneshotShared<T>>,
    _marker: PhantomData<&'brand ()>,
}

/// The receiving half of a branded oneshot channel.
pub struct GhostOneshotReceiver<'brand, T> {
    shared: Arc<OneshotShared<T>>,
    _marker: PhantomData<&'brand ()>,
}

unsafe impl<'brand, T: Send> Send for GhostOneshotSender<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostOneshotSender<'brand, T> {}
unsafe impl<'brand, T: Send> Send for GhostOneshotReceiver<'brand, T> {}
unsafe impl<'brand, T: Send> Sync for GhostOneshotReceiver<'brand, T> {}

/// Creates a new branded oneshot channel, returning the sender/receiver halves.
pub fn ghost_oneshot<'brand, T>() -> (
    GhostOneshotSender<'brand, T>,
    GhostOneshotReceiver<'brand, T>,
) {
    let shared = Arc::new(OneshotShared {
        state: Mutex::new(OneshotState {
            value: None,
            sender_alive: true,
            receiver_alive: true,
        }),
        condvar: Condvar::new(),
    });

    (
        GhostOneshotSender {
            shared: shared.clone(),
            _marker: PhantomData,
        },
        GhostOneshotReceiver {
            shared,
            _marker: PhantomData,
        },
    )
}

impl<'brand, T> Drop for GhostOneshotSender<'brand, T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.sender_alive = false;
        // Notify receiver if sender dropped without sending
        if state.value.is_none() {
            self.shared.condvar.notify_all();
        }
    }
}

impl<'brand, T> Drop for GhostOneshotReceiver<'brand, T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.receiver_alive = false;
    }
}

impl<'brand, T> GhostOneshotSender<'brand, T> {
    /// Sends a value on this oneshot channel.
    ///
    /// Returns an error if the receiver has already been dropped.
    pub fn send(self, t: T, _token: &impl GhostBorrow<'brand>) -> Result<(), OneshotSendError<T>> {
        let mut state = self.shared.state.lock().unwrap();
        if !state.receiver_alive {
            return Err(OneshotSendError(t));
        }
        state.value = Some(t);
        state.sender_alive = false; // Sender consumed, logically "done"
        self.shared.condvar.notify_one();

        // Prevent Drop from running logic again (though benign, as sender_alive=false already)
        // Actually Drop logic is fine: sender_alive=false, notify_all.
        // But we already notified.
        // It's cleaner to just let Drop run, or use ManuallyDrop logic, but Drop just sets sender_alive=false.
        // If we set it here, Drop will just set it to false again.
        // The only side effect of Drop is notify_all if value is None.
        // Here value is Some. So Drop does nothing important.
        Ok(())
    }
}

impl<'brand, T> GhostOneshotReceiver<'brand, T> {
    /// Attempts to wait for a value on this receiver.
    ///
    /// This function will block if there is no data available.
    pub fn recv(self, _token: &impl GhostBorrow<'brand>) -> Result<T, OneshotRecvError> {
        let mut state = self.shared.state.lock().unwrap();
        loop {
            if let Some(t) = state.value.take() {
                return Ok(t);
            }
            if !state.sender_alive {
                return Err(OneshotRecvError);
            }
            state = self.shared.condvar.wait(state).unwrap();
        }
    }

    /// Attempts to return a pending value on this receiver without blocking.
    pub fn try_recv(&mut self, _token: &impl GhostBorrow<'brand>) -> Result<T, TryRecvError> {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(t) = state.value.take() {
            Ok(t)
        } else if !state.sender_alive {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}
