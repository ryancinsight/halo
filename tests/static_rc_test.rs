use halo::alloc::{BrandedBox, StaticRc};
use halo::{GhostCell, GhostToken};
use std::mem::MaybeUninit;

#[test]
fn test_get_mut_unique() {
    let mut rc: StaticRc<'_, i32, 1, 1> = StaticRc::new(42);
    *rc.get_mut() = 100;
    assert_eq!(*rc, 100);
}

#[test]
fn test_from_box_into_box() {
    let b = Box::new(12345);
    // Convert to StaticRc
    let mut rc: StaticRc<'_, i32, 1, 1> = StaticRc::from_box(b);
    assert_eq!(*rc, 12345);
    *rc.get_mut() = 54321;

    // Convert back to Box
    let b = rc.into_box();
    assert_eq!(*b, 54321);
}

#[test]
fn test_from_branded_box_into_branded_box() {
    GhostToken::new(|mut token| {
        let bb = BrandedBox::new(999);

        // Convert to StaticRc
        let mut rc: StaticRc<'_, i32, 1, 1> = StaticRc::from_branded_box(bb);
        assert_eq!(*rc, 999);
        *rc.get_mut() = 888;

        // Convert back to BrandedBox
        let bb = rc.into_branded_box();
        assert_eq!(*bb.borrow(&token), 888);
    });
}

#[test]
fn test_new_uninit() {
    let mut rc: StaticRc<'_, MaybeUninit<i32>, 1, 1> = StaticRc::new_uninit();

    // Initialize
    rc.get_mut().write(777);

    // Assume init
    let rc: StaticRc<'_, i32, 1, 1> = unsafe { rc.assume_init() };
    assert_eq!(*rc, 777);
}

#[test]
fn test_scope_basic() {
    StaticRc::scope(42, |rc| {
        assert_eq!(*rc, 42);

        let (rc1, rc2) = rc.split::<1, 0>();
        assert_eq!(*rc1, 42);

        let rc = unsafe { rc1.join_unchecked::<0, 1>(rc2) };
        assert_eq!(*rc, 42);
    });
}

#[test]
fn test_join_unchecked_optimization() {
    StaticRc::scope(100, |rc| {
        let (rc1, rc2) = rc.split::<1, 0>();
        let rc_back = unsafe { rc1.join_unchecked::<0, 1>(rc2) };
        assert_eq!(*rc_back, 100);
    });
}

#[test]
fn test_ghost_cell_integration() {
    GhostToken::new(|mut token| {
        StaticRc::scope(GhostCell::new(10), |rc| {
            assert_eq!(*rc.borrow(&token), 10);
            *rc.borrow_mut(&mut token) += 5;
            assert_eq!(*rc.borrow(&token), 15);
        });
    });
}

#[test]
#[should_panic(expected = "Join result amount must equal sum of shares")]
fn test_join_unchecked_checks_amounts() {
    StaticRc::scope(10, |rc| {
        let (rc1, rc2) = rc.split::<1, 0>();
        unsafe { rc1.join_unchecked::<0, 2>(rc2) };
    });
}
