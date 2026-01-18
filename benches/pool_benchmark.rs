use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::alloc::BrandedPool;
use halo::GhostToken;

fn bench_pool_alloc_free(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool_alloc_free");

    group.bench_function("branded_pool_alloc_free", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let pool: BrandedPool<'_, i32> = BrandedPool::new();
                let mut indices = Vec::with_capacity(1000);
                for i in 0..1000 {
                    indices.push(pool.alloc(&mut token, i));
                }
                for idx in indices {
                    unsafe { pool.free(&mut token, idx) };
                }
            });
        });
    });

    group.bench_function("std_box_alloc_free", |b| {
        b.iter(|| {
            let mut boxes = Vec::with_capacity(1000);
            for i in 0..1000 {
                boxes.push(Box::new(i));
            }
            black_box(boxes);
        });
    });

    group.finish();
}

fn bench_pool_reuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool_reuse");

    group.bench_function("branded_pool_reuse", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let pool: BrandedPool<'_, i32> = BrandedPool::new();
                // Alloc one
                let idx = pool.alloc(&mut token, 0);
                unsafe { pool.free(&mut token, idx) };

                // Repeated alloc/free should reuse slot
                for i in 0..1000 {
                    let idx = pool.alloc(&mut token, i);
                    unsafe { pool.free(&mut token, idx) };
                }
            });
        });
    });

    group.bench_function("std_box_reuse", |b| {
        b.iter(|| {
            // Box doesn't reuse, it allocs/frees from heap allocator
            for i in 0..1000 {
                let b = Box::new(i);
                black_box(b); // Drop
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_pool_alloc_free, bench_pool_reuse);
criterion_main!(benches);
