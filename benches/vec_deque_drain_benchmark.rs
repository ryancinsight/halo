use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use halo::{BrandedVecDeque, GhostToken};
use std::collections::VecDeque;

fn bench_drain_middle(c: &mut Criterion) {
    let mut group = c.benchmark_group("VecDeque Drain Middle");
    let size = 10000;
    let drain_start = 4000;
    let drain_end = 6000; // Drain 2000 elements from middle

    group.bench_function("std::VecDeque", |b| {
        b.iter_batched(
            || (0..size).collect::<VecDeque<i32>>(),
            |mut deque| {
                let drained: Vec<_> = deque.drain(drain_start..drain_end).collect();
                black_box(drained);
                // deque is dropped here
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("BrandedVecDeque", |b| {
        b.iter_batched(
            || {
                GhostToken::new(|_token| {
                    let deque: BrandedVecDeque<'_, i32> = (0..size).collect();
                    unsafe {
                        std::mem::transmute::<BrandedVecDeque<'_, i32>, BrandedVecDeque<'static, i32>>(
                            deque,
                        )
                    }
                })
            },
            |mut deque| {
                let drained: Vec<_> = deque.drain(drain_start..drain_end).collect();
                black_box(drained);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_rotate_intensive(c: &mut Criterion) {
    let mut group = c.benchmark_group("VecDeque Rotate Intensive");
    let size = 10000;
    let rotate_amount = 2000;
    let iterations = 1000;

    group.bench_function("std::VecDeque", |b| {
        b.iter(|| {
            let mut deque: VecDeque<i32> = (0..size).collect();
            for _ in 0..iterations {
                deque.rotate_left(rotate_amount);
            }
            black_box(deque);
        })
    });

    group.bench_function("BrandedVecDeque", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let mut deque: BrandedVecDeque<i32> = BrandedVecDeque::from_iter(0..size);
                for _ in 0..iterations {
                    deque.rotate_left(rotate_amount);
                }
                black_box(deque);
            })
        })
    });

    group.finish();
}

fn bench_rotate_partial(c: &mut Criterion) {
    let mut group = c.benchmark_group("VecDeque Rotate Partial (len < cap)");
    let size = 5000;
    let capacity = 10000;
    let rotate_amount = 1000;
    let iterations = 200;

    group.bench_function("std::VecDeque", |b| {
        b.iter(|| {
             // Create with capacity
            let mut deque = VecDeque::with_capacity(capacity);
            deque.extend(0..size);

            for _ in 0..iterations {
                deque.rotate_left(rotate_amount);
            }
            black_box(deque);
        })
    });

    group.bench_function("BrandedVecDeque", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let mut deque = BrandedVecDeque::with_capacity(capacity);
                deque.extend(0..size);

                for _ in 0..iterations {
                    deque.rotate_left(rotate_amount);
                }
                black_box(deque);
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_drain_middle, bench_rotate_intensive, bench_rotate_partial);
criterion_main!(benches);
