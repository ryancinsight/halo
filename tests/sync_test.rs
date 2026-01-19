use halo::concurrency::sync::{GhostMutex, GhostCondvar};
use halo::GhostToken;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_mutex_basic() {
    GhostToken::new(|token| {
        let mutex = GhostMutex::new(token);
        {
            let guard = mutex.lock();
            assert!(guard.is_valid());
        }
        {
            let guard = mutex.lock();
             assert!(guard.is_valid());
        }
    });
}

#[test]
fn test_mutex_contention() {
    GhostToken::new(|token| {
        let mutex = Arc::new(GhostMutex::new(token));
        let m1 = mutex.clone();
        let m2 = mutex.clone();

        thread::scope(|s| {
            let t1 = s.spawn(move || {
                for _ in 0..100 {
                    let _g = m1.lock();
                }
            });

            let t2 = s.spawn(move || {
                 for _ in 0..100 {
                     let _g = m2.lock();
                 }
            });

            t1.join().unwrap();
            t2.join().unwrap();
        });
    });
}

#[test]
fn test_condvar() {
    GhostToken::new(|token| {
        let mutex = Arc::new(GhostMutex::new(token));
        let cond = Arc::new(GhostCondvar::new());

        let m1 = mutex.clone();
        let c1 = cond.clone();

        thread::scope(|s| {
            let t = s.spawn(move || {
                let guard = m1.lock();
                let _guard = c1.wait(guard);
            });

            thread::sleep(Duration::from_millis(50));
            cond.notify_one();
            t.join().unwrap();
        });
    });
}

#[test]
fn test_condvar_notify_all() {
    GhostToken::new(|token| {
        let mutex = Arc::new(GhostMutex::new(token));
        let cond = Arc::new(GhostCondvar::new());

        thread::scope(|s| {
            let mut handles = vec![];
            for _ in 0..5 {
                let m = mutex.clone();
                let c = cond.clone();
                handles.push(s.spawn(move || {
                    let guard = m.lock();
                    let _g = c.wait(guard);
                }));
            }

            thread::sleep(Duration::from_millis(100));
            cond.notify_all();

            for h in handles {
                h.join().unwrap();
            }
        });
    });
}
