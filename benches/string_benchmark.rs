use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use halo::{
    collections::{ActivateString, ActiveString, BrandedString},
    GhostToken,
};

fn bench_string_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("String Push (100x 'abc')");

    group.bench_function("std::String", |b| {
        b.iter(|| {
            let mut s = String::new();
            for _ in 0..100 {
                s.push_str("abc");
            }
            black_box(s);
        })
    });

    group.bench_function("BrandedString", |b| {
        b.iter(|| {
            let mut s = BrandedString::new();
            for _ in 0..100 {
                s.push_str("abc");
            }
            black_box(s);
        })
    });

    group.bench_function("ActiveString", |b| {
        b.iter_batched(
            || {},
            |()| {
                GhostToken::new(|mut token| {
                    let mut s = BrandedString::new();
                    // We include activation cost, which is just wrapping references
                    let mut active = s.activate(&mut token);
                    for _ in 0..100 {
                        active.push_str("abc");
                    }
                    black_box(s); // Use s to keep it alive (active borrows it)
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_string_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("String Read (len + chars count)");

    group.bench_function("std::String", |b| {
        b.iter_batched(
            || {
                let mut s = String::new();
                for _ in 0..100 {
                    s.push_str("abc");
                }
                s
            },
            |s| {
                black_box(s.as_str().len());
                black_box(s.chars().count());
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("BrandedString", |b| {
        b.iter_batched(
            || {},
            |()| {
                GhostToken::new(|token| {
                    let mut s = BrandedString::new();
                    for _ in 0..100 {
                        s.push_str("abc");
                    }
                    black_box(s.as_str(&token).len());
                    black_box(s.as_str(&token).chars().count());
                })
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("ActiveString", |b| {
        b.iter_batched(
            || {},
            |()| {
                GhostToken::new(|mut token| {
                    let mut s = BrandedString::new();
                    for _ in 0..100 {
                        s.push_str("abc");
                    }
                    let active = s.activate(&mut token);

                    black_box(active.as_str().len());
                    black_box(active.chars().count());
                })
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_string_push, bench_string_read);
criterion_main!(benches);
