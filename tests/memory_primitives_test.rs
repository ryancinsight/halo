use halo::alloc::{BrandedBox, StaticRc};
use halo::{GhostCell, GhostToken};
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn test_static_rc_basic() {
    GhostToken::new(|token| {
        let rc: StaticRc<'_, i32, 10, 10> = StaticRc::new(42);
        assert_eq!(*rc.borrow(&token), 42);

        // Split: 10 -> 5 + 5
        let (rc1, rc2): (StaticRc<'_, i32, 5, 10>, StaticRc<'_, i32, 5, 10>) = rc.split::<5, 5>();
        assert_eq!(*rc1.borrow(&token), 42);
        assert_eq!(*rc2.borrow(&token), 42);

        // Split again: 5 -> 2 + 3
        let (rc1a, rc1b): (StaticRc<'_, i32, 2, 10>, StaticRc<'_, i32, 3, 10>) =
            rc1.split::<2, 3>();
        assert_eq!(*rc1a.borrow(&token), 42);
        assert_eq!(*rc1b.borrow(&token), 42);

        // Join: 2 + 3 -> 5
        let rc1 = rc1a.join::<3, 5>(rc1b);
        // Join: 5 + 5 -> 10
        let rc = rc1.join::<5, 10>(rc2);
        assert_eq!(*rc.borrow(&token), 42);
        // Automatically drops here, N=D=10, so it deallocates.
    });
}

#[test]
fn test_static_rc_adjust() {
    GhostToken::new(|token| {
        let rc: StaticRc<'_, i32, 1, 1> = StaticRc::new(100);
        let rc: StaticRc<'_, i32, 10, 10> = rc.adjust();
        let (rc1, rc2) = rc.split::<5, 5>();
        assert_eq!(*rc1.borrow(&token), 100);
        assert_eq!(*rc2.borrow(&token), 100);

        let rc = rc1.join::<5, 10>(rc2);
        assert_eq!(*rc.borrow(&token), 100);
    });
}

#[test]
#[should_panic(expected = "Split amounts must sum to current shares")]
fn test_static_rc_invalid_split() {
    GhostToken::new(|_token| {
        let rc: StaticRc<'_, i32, 10, 10> = StaticRc::new(42);
        // 5 + 4 != 10
        let (_rc1, _rc2): (StaticRc<'_, i32, 5, 10>, StaticRc<'_, i32, 4, 10>) = rc.split::<5, 4>();
    });
}

#[test]
fn test_branded_box() {
    GhostToken::new(|mut token| {
        let mut b = BrandedBox::new(10);
        assert_eq!(*b.borrow(&token), 10);

        *b.borrow_mut(&mut token) = 20;
        assert_eq!(*b.borrow(&token), 20);
    });
}

#[test]
fn test_branded_box_downgrade() {
    GhostToken::new(|mut token| {
        let b = BrandedBox::new(55);
        let rc: StaticRc<GhostCell<i32>, 4, 4> = b.into_shared::<4>();

        // Access via rc
        // rc derefs to GhostCell (previously), now we must borrow rc first
        let cell = rc.borrow(&token);
        assert_eq!(*cell.borrow(&token), 55);

        *cell.borrow_mut(&mut token) = 66;
        assert_eq!(*cell.borrow(&token), 66);

        let (rc1, rc2) = rc.split::<2, 2>();

        let cell1 = rc1.borrow(&token);
        let cell2 = rc2.borrow(&token);
        assert_eq!(*cell1.borrow(&token), 66);
        assert_eq!(*cell2.borrow(&token), 66);

        // Mutate via one share
        *cell1.borrow_mut(&mut token) = 77;
        assert_eq!(*cell2.borrow(&token), 77);

        let rc = rc1.join::<2, 4>(rc2);
        // Drops here
    });
}

#[test]
fn test_branded_box_zst_drop() {
    struct ZstDropCounter {
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    use std::cell::RefCell;
    thread_local! {
        static DROP_COUNT: RefCell<usize> = RefCell::new(0);
    }

    struct MyZst;
    impl Drop for MyZst {
        fn drop(&mut self) {
            DROP_COUNT.with(|c| *c.borrow_mut() += 1);
        }
    }

    DROP_COUNT.with(|c| *c.borrow_mut() = 0);

    GhostToken::new(|mut token| {
        let b = BrandedBox::new(MyZst);
        // Should not be dropped yet.
        DROP_COUNT.with(|c| assert_eq!(*c.borrow(), 0));
        drop(b);
    });

    // Should be dropped now.
    DROP_COUNT.with(|c| assert_eq!(*c.borrow(), 1));
}
