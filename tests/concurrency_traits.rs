use halo::{GhostCell, GhostToken};

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

#[test]
fn ghost_token_is_send_and_sync() {
    assert_send::<GhostToken<'static>>();
    assert_sync::<GhostToken<'static>>();
}

#[test]
fn ghost_cell_send_sync_follows_t_bounds() {
    // `u64` is Send + Sync, so `GhostCell<u64>` should be Send + Sync.
    assert_send::<GhostCell<'static, u64>>();
    assert_sync::<GhostCell<'static, u64>>();
}
