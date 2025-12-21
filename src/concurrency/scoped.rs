//! Branded scoped-thread helpers (std-only, minimal overhead).
//!
//! These helpers wrap `std::thread::scope` to provide two useful patterns:
//! - **Read-scope**: share `&GhostToken<'brand>` across threads for read-only access.
//! - **Write-scope**: move `GhostToken<'brand>` by value into a thread and return it
//!   ("baton passing") for exclusive mutation without locking.
// People's expectation from GhostCell (per RustBelt paper) is "no runtime borrow state";
// these helpers keep that property while still respecting Rust's thread/lifetime rules.

use crate::GhostToken;

/// A scoped environment that can spawn tasks using a shared `&GhostToken<'brand>`.
pub struct GhostReadScope<'scope, 'env, 'brand> {
    scope: &'scope std::thread::Scope<'scope, 'env>,
    token: &'env GhostToken<'brand>,
}

impl<'scope, 'env, 'brand> GhostReadScope<'scope, 'env, 'brand> {
    /// Spawns a scoped thread that receives `&GhostToken<'brand>`.
    ///
    /// This is suitable for read-only work (e.g. `cell.borrow(token)`).
    #[inline]
    pub fn spawn<T, F>(&self, f: F) -> std::thread::ScopedJoinHandle<'scope, T>
    where
        T: Send + 'scope,
        F: FnOnce(&'env GhostToken<'brand>) -> T + Send + 'scope,
    {
        let t = self.token;
        self.scope.spawn(move || f(t))
    }
}

/// A scoped environment that can spawn tasks which **own** the `GhostToken<'brand>`.
pub struct GhostWriteScope<'scope, 'env, 'brand> {
    scope: &'scope std::thread::Scope<'scope, 'env>,
    _brand: core::marker::PhantomData<&'brand mut ()>,
}

impl<'scope, 'env, 'brand> GhostWriteScope<'scope, 'env, 'brand> {
    /// Spawns a scoped thread that takes ownership of the token, runs `f` with
    /// `&mut GhostToken<'brand>`, and returns the token.
    ///
    /// This is the lock-free "baton passing" pattern: exactly one thread owns the
    /// token (and therefore the right to create `&mut` borrows) at a time.
    #[inline]
    pub fn spawn_with_token<T, F>(
        &self,
        token: GhostToken<'brand>,
        f: F,
    ) -> std::thread::ScopedJoinHandle<'scope, (T, GhostToken<'brand>)>
    where
        'brand: 'scope,
        T: Send + 'scope,
        F: FnOnce(&mut GhostToken<'brand>) -> T + Send + 'scope,
    {
        self.scope.spawn(move || {
            let mut t = token;
            let out = f(&mut t);
            (out, t)
        })
    }
}

/// Runs a scoped region where `&GhostToken<'brand>` is shared with spawned threads.
#[inline]
pub fn with_read_scope<'env, 'brand, R, F>(token: &'env GhostToken<'brand>, f: F) -> R
where
    F: for<'scope> FnOnce(GhostReadScope<'scope, 'env, 'brand>) -> R,
{
    std::thread::scope(|scope| f(GhostReadScope { scope, token }))
}

/// Runs a scoped region where the token is **moved** into the region and must be returned.
#[inline]
pub fn with_write_scope<'env, 'brand, R, F>(
    token: GhostToken<'brand>,
    f: F,
) -> (R, GhostToken<'brand>)
where
    'brand: 'env,
    F: for<'scope> FnOnce(GhostWriteScope<'scope, 'env, 'brand>, GhostToken<'brand>) -> (R, GhostToken<'brand>),
{
    std::thread::scope(|scope| f(GhostWriteScope { scope, _brand: core::marker::PhantomData }, token))
}

/// Runs a **lock-free** two-phase parallel pattern:
///
/// 1. A parallel **compute phase** where all threads share `&GhostToken<'brand>` (read-only).
/// 2. A sequential **commit phase** where the caller gets `&mut GhostToken<'brand>` (exclusive write).
///
/// This is the recommended “ghost writing without locks” methodology when you can batch/aggregate
/// writes: do expensive work in parallel, then apply a compact set of updates in one place with
/// exclusive token access.
#[inline]
pub fn parallel_read_then_commit<'brand, W, R>(
    token: &mut GhostToken<'brand>,
    threads: usize,
    compute: impl for<'env> Fn(&'env GhostToken<'brand>, usize) -> W + Sync + Send,
    commit: impl FnOnce(&mut GhostToken<'brand>, Vec<W>) -> R,
) -> R
where
    W: Send,
{
    assert!(threads != 0, "threads must be > 0");

    let work: Vec<W> = with_read_scope(&*token, |scope| {
        let compute = &compute;
        let mut hs = Vec::with_capacity(threads);
        for tid in 0..threads {
            hs.push(scope.spawn(move |t| (compute)(t, tid)));
        }
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });

    commit(token, work)
}


