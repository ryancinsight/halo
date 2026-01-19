use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use halo::{GhostToken, SharedGhostToken, BrandedHashMap};
use std::sync::{Arc, RwLock, Barrier};
use std::thread;
use std::time::Duration;
use std::collections::HashMap;

fn benchmark_shared_token(c: &mut Criterion) {
    let mut group = c.benchmark_group("shared_token_vs_rwlock");
    group.measurement_time(Duration::from_secs(10));

    // Read-heavy workload
    group.bench_function("read_heavy_branded", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedHashMap::new();
            for i in 0..1000 {
                map.insert(i, i);
            }
            let map = Arc::new(map);
            let token = Arc::new(SharedGhostToken::new(token));

            b.iter_custom(|iters| {
                let thread_count = 8;
                let barrier = Arc::new(Barrier::new(thread_count + 1));

                thread::scope(|s| {
                    let mut handles = Vec::with_capacity(thread_count);
                    for _ in 0..thread_count {
                        let map = map.clone();
                        let token = token.clone();
                        let barrier = barrier.clone();
                        let loops_per_thread = iters / (thread_count as u64);

                        handles.push(s.spawn(move || {
                            barrier.wait(); // Wait for start
                            let start = std::time::Instant::now();
                            for i in 0..loops_per_thread {
                                let guard = token.read();
                                let _ = map.get(&guard, &((i as usize) % 1000));
                            }
                            start.elapsed()
                        }));
                    }

                    barrier.wait(); // Start all threads

                    // Wait for all threads to finish and take the max duration (wall clock time)
                    let mut max_duration = Duration::ZERO;
                    for h in handles {
                        let duration = h.join().unwrap();
                        if duration > max_duration {
                            max_duration = duration;
                        }
                    }
                    max_duration
                })
            });
        });
    });

    group.bench_function("read_heavy_std_rwlock", |b| {
        let mut map = HashMap::new();
        for i in 0..1000 {
            map.insert(i, i);
        }
        let map = Arc::new(RwLock::new(map));

        b.iter_custom(|iters| {
            let thread_count = 8;
            let barrier = Arc::new(Barrier::new(thread_count + 1));

            thread::scope(|s| {
                let mut handles = Vec::with_capacity(thread_count);
                for _ in 0..thread_count {
                    let map = map.clone();
                    let barrier = barrier.clone();
                    let loops = iters / (thread_count as u64);

                    handles.push(s.spawn(move || {
                        barrier.wait();
                        let start = std::time::Instant::now();
                        for i in 0..loops {
                            let guard = map.read().unwrap();
                            let _ = guard.get(&((i as usize) % 1000));
                        }
                        start.elapsed()
                    }));
                }

                barrier.wait();

                let mut max_duration = Duration::ZERO;
                for h in handles {
                    let duration = h.join().unwrap();
                    if duration > max_duration {
                        max_duration = duration;
                    }
                }
                max_duration
            })
        });
    });

    // Write-heavy workload
    group.bench_function("mixed_workload_branded", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedHashMap::new();
            for i in 0..1000 {
                map.insert(i, i);
            }
            let map = Arc::new(map);
            let token = Arc::new(SharedGhostToken::new(token));

            b.iter_custom(|iters| {
                let thread_count = 8;
                let barrier = Arc::new(Barrier::new(thread_count + 1));

                thread::scope(|s| {
                    let mut handles = Vec::with_capacity(thread_count);
                    let loops = iters / (thread_count as u64);

                    // Writer
                    let map_w = map.clone();
                    let token_w = token.clone();
                    let barrier_w = barrier.clone();
                    handles.push(s.spawn(move || {
                        barrier_w.wait();
                        let start = std::time::Instant::now();
                        for i in 0..loops {
                            let mut guard = token_w.write();
                            if let Some(val) = map_w.get_mut(&mut guard, &((i as usize) % 100)) {
                                *val += 1;
                            }
                        }
                        start.elapsed()
                    }));

                    // Readers
                    for _ in 0..(thread_count - 1) {
                        let map = map.clone();
                        let token = token.clone();
                        let barrier = barrier.clone();
                        handles.push(s.spawn(move || {
                            barrier.wait();
                            let start = std::time::Instant::now();
                            for i in 0..loops {
                                let guard = token.read();
                                let _ = map.get(&guard, &((i as usize) % 1000));
                            }
                            start.elapsed()
                        }));
                    }

                    barrier.wait();

                    let mut max_duration = Duration::ZERO;
                    for h in handles {
                        let duration = h.join().unwrap();
                        if duration > max_duration {
                            max_duration = duration;
                        }
                    }
                    max_duration
                })
            });
        });
    });

    group.bench_function("mixed_workload_std_rwlock", |b| {
        let mut map = HashMap::new();
        for i in 0..1000 {
            map.insert(i, i);
        }
        let map = Arc::new(RwLock::new(map));

        b.iter_custom(|iters| {
            let thread_count = 8;
            let barrier = Arc::new(Barrier::new(thread_count + 1));

            thread::scope(|s| {
                let mut handles = Vec::with_capacity(thread_count);
                let loops = iters / (thread_count as u64);

                // Writer
                let map_w = map.clone();
                let barrier_w = barrier.clone();
                handles.push(s.spawn(move || {
                    barrier_w.wait();
                    let start = std::time::Instant::now();
                    for i in 0..loops {
                        let mut guard = map_w.write().unwrap();
                        if let Some(val) = guard.get_mut(&((i as usize) % 100)) {
                            *val += 1;
                        }
                    }
                    start.elapsed()
                }));

                // Readers
                for _ in 0..(thread_count - 1) {
                    let map = map.clone();
                    let barrier = barrier.clone();
                    handles.push(s.spawn(move || {
                        barrier.wait();
                        let start = std::time::Instant::now();
                        for i in 0..loops {
                            let guard = map.read().unwrap();
                            let _ = guard.get(&((i as usize) % 1000));
                        }
                        start.elapsed()
                    }));
                }

                barrier.wait();

                let mut max_duration = Duration::ZERO;
                for h in handles {
                    let duration = h.join().unwrap();
                    if duration > max_duration {
                        max_duration = duration;
                    }
                }
                max_duration
            })
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_shared_token);
criterion_main!(benches);
