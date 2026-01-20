use halo::alloc::StaticRc;
use halo::{GhostCell, GhostToken};

#[test]
fn test_scope_basic() {
    StaticRc::scope(42, |rc| {
        assert_eq!(*rc, 42);

        let (rc1, rc2) = rc.split::<1, 0>();
        assert_eq!(*rc1, 42);

        // This confirms we can use the scoped rc.
        // N=1, D=1. Split M=1, R=0.
        // rc1: 1, 1. rc2: 0, 1.
        // join back.
        let rc = unsafe { rc1.join_unchecked::<0, 1>(rc2) };
        assert_eq!(*rc, 42);
    });
}

#[test]
fn test_join_unchecked_optimization() {
    StaticRc::scope(100, |rc| {
        // We know rc has a unique brand here.
        let (rc1, rc2) = rc.split::<1, 0>(); // 1/1 -> 1/1 + 0/1 (Wait, split M, R where M+R=N. N=1.)
        // split::<1, 0> implies M=1, R=0.

        // We can safely rejoin them without checking pointers because the type system
        // guarantees they are from the same 'id (which is unique to this scope).
        let rc_back = unsafe { rc1.join_unchecked::<0, 1>(rc2) };
        assert_eq!(*rc_back, 100);
    });
}

#[test]
fn test_ghost_cell_integration() {
    GhostToken::new(|mut token| {
        // Create a StaticRc containing a GhostCell
        // We can use scope to ensure unique ID for the RC,
        // and GhostToken for the Cell permissions.
        StaticRc::scope(GhostCell::new(10), |rc| {
            // Check immutable access via convenience method
            assert_eq!(*rc.borrow(&token), 10);

            // Check mutable access via convenience method
            *rc.borrow_mut(&mut token) += 5;
            assert_eq!(*rc.borrow(&token), 15);

            // Split and mutate via one share
            let (rc1, rc2) = rc.split::<1, 0>(); // 1/1 -> 1/1 + 0/1

            // Even though rc2 has 0 shares (maybe meaningless for ownership, but carries pointer),
            // it can still access data if it has pointer?
            // StaticRc::split returns StaticRc.
            // StaticRc gives access to T via get/deref.
            // If N=0, does it matter?
            // StaticRc implementation doesn't restrict access based on N, only Drop logic.
            // So yes, 0-share RC is a weak reference that doesn't own?
            // Actually, N/D is just accounting.
            // access is always allowed.

            *rc1.borrow_mut(&mut token) += 5;
            assert_eq!(*rc2.borrow(&token), 20);

            unsafe { rc1.join_unchecked::<0, 1>(rc2) };
        });
    });
}

#[test]
fn test_scope_isolation() {
    // This test conceptually verifies that we cannot mix RCs from different scopes.
    // We cannot easily test "compile fail" in a unit test file without trybuild.
    // But we can verify that separate scopes work independently.

    StaticRc::scope(1, |_rc1| {
        StaticRc::scope(2, |_rc2| {
            // If we tried to join _rc1 and _rc2, it would be a compile error
            // because _rc1 has type StaticRc<'id1, ...> and _rc2 has StaticRc<'id2, ...>.
            // _rc1.join(_rc2); // Uncommenting this should fail to compile.
        });
    });
}

#[test]
#[should_panic(expected = "Join result amount must equal sum of shares")]
fn test_join_unchecked_checks_amounts() {
    StaticRc::scope(10, |rc| {
        let (rc1, rc2) = rc.split::<1, 0>();
        // Improper sum usage should still assert.
        // 1 + 0 = 1. If we say SUM=2, it should panic.
        unsafe { rc1.join_unchecked::<0, 2>(rc2) };
    });
}
