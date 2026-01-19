use criterion::{criterion_group, criterion_main, Criterion};
use halo::{concurrency::sync::GhostRingBuffer, GhostToken};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn bench_mpmc_contended(c: &mut Criterion) {
    let capacity = 128; // Small capacity to force contention
    let items = 1000; // Items per thread
    let threads = 4;

    c.bench_function("ghost_ring_buffer_mpmc_contended", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let q = Arc::new(GhostRingBuffer::<usize>::new(capacity));

                thread::scope(|s| {
                    // Producers
                    for _ in 0..threads {
                        let q = q.clone();
                        s.spawn(move || {
                            for i in 0..items {
                                while q.try_push(i).is_err() {
                                    std::hint::spin_loop();
                                }
                            }
                        });
                    }

                    // Consumers
                    for _ in 0..threads {
                        let q = q.clone();
                        s.spawn(move || {
                            let mut count = 0;
                            while count < items {
                                if q.try_pop().is_some() {
                                    count += 1;
                                } else {
                                    std::hint::spin_loop();
                                }
                            }
                        });
                    }
                });
            });
        });
    });

    c.bench_function("mutex_vec_deque_mpmc_contended", |b| {
        b.iter(|| {
            let q = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));

            thread::scope(|s| {
                // Producers
                for _ in 0..threads {
                    let q = q.clone();
                    s.spawn(move || {
                        for i in 0..items {
                            loop {
                                // Minimize critical section
                                let mut guard = q.lock().unwrap();
                                if guard.len() < capacity {
                                    guard.push_back(i);
                                    drop(guard);
                                    break;
                                }
                                drop(guard);
                                std::hint::spin_loop();
                            }
                        }
                    });
                }

                // Consumers
                for _ in 0..threads {
                    let q = q.clone();
                    s.spawn(move || {
                        let mut count = 0;
                        while count < items {
                            let mut guard = q.lock().unwrap();
                            if guard.pop_front().is_some() {
                                count += 1;
                                drop(guard);
                            } else {
                                drop(guard);
                                std::hint::spin_loop();
                            }
                        }
                    });
                }
            });
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = bench_mpmc_contended
}
criterion_main!(benches);
