use halo::{GhostToken, GhostOnceLock, SharedGhostToken};
use std::sync::Arc;
use std::thread;

#[test]
fn test_ghost_once_lock_basic() {
    GhostToken::new(|token| {
        let lock = GhostOnceLock::new();
        assert!(!lock.is_initialized(&token));
        assert_eq!(lock.get(&token), None);

        assert_eq!(lock.set(&token, 42), Ok(()));
        assert!(lock.is_initialized(&token));
        assert_eq!(lock.get(&token), Some(&42));
        assert_eq!(lock.set(&token, 100), Err(100));
        assert_eq!(lock.get(&token), Some(&42));
    });
}

#[test]
fn test_ghost_once_lock_get_or_init() {
    GhostToken::new(|token| {
        let lock = GhostOnceLock::new();
        let value = lock.get_or_init(&token, || 42);
        assert_eq!(*value, 42);

        let value2 = lock.get_or_init(&token, || 100);
        assert_eq!(*value2, 42);
    });
}

#[test]
fn test_ghost_once_lock_mut_access() {
    GhostToken::new(|mut token| {
        let mut lock = GhostOnceLock::new();
        lock.set(&token, 42).unwrap();

        if let Some(v) = lock.get_mut_branded(&mut token) {
            *v += 1;
        }
        assert_eq!(lock.get(&token), Some(&43));

        if let Some(v) = lock.get_mut() {
            *v += 1;
        }
        assert_eq!(lock.get(&token), Some(&44));
    });
}

#[test]
fn test_ghost_once_lock_take() {
    GhostToken::new(|token| {
        let mut lock = GhostOnceLock::new();
        lock.set(&token, 42).unwrap();
        assert_eq!(lock.take(), Some(42));
        assert!(!lock.is_initialized(&token));
        assert_eq!(lock.get(&token), None);
    });
}

#[test]
fn test_ghost_once_lock_concurrent() {
    GhostToken::new(|token| {
        let shared_token = Arc::new(SharedGhostToken::new(token));
        let lock = Arc::new(GhostOnceLock::new());
        let mut handles = Vec::new();

        for i in 0..10 {
            let t = shared_token.clone();
            let l = lock.clone();
            handles.push(thread::spawn(move || {
                let token_guard = t.read();
                l.get_or_init(&*token_guard, || i)
            }));
        }

        let mut values = Vec::new();
        for h in handles {
            values.push(*h.join().unwrap());
        }

        // All threads should see the same value (the one from the thread that won the race)
        let first = values[0];
        for v in values {
            assert_eq!(v, first);
        }

        let token_guard = shared_token.read();
        assert_eq!(lock.get(&*token_guard), Some(&first));
    });
}
