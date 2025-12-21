# Halo — protective data structure toolkit

[![Crates.io](https://img.shields.io/crates/v/halo.svg)](https://crates.io/crates/halo)
[![Documentation](https://docs.rs/halo/badge.svg)](https://docs.rs/halo)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ryanclanton/halo)

A protective toolkit for safe, high-performance data structures and concurrency primitives using **ghost tokens**. Provides memory-efficient interior mutability with a subtle layer of type safety - permissions separated from data via **linear, branded tokens**.

## Design goals

- **Protective safety**: ghost tokens create protective boundaries around data access
- **Zero runtime overhead**: safety expressed through types, not borrow checking
- **Comprehensive toolkit**: cells, collections, concurrency, and graph primitives
- **Stratified design**: foundation → ergonomic APIs → domain-specific types

## Quick start

```rust
use halo::{GhostCell, GhostToken};

GhostToken::new(|mut token| {
    let cell = GhostCell::new(42);
    assert_eq!(*cell.borrow(&token), 42);

    *cell.borrow_mut(&mut token) = 100;
    assert_eq!(cell.get(&token), 100);
});
```

## Core toolkit

### Cell primitives
- `GhostToken<'brand>`: the *linear* protective capability gating access (intentionally **not** `Copy`/`Clone`)
- `GhostUnsafeCell<'brand, T>`: minimal raw storage with protective branding
- `GhostCell<'brand, T>`: safe ergonomic cell with token-gated access
- `GhostOnceCell<'brand, T>`: one-time initialization with protective reads
- `GhostLazyLock<'brand, T, F>`: one-time lazy computation (`F: FnOnce() -> T`)
- `GhostLazyCell<'brand, T, F>`: recomputable lazy cache with invalidation

### Collections & concurrency
- `ChunkedVec<T, CHUNK>`: contiguous-by-chunks growable vector
- `GhostAtomicBool<'brand>`: branded atomic boolean for lock-free access
- `GhostTreiberStack<'brand, T>`: lock-free work-stealing stack
- `GhostChaseLevDeque<'brand, T>`: work-stealing deque for parallel algorithms
- `GhostCsrGraph<'brand, EDGE_CHUNK>`: CSR graph with concurrent traversal support

## Benchmarks

Run:

```bash
cargo bench
```

Then run the ratio report / regression gate:

```bash
cargo run --example bench_report -- --threshold 1.05
```

## Documentation

- **Invariants**: see `docs/INVARIANTS.md`
- **Benchmarking**: see `docs/BENCHMARKING.md`
- **UB checking**: see `docs/UB_CHECKING.md`

## References

- [Original GhostCell Paper](https://plv.mpi-sws.org/rustbelt/ghostcell/paper.pdf)
- [RustBelt Project](https://plv.mpi-sws.org/rustbelt/)
- [Formal Verification](https://gitlab.mpi-sws.org/FP/ghostcell)


