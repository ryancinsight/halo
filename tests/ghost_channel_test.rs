use halo::GhostToken;
use halo::concurrency::sync::{ghost_channel, ghost_oneshot, RecvError, TryRecvError};
use std::thread;
use std::time::Duration;

#[test]
fn test_mpsc_basic() {
    GhostToken::new(|token| {
        let (tx, rx) = ghost_channel();

        tx.send(1, &token).unwrap();
        tx.send(2, &token).unwrap();
        tx.send(3, &token).unwrap();

        assert_eq!(rx.recv(&token).unwrap(), 1);
        assert_eq!(rx.recv(&token).unwrap(), 2);
        assert_eq!(rx.recv(&token).unwrap(), 3);
    });
}

#[test]
fn test_mpsc_try_recv() {
    GhostToken::new(|token| {
        let (tx, rx) = ghost_channel();

        assert_eq!(rx.try_recv(&token), Err(TryRecvError::Empty));

        tx.send(42, &token).unwrap();
        assert_eq!(rx.try_recv(&token), Ok(42));
        assert_eq!(rx.try_recv(&token), Err(TryRecvError::Empty));
    });
}

#[test]
fn test_mpsc_disconnect() {
    GhostToken::new(|token| {
        let (tx, rx) = ghost_channel::<i32>();
        drop(tx);
        assert_eq!(rx.recv(&token), Err(RecvError));
    });
}

#[test]
fn test_mpsc_threads() {
    // We can't pass the same token to multiple threads directly because GhostToken is unique and !Clone.
    // However, we can use `GhostToken::new` inside each thread if we want distinct brands.
    // But the channel is branded with ONE brand.
    // So all participants must share access to the token or the token must be shared.
    // GhostToken is !Sync if I recall correctly?
    // Wait, GhostToken is Sync.
    // So we can share &GhostToken across threads.

    GhostToken::new(|token| {
        let (tx, rx) = ghost_channel();

        // We need to pass the token to threads.
        // Since the closure has `token`, we can't easily move it into multiple threads unless we use scoped threads or Arc?
        // Scoped threads are best.

        std::thread::scope(|s| {
            s.spawn(|| {
                for i in 0..10 {
                    tx.send(i, &token).unwrap();
                }
            });

            s.spawn(|| {
                let mut sum = 0;
                for _ in 0..10 {
                    sum += rx.recv(&token).unwrap();
                }
                assert_eq!(sum, 45);
            });
        });
    });
}

#[test]
fn test_oneshot_basic() {
    GhostToken::new(|token| {
        let (tx, rx) = ghost_oneshot();

        tx.send(100, &token).unwrap();
        assert_eq!(rx.recv(&token).unwrap(), 100);
    });
}

#[test]
fn test_oneshot_threads() {
    GhostToken::new(|token| {
        let (tx, rx) = ghost_oneshot();

        std::thread::scope(|s| {
            s.spawn(|| {
                thread::sleep(Duration::from_millis(10));
                tx.send(200, &token).unwrap();
            });

            assert_eq!(rx.recv(&token).unwrap(), 200);
        });
    });
}

#[test]
fn test_oneshot_drop_sender() {
    GhostToken::new(|token| {
        let (tx, rx) = ghost_oneshot::<i32>();
        drop(tx);
        assert!(rx.recv(&token).is_err());
    });
}
