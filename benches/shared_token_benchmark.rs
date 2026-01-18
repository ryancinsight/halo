use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use halo::{GhostToken, SharedGhostToken, BrandedHashMap};
use std::sync::{Arc, RwLock};
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

            b.iter(|| {
                thread::scope(|s| {
                    for _ in 0..8 {
                        let map = map.clone();
                        let token = token.clone();
                        s.spawn(move || {
                            let guard = token.read();
                            for i in 0..100 {
                                let _ = map.get(&guard, &(i * 10));
                            }
                        });
                    }
                });
            });
        });
    });

    group.bench_function("read_heavy_std_rwlock", |b| {
        let mut map = HashMap::new();
        for i in 0..1000 {
            map.insert(i, i);
        }
        let map = Arc::new(RwLock::new(map));

        b.iter(|| {
            thread::scope(|s| {
                for _ in 0..8 {
                    let map = map.clone();
                    s.spawn(move || {
                        let guard = map.read().unwrap();
                        for i in 0..100 {
                            let _ = guard.get(&(i * 10));
                        }
                    });
                }
            });
        });
    });

    // Write-heavy workload (1 writer, multiple readers)
    group.bench_function("mixed_workload_branded", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedHashMap::new();
            for i in 0..1000 {
                map.insert(i, i);
            }
            let map = Arc::new(map);
            let token = Arc::new(SharedGhostToken::new(token));

            b.iter(|| {
                thread::scope(|s| {
                    // Writer
                    let map_w = map.clone();
                    let token_w = token.clone();
                    s.spawn(move || {
                        let mut guard = token_w.write();
                        for i in 0..10 {
                            if let Some(val) = map_w.get_mut(&mut guard, &(i * 100)) {
                                *val += 1;
                            }
                        }
                    });

                    // Readers
                    for _ in 0..4 {
                        let map = map.clone();
                        let token = token.clone();
                        s.spawn(move || {
                            let guard = token.read();
                            for i in 0..50 {
                                let _ = map.get(&guard, &(i * 20));
                            }
                        });
                    }
                });
            });
        });
    });

    group.bench_function("mixed_workload_std_rwlock", |b| {
        let mut map = HashMap::new();
        for i in 0..1000 {
            map.insert(i, i);
        }
        let map = Arc::new(RwLock::new(map));

        b.iter(|| {
            thread::scope(|s| {
                // Writer
                let map_w = map.clone();
                s.spawn(move || {
                    let mut guard = map_w.write().unwrap();
                    for i in 0..10 {
                        if let Some(val) = guard.get_mut(&(i * 100)) {
                            *val += 1;
                        }
                    }
                });

                // Readers
                for _ in 0..4 {
                    let map = map.clone();
                    s.spawn(move || {
                        let guard = map.read().unwrap();
                        for i in 0..50 {
                            let _ = guard.get(&(i * 20));
                        }
                    });
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_shared_token);
criterion_main!(benches);
