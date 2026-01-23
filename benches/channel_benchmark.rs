use criterion::{criterion_group, criterion_main, Criterion};
use halo::{
    concurrency::channel::{mpsc, oneshot},
    GhostToken, SharedGhostToken,
};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn benchmark_mpsc(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpsc_unbounded");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("halo_mpsc", |b| {
        b.iter_custom(|iters| {
            GhostToken::new(|token| {
                let shared_token = Arc::new(SharedGhostToken::new(token));

                // Create channel. Note: we use a temporary guard to create it.
                // The channel persists as long as 'brand exists.
                let (tx, rx) = mpsc::channel(&shared_token.read());

                let tx = Arc::new(tx);
                // rx is moved to consumer

                let start = std::time::Instant::now();

                // Workload: 2 producers sending iters/2 messages. 1 consumer.
                let producer_count = 2;
                let msgs_per_producer = iters / producer_count;
                let barrier = Arc::new(Barrier::new(producer_count as usize + 1));

                thread::scope(|s| {
                    // Producers
                    for _ in 0..producer_count {
                        let tx = tx.clone();
                        let st = shared_token.clone();
                        let b = barrier.clone();
                        s.spawn(move || {
                            b.wait();
                            let guard = st.read();
                            for i in 0..msgs_per_producer {
                                tx.send(&guard, i as usize).unwrap();
                            }
                        });
                    }

                    // Consumer
                    let b = barrier.clone();
                    let st = shared_token.clone();
                    s.spawn(move || {
                        b.wait();
                        let guard = st.read();
                        let mut count = 0;
                        let target = msgs_per_producer * producer_count;
                        while count < target {
                            if let Some(_) = rx.try_recv(&guard) {
                                count += 1;
                            } else {
                                std::thread::yield_now();
                            }
                        }
                    });
                });

                start.elapsed()
            })
        })
    });

    group.bench_function("std_mpsc", |b| {
        b.iter_custom(|iters| {
            let (tx, rx) = std_mpsc::channel();

            let start = std::time::Instant::now();
            let producer_count = 2;
            let msgs_per_producer = iters / producer_count;
            let barrier = Arc::new(Barrier::new(producer_count as usize + 1));

            thread::scope(|s| {
                for _ in 0..producer_count {
                    let tx = tx.clone();
                    let b = barrier.clone();
                    s.spawn(move || {
                        b.wait();
                        for i in 0..msgs_per_producer {
                            tx.send(i as usize).unwrap();
                        }
                    });
                }

                let b = barrier.clone();
                s.spawn(move || {
                    b.wait();
                    let mut count = 0;
                    let target = msgs_per_producer * producer_count;
                    while count < target {
                        if let Ok(_) = rx.try_recv() {
                            count += 1;
                        } else {
                            std::thread::yield_now();
                        }
                    }
                });
            });
            start.elapsed()
        })
    });

    group.finish();
}

fn benchmark_oneshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("oneshot_latency");

    // Measure latency of a single send-recv pair
    group.bench_function("halo_oneshot", |b| {
        b.iter_custom(|iters| {
            GhostToken::new(|token| {
                let shared_token = Arc::new(SharedGhostToken::new(token));
                let start = std::time::Instant::now();

                for _ in 0..iters {
                    let guard = shared_token.read();
                    let (tx, rx) = oneshot::channel(&guard);
                    drop(guard);

                    let st = shared_token.clone();
                    thread::scope(|s| {
                        s.spawn(move || {
                            let guard = st.read();
                            tx.send(&guard, 1).unwrap();
                        });
                        let guard = shared_token.read();
                        rx.recv(&guard).unwrap();
                    });
                }
                start.elapsed()
            })
        })
    });

    group.bench_function("std_oneshot", |b| {
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                let (tx, rx) = std_mpsc::channel();
                thread::scope(|s| {
                    s.spawn(move || {
                        tx.send(1).unwrap();
                    });
                    rx.recv().unwrap();
                });
            }
            start.elapsed()
        })
    });

    group.finish();
}

criterion_group!(benches, benchmark_mpsc, benchmark_oneshot);
criterion_main!(benches);
