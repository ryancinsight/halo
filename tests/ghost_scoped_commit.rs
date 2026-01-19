use halo::{concurrency::scoped, GhostCell, GhostToken};

#[test]
fn parallel_read_then_commit_applies_updates() {
    GhostToken::new(|mut token| {
        let a = GhostCell::new(0u64);
        let b = GhostCell::new(0u64);

        let out = scoped::parallel_read_then_commit(
            &mut token,
            4,
            |_t, tid| (tid as u64) + 1,
            |t, work| {
                // Commit: apply sum(work) to both cells.
                let sum = work.into_iter().fold(0u64, |acc, x| acc + x);
                *a.borrow_mut(t) += sum;
                *b.borrow_mut(t) += sum;
                sum
            },
        );

        assert_eq!(out, 10);
        assert_eq!(*a.borrow(&token), 10);
        assert_eq!(*b.borrow(&token), 10);
    });
}
