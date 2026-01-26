use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::allocator::bootstrap::arena::BootstrapArena;

fn bootstrap_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("bootstrap");

    for size_mb in [64, 128, 256, 1024].iter() {
        let size = size_mb * 1024 * 1024;
        group.bench_function(format!("reserve_{}MB", size_mb), |b| {
            b.iter(|| {
                let _arena = BootstrapArena::new(black_box(size));
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bootstrap_benchmark);
criterion_main!(benches);
