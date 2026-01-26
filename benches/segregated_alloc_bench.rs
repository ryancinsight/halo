use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::alloc::segregated::manager::{SizeClassManager, ThreadLocalCache};
use halo::alloc::segregated::size_class::SC;
use halo::GhostToken;
use halo::token::shared::SharedGhostToken;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

fn bench_single_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("Segregated Alloc Single Thread");
    group.sample_size(10);
    const BATCH: usize = 100;

    group.bench_function("SizeClassManager Alloc/Free", |b| {
        GhostToken::new(|token| {
            let manager = SizeClassManager::<'_, SC<32>, 32, 64>::new();
            b.iter(|| {
                let mut ptrs = Vec::with_capacity(BATCH);
                for _ in 0..BATCH {
                    ptrs.push(manager.alloc(&token).unwrap());
                }
                for ptr in ptrs {
                    unsafe { manager.free(&token, ptr); }
                }
            });
        });
    });

    group.bench_function("ThreadLocalCache Fill/Flush", |b| {
        GhostToken::new(|token| {
            let manager = SizeClassManager::<'_, SC<32>, 32, 64>::new();
            let mut cache = ThreadLocalCache::<'_, SC<32>>::new(BATCH);
            b.iter(|| {
                cache.fill(&manager, &token, BATCH);
                black_box(&cache);
                cache.flush(&manager, &token);
            });
        });
    });

    group.bench_function("Mutex<Vec> Alloc/Free", |b| {
         let m = Mutex::new(Vec::with_capacity(BATCH));
         b.iter(|| {
             let mut g = m.lock().unwrap();
             for _ in 0..BATCH {
                 g.push(Box::new(0u8));
             }
             g.clear(); // Free
         });
    });
}

fn bench_multi_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("Segregated Alloc Multi Thread Contention");
    group.sample_size(10);
    const THREADS: usize = 2;
    const OPS: usize = 100;

    group.bench_function("SizeClassManager", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                GhostToken::new(|token| {
                    let manager = SizeClassManager::<'_, SC<32>, 32, 64>::new();
                    let manager_ref = &manager;
                    let shared_token = SharedGhostToken::new(token);
                    let token_ref = &shared_token;

                    thread::scope(|s| {
                        for _ in 0..THREADS {
                            s.spawn(move || {
                                let guard = token_ref.read();
                                let mut cache = ThreadLocalCache::<'_, SC<32>>::new(OPS);
                                // Fill/Flush loop (100 batches of 10)
                                for _ in 0..100 {
                                    cache.fill(manager_ref, &*guard, 10);
                                    cache.flush(manager_ref, &*guard);
                                }
                            });
                        }
                    });
                });
            }
            start.elapsed()
        })
    });

    group.bench_function("Mutex Baseline", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let m = Arc::new(Mutex::new(Vec::new()));
                let m_ref = &m;
                thread::scope(|s| {
                    for _ in 0..THREADS {
                        s.spawn(move || {
                             for _ in 0..100 {
                                 let mut g = m_ref.lock().unwrap();
                                 for _ in 0..10 {
                                     g.push(Box::new(0u8));
                                 }
                                 // Simulate free by clearing
                                 g.clear();
                             }
                        });
                    }
                });
            }
            start.elapsed()
        })
    });
}

criterion_group!(benches, bench_single_thread, bench_multi_thread);
criterion_main!(benches);
