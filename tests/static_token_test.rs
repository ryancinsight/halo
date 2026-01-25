use halo::token::{static_token, with_static_token, with_static_token_mut};
use halo::cell::GhostCell;
use std::thread;

#[test]
fn test_static_token_is_singleton() {
    let t1 = static_token();
    let t2 = static_token();
    // Compare addresses of the references.
    assert_eq!(t1 as *const _, t2 as *const _);
}

#[test]
fn test_with_static_token() {
    let result = with_static_token(|token| {
        // Can read from a cell created with static lifetime
        let cell = GhostCell::<'static, i32>::new(42);
        *cell.borrow(token)
    });
    assert_eq!(result, 42);
}

#[test]
fn test_concurrent_immutable_access() {
    static GLOBAL_CELL: GhostCell<'static, i32> = GhostCell::new(100);

    let handles: Vec<_> = (0..10).map(|_| {
        thread::spawn(|| {
            with_static_token(|token| {
                *GLOBAL_CELL.borrow(token)
            })
        })
    }).collect();

    for h in handles {
        assert_eq!(h.join().unwrap(), 100);
    }
}

#[test]
fn test_mutable_bootstrapping() {
    static MUTABLE_CELL: GhostCell<'static, i32> = GhostCell::new(0);

    // Update value safely (serialized by the internal mutex of with_static_token_mut)
    // Note: In a real scenario, this should run before any concurrent readers exist.
    // In this test suite, it might run concurrently with other tests.
    // Because it accesses a different cell (MUTABLE_CELL) than the other tests (GLOBAL_CELL),
    // there is no data race on the cell content.
    unsafe {
        with_static_token_mut(|token| {
            let cell_ref = MUTABLE_CELL.borrow_mut(token);
            *cell_ref = 123;
        });
    }

    with_static_token(|token| {
         assert_eq!(*MUTABLE_CELL.borrow(token), 123);
    });
}
