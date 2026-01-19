use criterion::{criterion_group, criterion_main, Criterion, black_box};
use halo::concurrency::sync::{GhostRingBuffer};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

fn bench_mpmc(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpmc");

    const CAP: usize = 128;
    const ITEMS: usize = 1000;

    group.bench_function("std_mutex_vec_deque", |b| {
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(CAP)));
        b.iter(|| {
            let q1 = queue.clone();
            let q2 = queue.clone();

            thread::scope(|s| {
                s.spawn(move || {
                    for i in 0..ITEMS {
                        loop {
                            let mut g = q1.lock().unwrap();
                            if g.len() < CAP {
                                g.push_back(i);
                                break;
                            }
                            drop(g);
                            thread::yield_now();
                        }
                    }
                });

                s.spawn(move || {
                    let mut count = 0;
                    while count < ITEMS {
                         let mut g = q2.lock().unwrap();
                         if let Some(i) = g.pop_front() {
                             black_box(i);
                             count += 1;
                         }
                         drop(g);
                         if count < ITEMS {
                              // Don't spin too hard
                         }
                    }
                });
            });
        })
    });

    group.bench_function("ghost_ring_buffer", |b| {
         b.iter(|| {
             let buffer = GhostRingBuffer::new(CAP);
             let buffer = &buffer;

             thread::scope(|s| {
                 s.spawn(move || {
                     for i in 0..ITEMS {
                         while buffer.push(i).is_err() {
                             std::hint::spin_loop();
                         }
                     }
                 });

                 s.spawn(move || {
                     let mut count = 0;
                     while count < ITEMS {
                         if let Some(i) = buffer.pop() {
                             black_box(i);
                             count += 1;
                         }
                     }
                 });
             });
         })
    });

    group.finish();
}

criterion_group!(benches, bench_mpmc);
criterion_main!(benches);
