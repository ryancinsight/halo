use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use halo::{GhostToken, collections::{BrandedHashSet, ActiveHashSet, ActivateHashSet}};
use std::collections::HashSet;

fn bench_set_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("HashSet Insert (100 items)");

    group.bench_function("std::HashSet", |b| {
        b.iter(|| {
            let mut s = HashSet::new();
            for i in 0..100 {
                s.insert(i);
            }
            black_box(s);
        })
    });

    group.bench_function("BrandedHashSet", |b| {
         b.iter_batched(
            || {},
            |()| {
                // BrandedHashSet insertion is structural and does not require a token
                // because it operates on the internal structure via &mut self.
                let mut s = BrandedHashSet::new();
                for i in 0..100 {
                    s.insert(i);
                }
                black_box(s);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("ActiveHashSet", |b| {
        b.iter_batched(
            || {},
            |()| {
                GhostToken::new(|mut token| {
                    let mut s = BrandedHashSet::new();
                    let mut active = s.activate(&mut token);
                    for i in 0..100 {
                        active.insert(i);
                    }
                    black_box(s);
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_set_contains(c: &mut Criterion) {
    let mut group = c.benchmark_group("HashSet Contains (lookup)");

    group.bench_function("std::HashSet", |b| {
        b.iter_batched(
            || {
                let mut s = HashSet::new();
                for i in 0..100 { s.insert(i); }
                s
            },
            |s| {
                for i in 0..100 {
                    black_box(s.contains(&i));
                }
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("BrandedHashSet", |b| {
         b.iter_batched(
            || {
                let mut s = BrandedHashSet::new();
                for i in 0..100 { s.insert(i); }
                s
            },
            |s| {
                // BrandedHashSet contains doesn't need token
                for i in 0..100 {
                    black_box(s.contains(&i));
                }
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("ActiveHashSet", |b| {
        b.iter_batched(
            || {},
            |()| {
                GhostToken::new(|mut token| {
                    let mut s = BrandedHashSet::new();
                    for i in 0..100 { s.insert(i); }
                    let active = s.activate(&mut token);

                    for i in 0..100 {
                         black_box(active.contains(&i));
                    }
                });
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_set_insert, bench_set_contains);
criterion_main!(benches);
