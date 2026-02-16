pub mod ghost_once_lock;
pub mod mpmc;

pub use ghost_once_lock::GhostOnceLock;
pub use mpmc::GhostRingBuffer;

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize};

#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    WaitOnAddress, WakeByAddressAll, WakeByAddressSingle,
};

#[cfg(target_os = "linux")]
use libc::{SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAIT, FUTEX_WAKE};

#[cfg(target_os = "linux")]
#[inline]
fn futex_wait(addr: *const u32, expected: u32) {
    unsafe {
        libc::syscall(
            SYS_futex,
            addr,
            FUTEX_WAIT | FUTEX_PRIVATE_FLAG,
            expected,
            core::ptr::null::<libc::timespec>(),
        );
    }
}

#[cfg(target_os = "linux")]
#[inline]
fn futex_wake(addr: *const u32, count: i32) {
    unsafe {
        libc::syscall(SYS_futex, addr, FUTEX_WAKE | FUTEX_PRIVATE_FLAG, count);
    }
}

#[inline]
/// Wakes all threads waiting on the given boolean address.
pub fn wake_all_bool(addr: &AtomicBool) {
    #[cfg(windows)]
    unsafe {
        WakeByAddressAll(addr as *const _ as *mut _);
    }
}

#[inline]
/// Wakes one thread waiting on the given boolean address.
pub fn wake_one_bool(addr: &AtomicBool) {
    #[cfg(windows)]
    unsafe {
        WakeByAddressSingle(addr as *const _ as *mut _);
    }
}

#[inline]
/// Waits on the given boolean address until the value changes from `expected`.
pub fn wait_on_bool(addr: &AtomicBool, expected: bool) {
    #[cfg(windows)]
    unsafe {
        let expected_ptr = &expected as *const bool as *const _;
        let addr_ptr = addr as *const _ as *mut _;
        let size = core::mem::size_of::<bool>();
        WaitOnAddress(addr_ptr, expected_ptr, size, u32::MAX);
        return;
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    while addr.load(Ordering::SeqCst) == expected {
        std::thread::yield_now();
    }
}

/// Wakes all threads waiting on the given address.
#[inline]
pub fn wake_all_usize(addr: &AtomicUsize) {
    #[cfg(windows)]
    unsafe {
        WakeByAddressAll(addr as *const _ as *mut _);
    }
}

/// Wakes one thread waiting on the given address.
#[inline]
pub fn wake_one_usize(addr: &AtomicUsize) {
    #[cfg(windows)]
    unsafe {
        WakeByAddressSingle(addr as *const _ as *mut _);
    }
}

/// Waits on the given address until the value changes from `expected`.
#[inline]
pub fn wait_on_usize(addr: &AtomicUsize, expected: usize) {
    #[cfg(windows)]
    unsafe {
        let expected_ptr = &expected as *const usize as *const _;
        let addr_ptr = addr as *const _ as *mut _;
        let size = core::mem::size_of::<usize>();
        WaitOnAddress(addr_ptr, expected_ptr, size, u32::MAX);
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    while addr.load(Ordering::SeqCst) == expected {
        std::thread::yield_now();
    }
}

/// Wakes all threads waiting on the given address.
#[inline]
pub fn wake_all_u32(addr: &AtomicU32) {
    #[cfg(windows)]
    unsafe {
        WakeByAddressAll(addr as *const _ as *mut _);
    }
    #[cfg(target_os = "linux")]
    {
        futex_wake(addr as *const _ as *const u32, i32::MAX);
    }
}

/// Wakes one thread waiting on the given address.
#[inline]
pub fn wake_one_u32(addr: &AtomicU32) {
    #[cfg(windows)]
    unsafe {
        WakeByAddressSingle(addr as *const _ as *mut _);
    }
    #[cfg(target_os = "linux")]
    {
        futex_wake(addr as *const _ as *const u32, 1);
    }
}

/// Waits on the given address until the value changes from `expected`.
#[inline]
pub fn wait_on_u32(addr: &AtomicU32, expected: u32) {
    #[cfg(windows)]
    unsafe {
        let expected_ptr = &expected as *const u32 as *const _;
        let addr_ptr = addr as *const _ as *mut _;
        let size = core::mem::size_of::<u32>();
        WaitOnAddress(addr_ptr, expected_ptr, size, u32::MAX);
    }
    #[cfg(target_os = "linux")]
    unsafe {
        if addr.load(Ordering::SeqCst) == expected {
            futex_wait(addr as *const _ as *const u32, expected);
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    while addr.load(Ordering::SeqCst) == expected {
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn test_wait_on_u32_wake() {
        let flag = Arc::new(AtomicU32::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let flag_thread = flag.clone();
        let barrier_thread = barrier.clone();

        let handle = thread::spawn(move || {
            barrier_thread.wait();
            wait_on_u32(&flag_thread, 0);
            flag_thread.load(Ordering::SeqCst)
        });

        barrier.wait();
        flag.store(1, Ordering::SeqCst);
        wake_all_u32(&flag);

        let value = handle.join().unwrap();
        assert_eq!(value, 1);
    }
}
