use std::sync::atomic::{AtomicBool, Ordering};
use std::cell::UnsafeCell;
use std::ptr::NonNull;
use std::thread::{self, Thread};
use std::marker::PhantomPinned;

/// A node in the intrusive wait queue.
/// Must be pinned to the stack of the waiting thread.
pub struct WaitNode {
    thread: Thread,
    next: Option<NonNull<WaitNode>>,
    _pin: PhantomPinned,
}

impl WaitNode {
    pub fn new() -> Self {
        Self {
            thread: thread::current(),
            next: None,
            _pin: PhantomPinned,
        }
    }

    pub fn wake(&self) {
        self.thread.unpark();
    }
}

/// A FIFO queue of waiting threads.
///
/// Uses a simple spinlock to protect the linked list operations.
/// Since operations are just pointer swaps, contention is minimal.
pub struct WaitQueue {
    head: UnsafeCell<Option<NonNull<WaitNode>>>,
    tail: UnsafeCell<Option<NonNull<WaitNode>>>,
    lock: AtomicBool,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            head: UnsafeCell::new(None),
            tail: UnsafeCell::new(None),
            lock: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) {
        while self.lock.swap(true, Ordering::Acquire) {
             std::hint::spin_loop();
        }
    }

    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }

    /// Adds a node to the back of the queue.
    ///
    /// # Safety
    /// The `node` must be valid and pinned to the stack for as long as it is in the queue.
    pub unsafe fn push(&self, node: NonNull<WaitNode>) {
        self.lock();
        self.push_locked(node);
        self.unlock();
    }

    /// Adds a node to the back of the queue (caller must hold lock).
    ///
    /// # Safety
    /// Caller must hold the lock. Node must be valid/pinned.
    pub unsafe fn push_locked(&self, node: NonNull<WaitNode>) {
        let tail_ptr = self.tail.get();
        let head_ptr = self.head.get();

        // Ensure new node's next is cleared
        (*node.as_ptr()).next = None;

        if let Some(mut t) = *tail_ptr {
            t.as_mut().next = Some(node);
            *tail_ptr = Some(node);
        } else {
            *head_ptr = Some(node);
            *tail_ptr = Some(node);
        }
    }

    /// Removes and returns the head node.
    pub fn pop(&self) -> Option<NonNull<WaitNode>> {
        self.lock();
        let ret = unsafe { self.pop_locked() };
        self.unlock();
        ret
    }

    /// Removes and returns the head node (caller must hold lock).
    pub unsafe fn pop_locked(&self) -> Option<NonNull<WaitNode>> {
        let head_ptr = self.head.get();
        let tail_ptr = self.tail.get();

        let ret = *head_ptr;
        if let Some(h) = ret {
             *head_ptr = unsafe { h.as_ref().next };
             if (*head_ptr).is_none() {
                 *tail_ptr = None;
             }
        }
        ret
    }

    /// Checks if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.lock();
        let empty = unsafe { (*self.head.get()).is_none() };
        self.unlock();
        empty
    }
}

unsafe impl Sync for WaitQueue {}
unsafe impl Send for WaitQueue {}
