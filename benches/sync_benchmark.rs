use criterion::{criterion_group, criterion_main, Criterion};
use halo::{GhostToken, concurrency::sync::GhostRingBuffer};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::thread;
use std::sync::atomic::{AtomicUsize, Ordering};

fn bench_mpmc(c: &mut Criterion) {
    let capacity = 512;
    let total_items = 10_000;
    let producer_count = 2;
    let consumer_count = 2;
    let items_per_producer = total_items / producer_count;

    let mut group = c.benchmark_group("mpmc");

    group.bench_function("ghost_ring_buffer", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let queue = Arc::new(GhostRingBuffer::new(capacity));
                let consumed = Arc::new(AtomicUsize::new(0));

                thread::scope(|s| {
                    // Producers
                    for _ in 0..producer_count {
                        let q = queue.clone();
                        s.spawn(move || {
                            for i in 0..items_per_producer {
                                while q.try_push(i).is_err() {
                                    thread::yield_now();
                                }
                            }
                        });
                    }
                    // Consumers
                    for _ in 0..consumer_count {
                        let q = queue.clone();
                        let c = consumed.clone();
                        s.spawn(move || {
                             while c.load(Ordering::Relaxed) < total_items {
                                 if let Some(_) = q.try_pop() {
                                     c.fetch_add(1, Ordering::Relaxed);
                                 } else {
                                     thread::yield_now();
                                 }
                             }
                        });
                    }
                });
            })
        })
    });

    group.bench_function("mutex_vec_deque", |b| {
        b.iter(|| {
            let queue = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));
            let consumed = Arc::new(AtomicUsize::new(0));

             thread::scope(|s| {
                // Producers
                for _ in 0..producer_count {
                    let q = queue.clone();
                    s.spawn(move || {
                        for i in 0..items_per_producer {
                            loop {
                                {
                                    let mut g = q.lock().unwrap();
                                    if g.len() < capacity {
                                        g.push_back(i);
                                        break;
                                    }
                                }
                                thread::yield_now();
                            }
                        }
                    });
                }
                // Consumers
                for _ in 0..consumer_count {
                    let q = queue.clone();
                    let c = consumed.clone();
                    s.spawn(move || {
                         while c.load(Ordering::Relaxed) < total_items {
                             let val = {
                                 let mut g = q.lock().unwrap();
                                 g.pop_front()
                             };
                             if let Some(_) = val {
                                 c.fetch_add(1, Ordering::Relaxed);
                             } else {
                                 thread::yield_now();
                             }
                         }
                    });
                }
            });
        })
    });

    group.finish();
}

criterion_group!(benches, bench_mpmc);
criterion_main!(benches);
