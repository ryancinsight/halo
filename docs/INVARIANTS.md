# Ghost invariants (soundness + performance)

This crate implements GhostCell-style interior mutability by separating **permissions** (the token) from **data** (the cells). The correctness story is entirely about enforcing Rust’s aliasing rules with a *linear*, branded capability.

## Token linearity

- `GhostToken<'brand>` is **zero-sized** but **not** `Copy`/`Clone`.
- All APIs that can yield `&mut T` require `&mut GhostToken<'brand>`.
- Rust’s borrow checker ensures there is at most one live `&mut GhostToken<'brand>` at a time, therefore safe code cannot produce overlapping `&mut T` from the same brand.

## Brand separation

- Each call to `GhostToken::new` introduces a fresh brand `'brand`.
- Values branded with distinct brands are different types, so they cannot be mixed in safe code.

## Mapping to the GhostCell paper (RustBelt)

The paper’s key move is to separate:

- **permissions / capability** (the token), from
- **data** (the cell)

so that *aliasing* rules are enforced without runtime borrow state.

In this crate:

- **Brand creation** is expressed by `GhostToken::new` using rank-2 polymorphism:
  - `for<'brand> FnOnce(GhostToken<'brand>) -> R`
  - This makes `'brand` fresh and unnameable outside the closure.
- **Capability linearity** is enforced by making `GhostToken<'brand>`:
  - **ZST** (no runtime cost)
  - **not** `Copy`/`Clone` (prevents duplicating the capability)
  - therefore, safe code cannot obtain two simultaneous `&mut GhostToken<'brand>` values.
- **Cell branding** uses an invariant brand marker (`PhantomData<&'brand mut ()>`) so that
  values from different brands cannot unify via subtyping.

What we intentionally do *not* attempt here:

- Formal proof artifacts / mechanized verification (the RustBelt repo has that). We keep the
  invariants local and explicit in rustdoc and in this document.
- Automatic synchronization: the token enforces aliasing constraints in safe code; it does
  not replace locks/atomics when you need true concurrent mutation.

## Aliasing model

- `GhostUnsafeCell<'brand, T>` is the sole primitive that touches `core::cell::UnsafeCell<T>`.
- Safe access patterns are:
  - `&T` via `&GhostToken<'brand>`
  - `&mut T` via `&mut GhostToken<'brand>`
- Raw pointers exist only as escape hatches; dereferencing them remains `unsafe` and obeys standard Rust raw-pointer contracts.

## Send/Sync obligations

The ghost types are conditionally `Send`/`Sync` with explicit unsafe impls:

- Sharing `&Ghost*Cell<T>` across threads is safe **iff** the only safe shared access yields `&T`, which is thread-safe **iff** `T: Sync`.
- Moving a `Ghost*Cell<T>` across threads is safe **iff** the contained `T` can be moved across threads (`T: Send`).

The token gating does **not** introduce hidden synchronization: it only enforces aliasing constraints in safe code.

## Concurrency patterns (what GhostCell does and does not do)

GhostCell is primarily about **aliasing discipline** (who may obtain `&mut T`) without runtime borrow state.
It does **not** automatically provide synchronization for truly concurrent mutation.

Lock-free patterns you *can* build:

- **Parallel read-only access**:
  - share `&GhostToken<'brand>` across threads and call `borrow(&token)` to get `&T`.
  - this is safe when `T: Sync` (and thus `GhostCell<'brand, T>: Sync`).
- **Single-writer baton passing**:
  - move the token (by value) to exactly one thread at a time, mutate through `&mut GhostToken<'brand>`,
    then return the token to transfer exclusive write capability.
  - use `std::thread::scope` (or other scoped concurrency) so the brand `'brand` cannot outlive its scope.

Patterns you *cannot* get without synchronization:

- **Multiple writers concurrently**. If you need that, you need real coordination (locks/atomics/message passing),
  and GhostCell is not a drop-in replacement for `Mutex`/`RwLock` in that scenario.

### Lock-free “batched writing” (parallel compute → sequential commit)

If you want to **avoid locks** but still perform write-heavy work efficiently, use a two-phase structure:

1. **Parallel compute** under shared `&GhostToken<'brand>` (read-only): each thread computes a compact update representation
   (e.g. per-thread deltas, sparse updates, batches).
2. **Sequential commit** under `&mut GhostToken<'brand>`: apply the compact updates to `GhostCell`s.

This is exposed as `concurrency::scoped::parallel_read_then_commit`. It is lock-free, memory-efficient (when updates are
aggregated), and remains sound because the only phase that creates `&mut T` is the commit phase with exclusive token access.

## Branded atomics

If you need truly concurrent writers, the correct primitive is **hardware atomics**.
This crate provides branded atomic wrappers under `halo::concurrency::atomic`:

- They are **lock-free** where the platform atomic type is lock-free.
- The brand is a **compile-time domain marker** (no runtime borrow state).
- They do not “remove” atomic costs; they aim for **zero wrapper overhead**.

## Atomic bitsets (dense visited sets)

`GhostAtomicBitset<'brand>` is a **word-packed** visited/flag structure built from `GhostAtomicUsize<'brand>`.

- **Correctness invariant**: bit indices must be in-range of `len_bits()` (the safe APIs enforce this; unchecked APIs require it).
- **Concurrency invariant**: `test_and_set` is implemented via `fetch_or` on a word. This is linearizable per-bit and safe under
  arbitrary multi-threaded interleavings.
- **Performance goal**: compared to `Vec<AtomicBool>`, the bitset improves locality and reduces memory traffic for dense visited sets.

## Lock-free worklists (work distribution)

This crate contains two lock-free worklists for parallel traversals:

- **`GhostTreiberStack`**: an MPMC stack for indices. It supports `push_batch` to splice a local buffer onto the shared head with a
  single CAS (contention reduction).
- **`GhostChaseLevDeque`**: a fixed-capacity Chase–Lev work-stealing deque for indices.
  - **Owner-only operations**: `push_bottom` and `pop_bottom` must only be called by the owning worker.
  - **Multi-stealer operation**: `steal` may be called concurrently by multiple non-owners.
  - **Sentinel invariant**: the deque stores `usize` and reserves a sentinel `NONE` (must never be pushed).
  - **Index invariants**: `top` and `bottom` are monotone counters; `capacity` is a power of two and the ring index uses `mask`.
  - **Ordering note**: the implementation uses fences and Acquire/Release operations to ensure stealers never observe an
    uninitialized slot value “as a real item”.

## Global Static Token

To support global singletons and bootstrapping, the library provides a **Global Static Token** (`'static` brand).

- **Uniqueness**: Only one static token exists for the global brand. It is created lazily and leaked.
- **Immutable Access**: `static_token()` and `with_static_token` provide concurrent, lock-free access to `&'static GhostToken<'static>`. This is safe because `GhostToken` is `Sync` and immutable access only permits reading `GhostCell`s.
- **Mutable Access**: `with_static_token_mut` provides `&mut GhostToken<'static>`.
  - **Safety Rationale**: This function is `unsafe` because it creates a mutable reference that could alias with the leaked static reference.
  - **Usage**: It must **only** be used during single-threaded initialization (bootstrapping).
  - **Serialization**: It uses a global `Mutex` to prevent concurrent mutable accesses, but it does **not** protect against concurrent readers.

## Related designs: qcell and frankencell (what we adopt, what we reject)

This crate’s core safety story is **lifetime branding + linear capability** (GhostToken). Two related families are worth
calling out:

- `qcell`:
  - Provides several “owner-gated cell” variants. Of particular interest is the *owner gate* pattern: use a **single**
    owner/capability to access many cells, instead of locking each cell independently.
  - We adopt this *pattern* in a **lock-free** way: our preferred approach is to use scoped concurrency and share
    `&GhostToken<'brand>` for parallel read-only access (and baton-pass `GhostToken<'brand>` for exclusive writes),
    rather than wrapping the token in a lock.
- `frankencell`:
  - Uses **const-generic IDs** (`const ID: usize`) as a stand-in for generative brands. This can avoid closure-based
    generativity, but requires an ID-uniqueness story (e.g., a builder) and makes cross-crate passing inherently tricky.
  - We reject this approach for our core API: the brand must be *fresh and unforgeable* in safe code; lifetime branding
    provides that property directly, whereas const IDs require extra machinery and are easy to misuse.

## Benchmarking invariants

To validate “minimal overhead”, benchmarks must:

- prevent the optimizer from constant-folding/hoisting the measurement,
- create real data dependencies (e.g., mutation + accumulation),
- compare like-for-like primitives:
  - `GhostUnsafeCell` vs `UnsafeCell`
  - `GhostCell` vs `RefCell` / `Cell` (depending on operation)
  - `GhostOnceCell` vs `std::cell::OnceCell`
  - `GhostLazyLock` vs `std::sync::LazyLock` (closest conceptual match; thread-safe)

## References

- [GhostCell paper (RustBelt)](https://plv.mpi-sws.org/rustbelt/ghostcell/paper.pdf)
 - [`AtomicBitSet` (hibitset)](https://docs.rs/hibitset/latest/hibitset/struct.AtomicBitSet.html)
 - [`AtomicBitSet` (uniset)](https://docs.rs/uniset/latest/uniset/struct.AtomicBitSet.html)
 - [`qcell` crate docs](https://docs.rs/qcell/latest/qcell/)
 - [`frankencell` crate docs](https://docs.rs/frankencell/latest/frankencell/)
 - [`qcell` soundness advisory (historical)](https://rustsec.org/advisories/RUSTSEC-2022-0007.html)


