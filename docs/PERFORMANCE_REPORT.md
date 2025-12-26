# Performance report (Ghost vs std)

This document records the current benchmark ratios comparing Ghost primitives to close stdlib equivalents.

## How this report was produced

- Run:
  - `cargo bench`
  - `cargo run --example bench_report -- --stat median --threshold 1.05`
- The table below is copied from `bench_report` output.

## Ratio summary (median, lower is better)

```
comparison                                                    ghost(ns)      std(ns)      ratio
------------------------------------------------------------------------------------------------
GhostUnsafeCell get_mut loop vs UnsafeCell get_mut loop        0.116848     0.223717     0.5223
GhostCell inc loop vs RefCell inc loop                         0.113881     0.115280     0.9879
GhostLazyLock cached get vs std::sync::LazyLock cached get     0.226285     0.369023     0.6132
GhostLazyLock first get vs std::sync::LazyLock first get       0.271577    16.353568     0.0166
GhostOnceCell get vs std::cell::OnceCell get                  29.741181    29.878159     0.9954
GhostOnceCell set vs std::cell::OnceCell set                   0.231485     0.254334     0.9102
GhostAtomicU64 fetch_add vs AtomicU64 fetch_add             5048.314124  4958.033554     1.0182
ChunkedVec push+iter sum vs Vec push+iter sum              72737.685560 154271.753247     0.4715
Ghost CSR DFS (atomic visited) vs std CSR DFS (AtomicBool visited) 70521.826682 72705.635665     0.9700
Scoped fan-out read: GhostToken vs RwLock                  191313.467586 198502.160279     0.9638
Many-cells read: scoped GhostToken vs per-cell RwLock      217375.250494 7156350.000000     0.0304
Many-cells write: batched commit (GhostToken) vs per-cell RwLock write 273147.478514 6758437.500000     0.0404
Scoped baton write: GhostToken vs Mutex                    400034.335917 422910.327706     0.9459
Parallel reachability: lock-free worklist vs Mutex worklist 1696696.661852 2292086.842105     0.7402
Parallel reachability (batched): lock-free batched vs Mutex worklist 1096853.977273 2292086.842105     0.4785
Parallel reachability: bitset visited vs AtomicBool visited (both lock-free batched) 865696.730769 1096853.977273     0.7893
High-contention reachability: Chase–Lev deque vs batched Treiber stack 3812532.142857 6067127.777778     0.6284
```

## Interpretation notes

- The multithreaded wins are driven by the Ghost paradigm’s key property: **one capability gates many cells**.
  - Reads: share `&GhostToken` across threads.
  - Writes: compute in parallel under `&GhostToken`, then apply a compact batch under `&mut GhostToken`
    (see `concurrency::scoped::parallel_read_then_commit`).
- Microbench results are amortized to avoid the “ps-noise” regime; see `docs/BENCHMARKING.md`.







