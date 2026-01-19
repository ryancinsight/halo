//! Scoped-thread patterns for GhostCell (no locks on data access).
//!
//! Key idea:
//! - For **read-only** work, you can share `&GhostToken` across threads (token is `Sync`).
//! - For **mutation**, you must move the token (by value) to ensure exclusive access.
//!   Use `std::thread::scope` so the brand does not outlive its scope.

use halo::{concurrency::scoped, GhostCell, GhostToken};

fn main() {
    // This example uses scoped threads, which are joined before the token brand ends.
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(0u64);

        // Phase 1: read-only work. Share `&GhostToken` across threads.
        scoped::with_read_scope(&token, |rs| {
            let cell_ref = &cell;
            let h = rs.spawn(move |t| *cell_ref.borrow(t));
            let _ = h.join().unwrap();
        });

        // Phase 2: exclusive mutation. Move the token by value into one worker and return it.
        let ((), new_token) = scoped::with_write_scope(token, |ws, token| {
            let cell_ref = &cell;
            let h = ws.spawn_with_token(token, move |t| {
                *cell_ref.borrow_mut(t) = 123;
            });
            let (_unit, token) = h.join().unwrap();
            ((), token)
        });
        token = new_token;

        // Phase 3: use the token again in this thread.
        assert_eq!(cell.get(&token), 123);
    });
}
