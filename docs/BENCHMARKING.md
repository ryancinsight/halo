# Benchmarking workflow (iterative optimization)

This crate uses Criterion to benchmark Ghost primitives against close stdlib equivalents and to iterate on performance changes without regressions.

## Quick workflow

1. Run benchmarks:

```bash
cargo bench
```

2. Compute Ghost/std ratios (and optionally fail if slower beyond a threshold):

```bash
cargo run --example bench_report -- --threshold 1.05
```

You can also choose which statistic to gate on:

```bash
cargo run --example bench_report -- --stat median --threshold 1.05
```

## Notes on correctness of measurements

- Many operations here are **sub-nanosecond**. Benchmarks therefore:
  - amortize work per iteration (e.g. multiple increments per `b.iter`)
  - enforce real data dependency (mutation + readback)
  - compare “like-for-like” loops (same work, same structure)

If you change a benchmark, keep the stdlib baseline structurally identical; otherwise you will measure differences in benchmark scaffolding instead of the cell primitive.

### Microbench stabilization rule (sub-ns ops)

If a benchmark measures in the **picosecond** range, small system noise can dominate.
Before treating a ratio gate trip as a real regression:

- Increase the *amortized work per iteration* (e.g., raise `INC_REPS` for the paired kernels).
- Ensure Ghost and std baselines have symmetric loop structure and equivalent `black_box` placement.
- Re-run with `--stat median` (less sensitive to outliers) and consider deleting `target/criterion` if you recently renamed benchmarks.

## Multi-thread benchmarks (where Ghost beats per-item locks)

Two families matter for “reduce multithreading overhead”:

- **Many-cells read**: compares per-item locking (`Arc<RwLock<T>>` per cell) against ghost token sharing
  (`Arc<GhostCell<T>>` per cell + shared `&GhostToken` via `concurrency::scoped::with_read_scope`).
- **Many-cells write (lock-free)**: compares per-item write locking (`Arc<RwLock<T>>` per cell) against a
  **two-phase** ghost pattern:
  - parallel compute under shared `&GhostToken` (read-only)
  - sequential batched commit under `&mut GhostToken`

These are intentionally designed to model real operations that touch many items, where per-item locks amplify overhead.

## Cleaning stale Criterion output

Criterion output accumulates under `target/criterion`. If you renamed/removed benchmarks, stale results can remain on disk.

When in doubt, delete the directory and re-run:

```bash
rm -rf target/criterion
cargo bench
```

On Windows you can simply delete `target\\criterion` in Explorer.


