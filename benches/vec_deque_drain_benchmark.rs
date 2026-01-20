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
                // We need to construct the deque inside a token scope or pass it out?
                // BrandedVecDeque has a lifetime 'brand.
                // We can't return it easily from setup unless we use a static-like token or unsafe hack for bench.
                // Or we accept that construction is part of it but try to minimize it?
                // No, we can't easily use iter_batched with BrandedVecDeque because of the lifetime binding to the closure.
                // The setup closure returns a value, but the token is local to the setup?
                // If we create token in setup, we can't return the deque because it borrows the token.

                // Workaround: We benchmark the whole block (Token + Construction + Op + Drop)
                // VS std (Construction + Op + Drop).
                // The difference in Op should still be visible if we subtract the baseline?

                // Let's rely on the previous run's data but interpret it carefully.
                // If Drop is O(N), it dominates O(1) rotation.
                // But Drain is O(N) anyway.

                // Let's try to make BrandedVecDeque faster at Drop for Copy types?
                // That requires specialization or specific optimization.

                // For now, let's just create the setup inside the measurement but keep it fair.
                // Since I can't use iter_batched for Branded types easily, I'll stick to `iter`.
                // But I should verify the Drop cost.

                // Wait, I can verify the Op cost by doing it many times?
                // Rotate can be done many times.

                // For Drain, I can't.

                // Let's reconsider the benchmark.
                // If I can't isolate the op, I can't prove O(1) vs O(N) easily if O(N) setup/teardown dominates.

                // Maybe I can make the setup faster?
                // No.

                // What if I increase the operation cost relative to setup?
                // e.g. Drain larger amount?

                // For Rotate, I can rotate many times in one iteration.
                // 1000 rotations.
                ()
            },
            |_| (),
            BatchSize::SmallInput
        )
    });

    // Updated benchmarks with repeated operations for Rotate
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
            GhostToken::new(|mut token| {
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

criterion_group!(benches, bench_rotate_intensive);
criterion_main!(benches);
