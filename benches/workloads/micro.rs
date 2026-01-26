use criterion::{black_box, Criterion};

pub fn run(c: &mut Criterion) {
    bench_alloc_small(c);
    bench_alloc_medium(c);
    bench_alloc_large(c);
    bench_vec_push(c);
}

fn bench_alloc_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("micro_small");
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(1));
    group.sample_size(10);

    group.bench_function("alloc_free_16b", |b| {
        b.iter(|| {
            black_box(Box::new(black_box(10u128)));
        })
    });

    group.finish();
}

fn bench_alloc_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("micro_medium");

    group.bench_function("alloc_free_1kb", |b| {
        b.iter(|| {
            // Use Vec to avoid stack overflow, then convert to box to ensure heap alloc
            let v = Vec::<u8>::with_capacity(1024);
            black_box(v.into_boxed_slice());
        })
    });

    group.finish();
}

fn bench_alloc_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("micro_large");

    group.bench_function("alloc_free_1mb", |b| {
        b.iter(|| {
            let v = Vec::<u8>::with_capacity(1024 * 1024);
            black_box(v.into_boxed_slice());
        })
    });

    group.finish();
}

fn bench_vec_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("micro_vec");

    group.bench_function("vec_push_1000", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(1000);
            for i in 0..1000 {
                v.push(black_box(i));
            }
            black_box(v);
        })
    });

    group.finish();
}
