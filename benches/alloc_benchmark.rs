use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use halo::alloc::BrandedBumpAllocator;

fn bench_alloc_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("Single Allocation");

    group.bench_function("Box::new(u64)", |b| {
        b.iter(|| {
            black_box(Box::new(42u64));
        })
    });

    // For Bump, we can't easily reset per single alloc without cost.
    // We'll benchmark a batch of allocations.
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
                 // Setup: Create allocator inside token
                 // We can't return allocator out of token closure because of brand.
                 // This makes benchmarking tricky with Criterion's structure.
                 // We have to put the loop inside the token closure.
                 // But iter_batched expects setup -> input.

                 // Alternative: Just create allocator in the measurement loop?
                 // Allocator creation is cheap (vec new).
                 BrandedBumpAllocator::new()
            },
            |allocator| {
                // Measurement: Allocate 1000 items
                // We need to keep allocator alive.
                let alloc = &allocator;
                for i in 0..BATCH_SIZE {
                    black_box(alloc.alloc(i));
                }
                // Allocator drops here, freeing memory.
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
            || BrandedBumpAllocator::new(),
            |allocator| {
                for i in 0..BATCH_SIZE {
                    if i % 2 == 0 {
                        black_box(allocator.alloc(i as u64));
                    } else {
                        black_box(allocator.alloc_str("short string"));
                    }
                }
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_alloc_single, bench_alloc_batch, bench_alloc_mixed);
criterion_main!(benches);
