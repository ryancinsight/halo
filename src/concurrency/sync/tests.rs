use super::*;
use crate::token::GhostToken;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_ghost_mutex_lock() {
    GhostToken::new(|token| {
        let mutex = GhostMutex::new(token);

        {
            let guard = mutex.lock();
            // We have the token!
            // Let's just check that we can access it.
            let _token_ref = &*guard;
        } // guard dropped, unlocked

        assert!(mutex.try_lock().is_some());
    });
}

#[test]
fn test_ghost_mutex_contention() {
    GhostToken::new(|token| {
        let mutex = GhostMutex::new(token);
        let mutex = &mutex;

        thread::scope(|s| {
            s.spawn(move || {
                let guard = mutex.lock();
                thread::sleep(Duration::from_millis(50));
                drop(guard);
            });

            s.spawn(move || {
                thread::sleep(Duration::from_millis(10));
                // Should block until first thread releases
                let guard = mutex.lock();
                drop(guard);
            });
        });
    });
}

#[test]
fn test_ghost_condvar() {
    GhostToken::new(|token| {
        let mutex = GhostMutex::new(token);
        let condvar = GhostCondvar::new();

        let mutex = &mutex;
        let condvar = &condvar;
        let started = std::sync::atomic::AtomicBool::new(false);
        let started = &started;

        thread::scope(|s| {
            s.spawn(move || {
                let guard = mutex.lock();
                started.store(true, Ordering::SeqCst);
                let _guard = condvar.wait(guard);
                // Woken up!
            });

            s.spawn(move || {
                while !started.load(Ordering::SeqCst) {
                    thread::yield_now();
                }
                thread::sleep(Duration::from_millis(20));
                condvar.notify_one();
            });
        });
    });
}

#[test]
fn test_ghost_barrier() {
    GhostToken::new(|token| {
        let barrier = GhostBarrier::new(2);
        let barrier = &barrier;

        // We need shared tokens.
        let (child1, child2) = token.split_immutable();

        thread::scope(|s| {
            s.spawn(move || {
                barrier.wait(&child1);
            });

            s.spawn(move || {
                barrier.wait(&child2);
            });
        });
    });
}

#[test]
fn test_wait_on_u32_wake_existing() {
    // Porting the existing test from mod.rs
    use std::sync::Barrier;
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
