use criterion::{black_box, criterion_group, criterion_main, Criterion};

// Default system allocator

fn bench_alloc_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("Batch Allocation 1000");
    const BATCH_SIZE: usize = 1000;

    group.bench_function("Box::new (System)", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(BATCH_SIZE);
            for i in 0..BATCH_SIZE {
                v.push(Box::new(i));
            }
            black_box(v);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_alloc_batch);
criterion_main!(benches);
