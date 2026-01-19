//! Performance benchmarks comparing GhostCell with standard approaches

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use halo::*;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Mutex, RwLock};
use std::thread;

// Keep these micro-benchmarks out of the "ps-level noise" regime.
// (This is intentionally large so Ghost-vs-stdlib ratios are stable.)
const INC_REPS: u64 = 1024;

#[inline(always)]
fn ghostcell_inc<'brand>(cell: &GhostCell<'brand, u64>, token: &mut GhostToken<'brand>) -> u64 {
    let p = cell.borrow_mut(token);
    for _ in 0..INC_REPS {
        *p = p.wrapping_add(1);
    }
    *p
}

#[inline(always)]
fn refcell_inc(cell: &RefCell<u64>) -> u64 {
    let mut p = cell.borrow_mut();
    for _ in 0..INC_REPS {
        *p = p.wrapping_add(1);
    }
    *p
}

#[inline(always)]
unsafe fn unsafe_cell_inc(cell: &UnsafeCell<u64>) -> u64 {
    let p = cell.get();
    for _ in 0..INC_REPS {
        let v = *p;
        *p = v.wrapping_add(1);
    }
    *p
}

#[inline(always)]
fn ghost_unsafe_cell_inc<'brand>(
    cell: &GhostUnsafeCell<'brand, u64>,
    token: &mut GhostToken<'brand>,
) -> u64 {
    let p = cell.as_mut_ptr(token);
    for _ in 0..INC_REPS {
        // SAFETY: `token` is a linear capability, so this benchmark has exclusive access.
        unsafe {
            let v = *p;
            *p = v.wrapping_add(1);
        }
    }
    // SAFETY: same as above.
    unsafe { *p }
}

fn bench_ghostcell_basic(c: &mut Criterion) {
    c.bench_function("ghostcell_basic_borrow", |b| {
        GhostToken::new(|token| {
            let cell = GhostCell::new(black_box(42));

            b.iter(|| {
                let value = cell.borrow(&token);
                black_box(*value);
            });
        });
    });

    c.bench_function("ghostcell_basic_mutate", |b| {
        GhostToken::new(|mut token| {
            let cell = GhostCell::new(black_box(42));

            b.iter(|| {
                *cell.borrow_mut(&mut token) = black_box(43);
            });
        });
    });
}

fn bench_comparison_with_std(c: &mut Criterion) {
    // Compare with raw UnsafeCell (like-for-like foundation)
    c.bench_function("unsafe_cell_get_mut", |b| {
        let cell = UnsafeCell::new(black_box(0u64));
        b.iter(|| {
            // SAFETY: this benchmark is single-threaded and does not create aliases.
            unsafe { black_box(unsafe_cell_inc(&cell)) }
        });
    });

    c.bench_function("ghost_unsafe_cell_get_mut_loop", |b| {
        GhostToken::new(|mut token| {
            let cell = GhostUnsafeCell::new(black_box(0u64));
            b.iter(|| {
                black_box(ghost_unsafe_cell_inc(&cell, &mut token));
            });
        });
    });

    // Compare with RefCell
    c.bench_function("refcell_borrow", |b| {
        let cell = RefCell::new(black_box(42));

        b.iter(|| {
            let value = cell.borrow();
            black_box(*value);
        });
    });

    c.bench_function("refcell_mutate", |b| {
        let cell = RefCell::new(black_box(42));

        b.iter(|| {
            *cell.borrow_mut() = black_box(43);
        });
    });

    // Compare with Cell
    c.bench_function("cell_get_set", |b| {
        let cell = Cell::new(black_box(42));

        b.iter(|| {
            let value = cell.get();
            cell.set(black_box(value + 1));
        });
    });

    // A mutation-heavy loop to force data dependency (avoids "ps-level nonsense").
    c.bench_function("ghostcell_copy_ops_wrapping", |b| {
        GhostToken::new(|mut token| {
            let cell = GhostCell::new(black_box(0u64));

            b.iter(|| {
                black_box(ghostcell_inc(&cell, &mut token));
            });
        });
    });

    c.bench_function("refcell_copy_ops_wrapping", |b| {
        let cell = RefCell::new(black_box(0u64));
        b.iter(|| {
            black_box(refcell_inc(&cell));
        });
    });
}

fn bench_iterator_performance(c: &mut Criterion) {
    let data: Vec<i32> = (0..1000).collect();

    c.bench_function("standard_iterator_map", |b| {
        b.iter(|| {
            let result: Vec<i32> = data.iter().map(|&x| black_box(x * 2)).collect();
            black_box(result);
        });
    });

    // Removed misleading benchmarks: ghostcell_iterator_map and ghostcell_bulk_update
    // These measured allocation overhead, not cell performance
    // For fair comparison, GhostCell operations are already benchmarked individually
}

fn bench_lazy_evaluation(c: &mut Criterion) {
    // Separate first-init from cached-get. Compare to std lazy/once types.
    c.bench_function("ghost_lazy_lock_first_get", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let lazy = GhostLazyLock::new(|| black_box(1234u64));
                black_box(*lazy.get(&mut token));
            })
        });
    });

    c.bench_function("ghost_lazy_lock_cached_get", |b| {
        GhostToken::new(|mut token| {
            let lazy = GhostLazyLock::new(|| black_box(1234u64));
            let _ = lazy.get(&mut token);
            b.iter(|| black_box(*lazy.get(&mut token)));
        });
    });

    // std::sync::LazyLock is the closest conceptual match to GhostLazyLock (lazy init wrapper).
    c.bench_function("std_sync_lazy_lock_first_get", |b| {
        b.iter(|| {
            let lazy = std::sync::LazyLock::new(|| black_box(1234u64));
            black_box(*lazy);
        });
    });

    c.bench_function("std_sync_lazy_lock_cached_get", |b| {
        let lazy = std::sync::LazyLock::new(|| black_box(1234u64));
        let _ = *lazy; // pre-init
        b.iter(|| black_box(*lazy));
    });
}

fn bench_cow_operations(c: &mut Criterion) {
    let large_data = vec![1; 10000];

    c.bench_function("standard_clone_modify", |b| {
        let mut data = large_data.clone();

        b.iter(|| {
            let mut new_vec = data.clone();
            new_vec.push(black_box(42));
            data = new_vec;
        });
    });
}

fn bench_memory_overhead(c: &mut Criterion) {
    c.bench_function("ghostcell_memory_usage", |b| {
        GhostToken::new(|token| {
            let cells: Vec<GhostCell<i32>> =
                (0..10000).map(|i| GhostCell::new(black_box(i))).collect();

            b.iter(|| {
                let _sum: i32 = cells.iter().map(|cell| *cell.borrow(&token)).sum();
            });
        });
    });

    c.bench_function("refcell_memory_usage", |b| {
        let cells: Vec<RefCell<i32>> = (0..10000).map(|i| RefCell::new(black_box(i))).collect();

        b.iter(|| {
            let _sum: i32 = cells.iter().map(|cell| *cell.borrow()).sum();
        });
    });
}

fn bench_concurrency_primitives(c: &mut Criterion) {
    // Compare with RwLock (single-threaded usage)
    // Use direct RwLock for fair comparison - avoid Arc overhead
    c.bench_function("rwlock_read", |b| {
        let lock = RwLock::new(42);
        b.iter(|| {
            let value = lock.read().unwrap();
            black_box(*value);
        });
    });

    c.bench_function("rwlock_write", |b| {
        let lock = RwLock::new(42);
        b.iter(|| {
            let mut value = lock.write().unwrap();
            *value = black_box(43);
        });
    });

    // Compare with Mutex (single-threaded usage)
    // Use direct Mutex for fair comparison
    c.bench_function("mutex_lock", |b| {
        let mutex = Mutex::new(42);
        b.iter(|| {
            let value = mutex.lock().unwrap();
            black_box(*value);
        });
    });

    c.bench_function("mutex_lock_mut", |b| {
        let mutex = Mutex::new(42);
        b.iter(|| {
            let mut value = mutex.lock().unwrap();
            *value = black_box(43);
        });
    });

    // GhostCell for comparison (already zero-cost in single-threaded context)
    c.bench_function("ghostcell_read", |b| {
        GhostToken::new(|token| {
            let cell = GhostCell::new(black_box(42));

            b.iter(|| {
                let value = cell.borrow(&token);
                black_box(*value);
            });
        });
    });

    c.bench_function("ghostcell_write", |b| {
        GhostToken::new(|mut token| {
            let cell = GhostCell::new(black_box(42));

            b.iter(|| {
                *cell.borrow_mut(&mut token) = black_box(43);
            });
        });
    });
}

fn bench_lazy_cells(c: &mut Criterion) {
    // Benchmark GhostLazyCell performance
    c.bench_function("lazy_ghostcell_first_access", |b| {
        GhostToken::new(|mut token| {
            let lazy = GhostLazyCell::new(|| black_box(vec![1, 2, 3, 4, 5]));
            b.iter(|| {
                let len = lazy.get(&mut token).len();
                black_box(len);
            });
        });
    });

    c.bench_function("lazy_ghostcell_cached_access", |b| {
        GhostToken::new(|mut token| {
            let lazy = GhostLazyCell::new(|| black_box(vec![1, 2, 3, 4, 5]));
            let _ = lazy.get(&mut token); // pre-compute
            b.iter(|| {
                let len = lazy.get(&mut token).len();
                black_box(len);
            });
        });
    });

    // Benchmark GhostLazyCell invalidation (with Fn for recomputation)
}

#[inline(never)]
fn std_parallel_reachable_mutex_worklist(
    offsets: &[usize],
    edges: &[usize],
    start: usize,
    threads: usize,
    visited: &[AtomicBool],
    work: &Mutex<Vec<usize>>,
) -> usize {
    for v in visited {
        v.store(false, AtomicOrdering::Relaxed);
    }
    {
        let mut w = work.lock().unwrap();
        w.clear();
        w.push(start);
    }
    visited[start].store(true, AtomicOrdering::Relaxed);

    let count = std::sync::atomic::AtomicUsize::new(0);
    thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let u = {
                    let mut w = work.lock().unwrap();
                    w.pop()
                };
                let Some(u) = u else {
                    break;
                };
                count.fetch_add(1, AtomicOrdering::Relaxed);

                let start_i = offsets[u];
                let end_i = offsets[u + 1];
                let mut i = end_i;
                while i > start_i {
                    i -= 1;
                    let v = edges[i];
                    if !visited[v].swap(true, AtomicOrdering::Relaxed) {
                        let mut w = work.lock().unwrap();
                        w.push(v);
                    }
                }
            });
        }
    });
    count.load(AtomicOrdering::Relaxed)
}

fn bench_parallel_graph_traversal(c: &mut Criterion) {
    const N: usize = 8192;
    const DEG: usize = 4;
    const THREADS: usize = 4;
    const BATCH: usize = 64;

    let mut offsets = Vec::with_capacity(N + 1);
    offsets.push(0);
    for i in 0..N {
        offsets.push(offsets[i] + DEG);
    }

    let mut edges = Vec::with_capacity(N * DEG);
    for i in 0..N {
        for d in 1..=DEG {
            edges.push((i + d) % N);
        }
    }

    c.bench_function("std_parallel_reachable_mutex_worklist", |b| {
        let visited: Vec<AtomicBool> = (0..N).map(|_| AtomicBool::new(false)).collect();
        let work = Mutex::new(Vec::<usize>::with_capacity(N));
        b.iter(|| {
            black_box(std_parallel_reachable_mutex_worklist(
                &offsets, &edges, 0, THREADS, &visited, &work,
            ))
        });
    });

    c.bench_function("ghost_parallel_reachable_lockfree_worklist", |b| {
        GhostToken::new(|_token| {
            let g = halo::GhostCsrGraph::<1024>::from_csr_parts(offsets.clone(), edges.clone());
            let stack: halo::concurrency::worklist::GhostTreiberStack<'_> =
                halo::concurrency::worklist::GhostTreiberStack::new(N);
            b.iter(|| black_box(g.parallel_reachable_count_with_stack(0, THREADS, &stack)));
        });
    });

    c.bench_function("ghost_parallel_reachable_lockfree_worklist_batched", |b| {
        GhostToken::new(|_token| {
            let g = halo::GhostCsrGraph::<1024>::from_csr_parts(offsets.clone(), edges.clone());
            let stack: halo::concurrency::worklist::GhostTreiberStack<'_> =
                halo::concurrency::worklist::GhostTreiberStack::new(N);
            b.iter(|| {
                black_box(g.parallel_reachable_count_batched_with_stack(0, THREADS, &stack, BATCH))
            });
        });
    });

    c.bench_function(
        "ghost_parallel_reachable_lockfree_worklist_batched_bitset",
        |b| {
            GhostToken::new(|_token| {
                let g = halo::GhostCsrGraph::<1024>::from_csr_parts(offsets.clone(), edges.clone());
                let stack: halo::concurrency::worklist::GhostTreiberStack<'_> =
                    halo::concurrency::worklist::GhostTreiberStack::new(N);
                let visited: halo::concurrency::atomic::GhostAtomicBitset<'_> =
                    halo::concurrency::atomic::GhostAtomicBitset::new(N);
                b.iter(|| {
                    black_box(g.parallel_reachable_count_batched_with_stack_bitset(
                        0, THREADS, &stack, BATCH, &visited,
                    ))
                });
            });
        },
    );
}

fn bench_parallel_graph_traversal_high_contention(c: &mut Criterion) {
    const N: usize = 16_384;
    const DEG: usize = 16;
    const THREADS: usize = 8;
    const BATCH: usize = 64;

    let mut offsets = Vec::with_capacity(N + 1);
    offsets.push(0);
    for i in 0..N {
        offsets.push(offsets[i] + DEG);
    }

    let mut edges = Vec::with_capacity(N * DEG);
    for i in 0..N {
        for d in 1..=DEG {
            edges.push((i + d) % N);
        }
    }

    c.bench_function(
        "ghost_parallel_reachable_lockfree_worklist_batched_hi",
        |b| {
            GhostToken::new(|_token| {
                let g = halo::GhostCsrGraph::<1024>::from_csr_parts(offsets.clone(), edges.clone());
                let stack: halo::concurrency::worklist::GhostTreiberStack<'_> =
                    halo::concurrency::worklist::GhostTreiberStack::new(N);
                b.iter(|| {
                    black_box(
                        g.parallel_reachable_count_batched_with_stack(0, THREADS, &stack, BATCH),
                    )
                });
            });
        },
    );

    c.bench_function("ghost_parallel_reachable_workstealing_deque_hi", |b| {
        GhostToken::new(|_token| {
            let g = halo::GhostCsrGraph::<1024>::from_csr_parts(offsets.clone(), edges.clone());
            let cap = N.next_power_of_two();
            let deques: Vec<halo::concurrency::worklist::GhostChaseLevDeque<'_>> = (0..THREADS)
                .map(|_| halo::concurrency::worklist::GhostChaseLevDeque::new(cap))
                .collect();
            b.iter(|| black_box(g.parallel_reachable_count_workstealing_with_deques(0, &deques)));
        });
    });
}

fn bench_unsafe_cells(c: &mut Criterion) {
    // Benchmark GhostUnsafeCell (most direct access)
    c.bench_function("ghost_unsafe_cell_get", |b| {
        GhostToken::new(|token| {
            let cell = GhostUnsafeCell::new(black_box(42));
            b.iter(|| {
                black_box(cell.get(&token));
            });
        });
    });

    c.bench_function("ghost_unsafe_cell_get_mut", |b| {
        GhostToken::new(|mut token| {
            let cell = GhostUnsafeCell::new(black_box(42));
            b.iter(|| {
                let value = cell.get_mut(&mut token);
                *value = black_box(*value + 1);
            });
        });
    });

    c.bench_function("ghost_unsafe_cell_replace", |b| {
        GhostToken::new(|mut token| {
            let cell = GhostUnsafeCell::new(black_box(42));
            b.iter(|| {
                black_box(cell.replace(black_box(43), &mut token));
            });
        });
    });

    // Note: GhostUnsafeCell is now conditionally Send + Sync, so no separate sync benchmarks needed
}

fn bench_once_cells(c: &mut Criterion) {
    // Benchmark GhostOnceCell vs std::cell::OnceCell
    const GET_REPS: usize = 2048;
    c.bench_function("ghost_once_cell_set", |b| {
        GhostToken::new(|mut token| {
            b.iter(|| {
                let cell: GhostOnceCell<i32> = GhostOnceCell::new();
                black_box(cell.set(&mut token, black_box(42)).is_ok());
            });
        });
    });

    c.bench_function("ghost_once_cell_get", |b| {
        GhostToken::new(|mut token| {
            let cell: GhostOnceCell<i32> = GhostOnceCell::new();
            let _ = cell.set(&mut token, 42);

            b.iter(|| {
                let mut acc = 0i32;
                for _ in 0..GET_REPS {
                    // Force a real data dependency (not just an Option<&T> value).
                    acc = acc.wrapping_add(*cell.get(&token).unwrap());
                }
                black_box(acc);
            });
        });
    });

    // Compare with std::cell::OnceCell
    c.bench_function("std_once_cell_set", |b| {
        b.iter(|| {
            let cell = std::cell::OnceCell::new();
            let _ = black_box(cell.set(black_box(42)));
        });
    });

    c.bench_function("std_once_cell_get", |b| {
        let cell = std::cell::OnceCell::new();
        cell.set(42).unwrap();

        b.iter(|| {
            let mut acc = 0i32;
            for _ in 0..GET_REPS {
                acc = acc.wrapping_add(*cell.get().unwrap());
            }
            black_box(acc);
        });
    });
}

fn bench_atomics(c: &mut Criterion) {
    // Wrapper-overhead check: GhostAtomicU64 vs std::sync::atomic::AtomicU64
    const REPS: u64 = 1024;

    c.bench_function("std_atomic_u64_fetch_add", |b| {
        let a = AtomicU64::new(0);
        b.iter(|| {
            for _ in 0..REPS {
                black_box(a.fetch_add(1, AtomicOrdering::Relaxed));
            }
        });
    });

    c.bench_function("ghost_atomic_u64_fetch_add", |b| {
        // brand is compile-time only; operations do not require token.
        let a: halo::concurrency::atomic::GhostAtomicU64<'static> =
            halo::concurrency::atomic::GhostAtomicU64::new(0);
        b.iter(|| {
            for _ in 0..REPS {
                black_box(a.fetch_add(1, AtomicOrdering::Relaxed));
            }
        });
    });
}

fn bench_chunked_vec(c: &mut Criterion) {
    const N: usize = 100_000;

    c.bench_function("std_vec_push_iter_sum", |b| {
        b.iter_batched(
            || Vec::<u64>::with_capacity(N),
            |mut v| {
                for i in 0..N as u64 {
                    v.push(i);
                }
                black_box(v.iter().fold(0u64, |acc, &x| acc.wrapping_add(x)))
            },
            BatchSize::SmallInput,
        );
    });

    c.bench_function("ghost_chunked_vec_push_iter_sum", |b| {
        b.iter_batched(
            || halo::collections::ChunkedVec::<u64, 1024>::new(),
            |mut v| {
                v.reserve(N);
                for i in 0..N as u64 {
                    v.push(i);
                }
                black_box(v.iter().fold(0u64, |acc, &x| acc.wrapping_add(x)))
            },
            BatchSize::SmallInput,
        );
    });
}

#[inline(never)]
fn std_csr_dfs_u8_visited(
    offsets: &[usize],
    edges: &[usize],
    start: usize,
    visited: &mut [u8],
) -> usize {
    let mut stack = Vec::new();
    let mut count = 0usize;

    visited[start] = 1;
    stack.push(start);

    while let Some(u) = stack.pop() {
        count += 1;
        let start_i = offsets[u];
        let end_i = offsets[u + 1];
        let mut i = end_i;
        while i > start_i {
            i -= 1;
            let v = edges[i];
            if visited[v] == 0 {
                visited[v] = 1;
                stack.push(v);
            }
        }
    }

    count
}

#[inline(never)]
fn std_csr_dfs_atomicbool_visited(
    offsets: &[usize],
    edges: &[usize],
    start: usize,
    visited: &[std::sync::atomic::AtomicBool],
) -> usize {
    let mut stack = Vec::new();
    let mut count = 0usize;

    visited[start].store(true, AtomicOrdering::Relaxed);
    stack.push(start);

    while let Some(u) = stack.pop() {
        count += 1;
        let start_i = offsets[u];
        let end_i = offsets[u + 1];
        let mut i = end_i;
        while i > start_i {
            i -= 1;
            let v = edges[i];
            if !visited[v].swap(true, AtomicOrdering::Relaxed) {
                stack.push(v);
            }
        }
    }

    count
}

fn bench_dfs_csr(c: &mut Criterion) {
    const N: usize = 4096;
    const DEG: usize = 4;

    // Deterministic synthetic graph: each node i connects to (i+1..i+DEG) modulo N.
    let mut offsets = Vec::with_capacity(N + 1);
    offsets.push(0);
    for i in 0..N {
        offsets.push(offsets[i] + DEG);
    }

    let mut edges = Vec::with_capacity(N * DEG);
    for i in 0..N {
        for d in 1..=DEG {
            edges.push((i + d) % N);
        }
    }

    c.bench_function("std_csr_dfs_u8_visited", |b| {
        b.iter_batched(
            || vec![0u8; N],
            |mut visited| black_box(std_csr_dfs_u8_visited(&offsets, &edges, 0, &mut visited)),
            BatchSize::SmallInput,
        );
    });

    c.bench_function("std_csr_dfs_atomicbool_visited", |b| {
        let visited: Vec<std::sync::atomic::AtomicBool> = (0..N)
            .map(|_| std::sync::atomic::AtomicBool::new(false))
            .collect();

        b.iter(|| {
            for f in &visited {
                f.store(false, AtomicOrdering::Relaxed);
            }
            black_box(std_csr_dfs_atomicbool_visited(
                &offsets, &edges, 0, &visited,
            ))
        });
    });

    c.bench_function("ghost_csr_dfs_atomic_visited", |b| {
        GhostToken::new(|_token| {
            let adj: Vec<Vec<usize>> = (0..N)
                .map(|i| (1..=DEG).map(|d| (i + d) % N).collect())
                .collect();
            let g = halo::GhostCsrGraph::<1024>::from_adjacency(&adj);

            b.iter(|| {
                g.reset_visited();
                black_box(g.dfs_count(0))
            });
        });
    });
}

fn bench_scoped_parallel_patterns(c: &mut Criterion) {
    const THREADS: usize = 4;
    const REPS: usize = 1024;
    const CELLS: usize = 128;
    const BATON_RUNS: usize = 8;

    c.bench_function("std_rwlock_read_fanout", |b| {
        let lock = RwLock::new(black_box(1234u64));
        b.iter(|| {
            thread::scope(|s| {
                for _ in 0..THREADS {
                    s.spawn(|| {
                        let mut acc = 0u64;
                        for _ in 0..REPS {
                            let v = *lock.read().unwrap();
                            acc = acc.wrapping_add(v);
                        }
                        black_box(acc);
                    });
                }
            });
        });
    });

    c.bench_function("ghost_scoped_read_fanout", |b| {
        GhostToken::new(|token| {
            let cell = GhostCell::new(black_box(1234u64));
            b.iter(|| {
                halo::concurrency::scoped::with_read_scope(&token, |scope| {
                    for _ in 0..THREADS {
                        scope.spawn(|t| {
                            let mut acc = 0u64;
                            for _ in 0..REPS {
                                acc = acc.wrapping_add(*cell.borrow(t));
                            }
                            black_box(acc);
                        });
                    }
                });
            });
        });
    });

    c.bench_function("std_rwlock_many_cells_read", |b| {
        let cells: Vec<std::sync::Arc<RwLock<u64>>> = (0..CELLS)
            .map(|i| std::sync::Arc::new(RwLock::new(black_box(i as u64))))
            .collect();

        b.iter(|| {
            thread::scope(|s| {
                for tid in 0..THREADS {
                    let cells = cells.clone();
                    s.spawn(move || {
                        let mut acc = 0u64;
                        for _ in 0..REPS {
                            // Touch many items per iteration; this is where
                            // per-item locking gets expensive.
                            for j in 0..CELLS {
                                let v = *cells[(tid + j) & (CELLS - 1)].read().unwrap();
                                acc = acc.wrapping_add(v);
                            }
                        }
                        black_box(acc);
                    });
                }
            });
        });
    });

    c.bench_function("ghost_scoped_many_cells_read", |b| {
        GhostToken::new(|token| {
            let cells: Vec<std::sync::Arc<GhostCell<'_, u64>>> = (0..CELLS)
                .map(|i| std::sync::Arc::new(GhostCell::new(black_box(i as u64))))
                .collect();

            b.iter(|| {
                halo::concurrency::scoped::with_read_scope(&token, |scope| {
                    for tid in 0..THREADS {
                        let cells = cells.clone();
                        scope.spawn(move |t| {
                            let mut acc = 0u64;
                            for _ in 0..REPS {
                                for j in 0..CELLS {
                                    acc =
                                        acc.wrapping_add(*cells[(tid + j) & (CELLS - 1)].borrow(t));
                                }
                            }
                            black_box(acc);
                        });
                    }
                });
            });
        });
    });

    c.bench_function("std_rwlock_many_cells_write", |b| {
        let cells: Vec<std::sync::Arc<RwLock<u64>>> = (0..CELLS)
            .map(|i| std::sync::Arc::new(RwLock::new(black_box(i as u64))))
            .collect();

        b.iter(|| {
            thread::scope(|s| {
                for tid in 0..THREADS {
                    let cells = cells.clone();
                    s.spawn(move || {
                        for _ in 0..REPS {
                            for j in 0..CELLS {
                                let idx = (tid + j) & (CELLS - 1);
                                let mut g = cells[idx].write().unwrap();
                                *g = g.wrapping_add(1);
                            }
                        }
                    });
                }
            });
            // Consume a value to keep the writes observable.
            black_box(*cells[0].read().unwrap());
        });
    });

    c.bench_function("ghost_scoped_many_cells_write_batched_commit", |b| {
        GhostToken::new(|mut token| {
            let cells: Vec<std::sync::Arc<GhostCell<'_, u64>>> = (0..CELLS)
                .map(|i| std::sync::Arc::new(GhostCell::new(black_box(i as u64))))
                .collect();

            b.iter(|| {
                let out = halo::concurrency::scoped::parallel_read_then_commit(
                    &mut token,
                    THREADS,
                    |t, tid| {
                        let mut deltas = [0u64; CELLS];
                        for _ in 0..REPS {
                            for j in 0..CELLS {
                                let idx = (tid + j) & (CELLS - 1);
                                // Make the compute phase data-dependent on the current state.
                                let v = *cells[idx].borrow(t);
                                deltas[idx] = deltas[idx].wrapping_add(1 + (v & 1));
                            }
                        }
                        deltas
                    },
                    |tt, deltas_per_thread| {
                        for idx in 0..CELLS {
                            let mut total = 0u64;
                            for d in &deltas_per_thread {
                                total = total.wrapping_add(d[idx]);
                            }
                            let p = cells[idx].borrow_mut(tt);
                            *p = p.wrapping_add(total);
                        }
                        black_box(*cells[0].borrow(tt))
                    },
                );
                black_box(out);
            });
        });
    });

    c.bench_function("std_mutex_baton_write", |b| {
        b.iter(|| {
            // Repeat multiple times per measurement to reduce scheduler noise.
            let mut out = 0u64;
            for _ in 0..BATON_RUNS {
                let m = Mutex::new(black_box(0u64));
                // Sequential baton: spawn, lock+mutate, join, repeat.
                thread::scope(|s| {
                    for _ in 0..THREADS {
                        let h = s.spawn(|| {
                            for _ in 0..REPS {
                                let mut g = m.lock().unwrap();
                                *g = g.wrapping_add(1);
                            }
                        });
                        h.join().unwrap();
                    }
                });
                out = out.wrapping_add(*m.lock().unwrap());
            }
            black_box(out);
        });
    });

    c.bench_function("ghost_scoped_baton_write", |b| {
        b.iter(|| {
            // Repeat multiple times per measurement to reduce scheduler noise.
            let mut out = 0u64;
            for _ in 0..BATON_RUNS {
                GhostToken::new(|token| {
                    let cell = GhostCell::new(black_box(0u64));
                    let (_unit, returned_token) =
                        halo::concurrency::scoped::with_write_scope(token, |scope, t| {
                            // Sequential baton passing through scoped threads.
                            let mut token = t;
                            for _ in 0..THREADS {
                                let h = scope.spawn_with_token(token, |tt| {
                                    for _ in 0..REPS {
                                        let p = cell.borrow_mut(tt);
                                        *p = p.wrapping_add(1);
                                    }
                                });
                                let (_out, t2) = h.join().unwrap();
                                token = t2;
                            }
                            ((), token)
                        });
                    out = out.wrapping_add(*cell.borrow(&returned_token));
                });
            }
            black_box(out);
        });
    });
}

criterion_group!(
    benches,
    bench_ghostcell_basic,
    bench_comparison_with_std,
    bench_iterator_performance,
    bench_lazy_evaluation,
    bench_cow_operations,
    bench_memory_overhead,
    bench_concurrency_primitives,
    bench_scoped_parallel_patterns,
    bench_lazy_cells,
    bench_once_cells,
    bench_atomics,
    bench_chunked_vec,
    bench_dfs_csr,
    bench_parallel_graph_traversal,
    bench_parallel_graph_traversal_high_contention,
    bench_unsafe_cells
);
criterion_main!(benches);
