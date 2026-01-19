use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use halo::alloc::BrandedBumpAllocator;
use halo::GhostToken;

fn bench_alloc_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("Single Allocation");

    group.bench_function("Box::new(u64)", |b| {
        b.iter(|| {
            black_box(Box::new(42u64));
        })
    });
}

fn bench_alloc_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("Batch Allocation 1000");
    const BATCH_SIZE: usize = 1000;

    group.bench_function("Box::new", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(BATCH_SIZE);
            for i in 0..BATCH_SIZE {
                v.push(Box::new(i));
            }
            black_box(v);
        })
    });

    group.bench_function("BrandedBumpAllocator", |b| {
        b.iter_batched(
            || {
                // We need to return the allocator setup, but we can't easily pass the token.
                // So we will create the token inside the measurement.
                // This adds token creation overhead to the benchmark, but it should be minimal (ZST).
            },
            |()| {
                GhostToken::new(|mut token| {
                    let allocator = BrandedBumpAllocator::new();
                    for i in 0..BATCH_SIZE {
                        black_box(allocator.alloc(i, &mut token));
                    }
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_alloc_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("Mixed Allocation 1000");
    const BATCH_SIZE: usize = 1000;

    group.bench_function("BrandedBumpAllocator Mixed", |b| {
        b.iter_batched(
            || {},
            |()| {
                GhostToken::new(|mut token| {
                    let allocator = BrandedBumpAllocator::new();
                    for i in 0..BATCH_SIZE {
                        if i % 2 == 0 {
                            black_box(allocator.alloc(i as u64, &mut token));
                        } else {
                            black_box(allocator.alloc_str("short string", &mut token));
                        }
                    }
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_alloc_single,
    bench_alloc_batch,
    bench_alloc_mixed
);
criterion_main!(benches);
