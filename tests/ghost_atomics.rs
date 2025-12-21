use core::sync::atomic::Ordering;
use halo::concurrency::atomic::{GhostAtomicBool, GhostAtomicU64, GhostAtomicUsize};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn branded_atomics_are_send_sync_and_work() {
    assert_send_sync::<GhostAtomicBool<'static>>();
    assert_send_sync::<GhostAtomicU64<'static>>();
    assert_send_sync::<GhostAtomicUsize<'static>>();

    let a = GhostAtomicU64::new(0);
    assert_eq!(a.load(Ordering::Relaxed), 0);
    a.store(5, Ordering::Relaxed);
    assert_eq!(a.fetch_add(2, Ordering::Relaxed), 5);
    assert_eq!(a.load(Ordering::Relaxed), 7);

    let f = GhostAtomicBool::new(false);
    assert_eq!(f.swap(true, Ordering::Relaxed), false);
    assert_eq!(f.load(Ordering::Relaxed), true);

    let u = GhostAtomicUsize::new(1);
    assert_eq!(u.swap(2, Ordering::Relaxed), 1);
    assert_eq!(u.load(Ordering::Relaxed), 2);
    assert_eq!(
        u.compare_exchange(2, 9, Ordering::Relaxed, Ordering::Relaxed),
        Ok(2)
    );
    assert_eq!(u.load(Ordering::Relaxed), 9);
}


