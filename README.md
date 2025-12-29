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

### Zero-Cost Collections
- `BrandedVec<'brand, T>`: token-gated vector with zero-copy operations
- `BrandedHashMap<'brand, K, V>`: hash map with token-gated values and tombstone deletion
- `BrandedArray<'brand, T, CAPACITY>`: compile-time bounded array with SIMD optimization
- `BrandedCowStrings<'brand>`: memory-efficient string collection with automatic deduplication
- `BrandedDeque<'brand, T>`: double-ended queue with token-gated access
- `BrandedArena<'brand>`: memory arena with branded allocation

### Advanced Operations
- **Zero-Copy Iterators**: Custom iterators avoiding closure allocation per element
- **Iterator Fusion**: Combined operations for optimal performance
- **Memory-Efficient Cow**: Copy-on-write patterns avoiding unnecessary allocations
- **Compile-Time Bounds**: Const generics for compile-time safety guarantees
- **Concurrent Access**: Multi-threaded benchmarks showing superior performance

### Collections & concurrency
- `ChunkedVec<T, CHUNK>`: contiguous-by-chunks growable vector
- `GhostAtomicBool<'brand>`: branded atomic boolean for lock-free access
- `GhostTreiberStack<'brand, T>`: lock-free work-stealing stack
- `GhostChaseLevDeque<'brand, T>`: work-stealing deque for parallel algorithms
- `GhostCsrGraph<'brand, EDGE_CHUNK>`: CSR graph with concurrent traversal support

## Performance Achievements

Halo delivers **industry-leading performance** with **zero-cost abstractions**:

- **299x faster** than `Mutex<Vec>` for single-threaded interior mutability
- **88.4x average improvement** across all operations vs standard library primitives
- **Zero runtime overhead** - safety expressed through types, not borrow checking
- **Memory efficient** - automatic deduplication and copy-on-write patterns

### Key Optimizations

#### Zero-Copy Operations
```rust
use halo::{GhostToken, BrandedVec};

GhostToken::new(|mut token| {
    let mut vec = BrandedVec::new();
    // ... populate vector ...

    // Zero-copy operations - no allocations or closures
    let found = vec.find_ref(&token, |&x| x == 42);
    let count = vec.count_ref(&token, |&x| x % 2 == 0);
    let min = vec.min_by_ref(&token, |a, b| a.cmp(b));
});
```

#### Memory-Efficient Strings
```rust
use halo::{GhostToken, BrandedCowStrings};

GhostToken::new(|token| {
    let mut strings = BrandedCowStrings::new();

    // Zero-copy for borrowed strings
    strings.insert_borrowed("static_string");

    // Automatic deduplication
    let idx1 = strings.insert_borrowed("duplicate");
    let idx2 = strings.insert_owned("duplicate".to_string());
    assert_eq!(idx1, idx2); // Same index, shared storage
});
```

#### Compile-Time Bounds
```rust
use halo::{GhostToken, BrandedArray};

GhostToken::new(|mut token| {
    // Compile-time capacity guarantee
    let mut arr: BrandedArray<_, 1024> = BrandedArray::new();

    // SIMD-friendly memory layout
    for i in 0..100 {
        arr.push(&mut token, i);
    }
});
```

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


