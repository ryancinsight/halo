# Run + benchmark results (local)

Date: 2025-12-21  
Host: Windows 10 (x86_64-pc-windows-gnu)

## Toolchain

```text
rustc 1.92.0-nightly (4068bafed 2025-10-20)
cargo 1.92.0-nightly (367fd9f21 2025-10-15)
LLVM version: 21.1.3
```

## Examples

All examples executed successfully:

- `cargo run --example basic_usage`
- `cargo run --example branded_vec`
- `cargo run --example lazy_usage`
- `cargo run --example linked_list_arc_rwlock`
- `cargo run --example linked_list_arc_ghostcell`
- `cargo run --example refcell_comparison`
- `cargo run --example scoped_threads`

### `bench_report` (criterion ratio gate)

Command:

```text
cargo run --example bench_report
```

Result: **OK** (all ratios within threshold 1.05).

```text
comparison                                                    ghost(ns)      std(ns)      ratio
------------------------------------------------------------------------------------------------
GhostUnsafeCell get_mut loop vs UnsafeCell get_mut loop      243.983931   243.692469     1.0012
GhostCell inc loop vs RefCell inc loop                       204.038435   201.723047     1.0115
GhostLazyLock cached get vs std::sync::LazyLock cached get     0.244881     0.366197     0.6687
GhostLazyLock first get vs std::sync::LazyLock first get       0.543414    17.494235     0.0311
GhostOnceCell get vs std::cell::OnceCell get                   0.111757     0.121688     0.9184
GhostOnceCell set vs std::cell::OnceCell set                   0.220464     0.245055     0.8997
GhostAtomicU64 fetch_add vs AtomicU64 fetch_add             4595.947341  4522.092489     1.0163
ChunkedVec push+iter sum vs Vec push+iter sum              67804.653242 237694.617751     0.2853
Ghost CSR DFS (atomic visited) vs std CSR DFS (AtomicBool visited) 69497.438640 70649.077210     0.9837
Scoped fan-out read: GhostToken vs RwLock                  195746.985038 208950.476449     0.9368
Many-cells read: scoped GhostToken vs per-cell RwLock      223421.653997 7187513.750000     0.0311
Many-cells write: batched commit (GhostToken) vs per-cell RwLock write 273380.371681 6801551.400000     0.0402
Scoped baton write: GhostToken vs Mutex                    2953656.576688 3100635.536005     0.9526
Parallel reachability: lock-free worklist vs Mutex worklist 1699030.533596 2204526.979715     0.7707
Parallel reachability (batched): lock-free batched vs Mutex worklist 1197252.829495 2204526.979715     0.5431
Parallel reachability: bitset visited vs AtomicBool visited (both lock-free batched) 984234.705352 1197252.829495     0.8221
High-contention reachability: Chase–Lev deque vs batched Treiber stack 3552067.145985 6022253.422500     0.5898
```

## Benchmarks (criterion artifacts)

Bench suite:

```text
cargo bench --bench halo_benchmark
```

Artifacts are under:

- `target/criterion/`
- `target/criterion/report/index.html`

## Changes made to improve correctness + measurement validity

- Fixed `examples/lazy_usage.rs` debug-overflow (now uses wrapping arithmetic) and removed divide-by-zero output for 0ns cached reads.
- Fixed benchmark loop-collapse by using `black_box` inside increment loops so the benchmark measures real work (not an optimized-away closed form).

## Changes made to improve performance / scalability

- Optimized `GhostAtomicBitset` bit indexing to use shifts/masks instead of division/mod on the hot path.
- Reworked CSR bitset-based parallel reachability to group neighbor updates by bitset-word (reduces atomic contention from word sharing).


