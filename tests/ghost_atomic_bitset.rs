use core::sync::atomic::Ordering;

use halo::{concurrency::atomic::GhostAtomicBitset, GhostToken};

#[test]
fn atomic_bitset_basic() {
    GhostToken::new(|_token| {
        let b: GhostAtomicBitset<'_> = GhostAtomicBitset::new(130);
        assert_eq!(b.len_bits(), 130);

        assert!(!b.is_set(0));
        assert!(b.test_and_set(0, Ordering::Relaxed));
        assert!(b.is_set(0));
        assert!(!b.test_and_set(0, Ordering::Relaxed));

        assert!(b.test_and_set(129, Ordering::Relaxed));
        assert!(b.is_set(129));

        b.clear_all();
        assert!(!b.is_set(0));
        assert!(!b.is_set(129));
    });
}
