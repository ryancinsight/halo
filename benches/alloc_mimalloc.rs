use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn bench_alloc_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("Batch Allocation 1000");
    const BATCH_SIZE: usize = 1000;

    group.bench_function("Box::new (Mimalloc)", |b| {
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
