use halo::alloc::GenerationalPool;
use halo::GhostToken;

#[test]
fn test_generational_pool_basic() {
    GhostToken::new(|mut token| {
        let pool = GenerationalPool::new();

        let idx1 = pool.alloc(&mut token, 10);
        let idx2 = pool.alloc(&mut token, 20);

        assert_eq!(*pool.get(&token, idx1).unwrap(), 10);
        assert_eq!(*pool.get(&token, idx2).unwrap(), 20);

        // Test free
        assert!(pool.free(&mut token, idx1));
        assert!(pool.get(&token, idx1).is_none());

        // Reuse
        let idx3 = pool.alloc(&mut token, 30);
        // idx3 should have same index as idx1 but different generation
        assert_eq!(idx1.index(), idx3.index());
        assert_ne!(idx1.generation(), idx3.generation());

        assert!(pool.get(&token, idx1).is_none()); // Old index should fail
        assert_eq!(*pool.get(&token, idx3).unwrap(), 30);
    });
}

#[test]
fn test_generational_pool_aba_protection() {
    GhostToken::new(|mut token| {
        let pool = GenerationalPool::new();

        let idx = pool.alloc(&mut token, "A");
        pool.free(&mut token, idx);

        let idx_new = pool.alloc(&mut token, "B");

        // Old index still points to same slot index
        assert_eq!(idx.index(), idx_new.index());

        // But access with old index fails
        assert!(pool.get(&token, idx).is_none());

        // Access with new index works
        assert_eq!(*pool.get(&token, idx_new).unwrap(), "B");
    });
}
