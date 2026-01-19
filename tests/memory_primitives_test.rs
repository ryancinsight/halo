use halo::alloc::{StaticRc, BrandedBox};
use halo::{GhostToken, GhostCell};

#[test]
fn test_static_rc_basic() {
    let rc: StaticRc<i32, 10, 10> = StaticRc::new(42);
    assert_eq!(*rc, 42);

    // Split: 10 -> 5 + 5
    let (rc1, rc2): (StaticRc<i32, 5, 10>, StaticRc<i32, 5, 10>) = rc.split::<5, 5>();
    assert_eq!(*rc1, 42);
    assert_eq!(*rc2, 42);

    // Split again: 5 -> 2 + 3
    let (rc1a, rc1b): (StaticRc<i32, 2, 10>, StaticRc<i32, 3, 10>) = rc1.split::<2, 3>();
    assert_eq!(*rc1a, 42);
    assert_eq!(*rc1b, 42);

    // Join: 2 + 3 -> 5
    let rc1 = rc1a.join::<3, 5>(rc1b);
    // Join: 5 + 5 -> 10
    let rc = rc1.join::<5, 10>(rc2);
    assert_eq!(*rc, 42);
    // Automatically drops here, N=D=10, so it deallocates.
}

#[test]
fn test_static_rc_adjust() {
    let rc: StaticRc<i32, 1, 1> = StaticRc::new(100);
    let rc: StaticRc<i32, 10, 10> = rc.adjust();
    let (rc1, rc2) = rc.split::<5, 5>();
    assert_eq!(*rc1, 100);
    assert_eq!(*rc2, 100);

    let rc = rc1.join::<5, 10>(rc2);
    assert_eq!(*rc, 100);
}

#[test]
#[should_panic(expected = "Split amounts must sum to current shares")]
fn test_static_rc_invalid_split() {
    let rc: StaticRc<i32, 10, 10> = StaticRc::new(42);
    // 5 + 4 != 10
    let (_rc1, _rc2): (StaticRc<i32, 5, 10>, StaticRc<i32, 4, 10>) = rc.split::<5, 4>();
}

#[test]
fn test_branded_box() {
    GhostToken::new(|mut token| {
        let mut b = BrandedBox::new(10, &mut token);
        assert_eq!(*b.borrow(&token), 10);

        *b.borrow_mut(&mut token) = 20;
        assert_eq!(*b.borrow(&token), 20);
    });
}

#[test]
fn test_branded_box_downgrade() {
    GhostToken::new(|mut token| {
        let b = BrandedBox::new(55, &mut token);
        let rc: StaticRc<GhostCell<i32>, 4, 4> = b.into_shared::<4>();

        // Access via rc
        // rc derefs to GhostCell
        assert_eq!(*rc.borrow(&token), 55);

        *rc.borrow_mut(&mut token) = 66;
        assert_eq!(*rc.borrow(&token), 66);

        let (rc1, rc2) = rc.split::<2, 2>();

        assert_eq!(*rc1.borrow(&token), 66);
        assert_eq!(*rc2.borrow(&token), 66);

        // Mutate via one share
        *rc1.borrow_mut(&mut token) = 77;
        assert_eq!(*rc2.borrow(&token), 77);

        let rc = rc1.join::<2, 4>(rc2);
        // Drops here
    });
}
