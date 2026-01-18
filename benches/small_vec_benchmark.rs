use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{BrandedSmallVec, BrandedVec};
use halo::GhostToken;

fn bench_small_vec_inline(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_vec_inline");

    // Push/Pop within inline capacity (N=8)
    group.bench_function("branded_small_vec_push_pop_inline", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let mut vec: BrandedSmallVec<'_, i32, 8> = BrandedSmallVec::new();
                for i in 0..8 {
                    vec.push(black_box(i));
                }
                for _ in 0..8 {
                    black_box(vec.pop());
                }
            });
        });
    });

    // Comparison with BrandedVec (always heap)
    group.bench_function("branded_vec_push_pop_small", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let mut vec = BrandedVec::with_capacity(8);
                for i in 0..8 {
                    vec.push(black_box(i));
                }
                for _ in 0..8 {
                    black_box(vec.pop());
                }
            });
        });
    });

    // Comparison with std::Vec
    group.bench_function("std_vec_push_pop_small", |b| {
        b.iter(|| {
            let mut vec = Vec::with_capacity(8);
            for i in 0..8 {
                vec.push(black_box(i));
            }
            for _ in 0..8 {
                black_box(vec.pop());
            }
        });
    });

    group.finish();
}

fn bench_small_vec_spill(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_vec_spill");

    // Push exceeding inline capacity (N=8, push 16)
    group.bench_function("branded_small_vec_spill", |b| {
        b.iter(|| {
             GhostToken::new(|_token| {
                let mut vec: BrandedSmallVec<'_, i32, 8> = BrandedSmallVec::new();
                for i in 0..16 {
                    vec.push(black_box(i));
                }
                black_box(vec);
             });
        });
    });

    group.bench_function("branded_vec_resize", |b| {
        b.iter(|| {
             GhostToken::new(|_token| {
                // Start with small capacity and grow
                let mut vec = BrandedVec::with_capacity(8);
                for i in 0..16 {
                    vec.push(black_box(i));
                }
                black_box(vec);
             });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_small_vec_inline, bench_small_vec_spill);
criterion_main!(benches);
