use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedIndexMap;
use halo::GhostToken;
use std::collections::HashMap;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    group.bench_function("std::HashMap", |b| {
        b.iter(|| {
            let mut map = HashMap::new();
            for i in 0..1000 {
                map.insert(i, i);
            }
            black_box(map)
        })
    });

    group.bench_function("BrandedIndexMap", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedIndexMap::new();
                for i in 0..1000 {
                    map.insert(i, i);
                }
                black_box(map)
            })
        })
    });

    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");
    let n = 10000;

    let mut std_map = HashMap::new();
    for i in 0..n {
        std_map.insert(i, i);
    }

    group.bench_function("std::HashMap", |b| {
        b.iter(|| {
            for i in 0..n {
                black_box(std_map.get(&i));
            }
        })
    });

    group.bench_function("BrandedIndexMap", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedIndexMap::new();
            for i in 0..n {
                map.insert(i, i);
            }

            b.iter(|| {
                for i in 0..n {
                    black_box(map.get(&token, &i));
                }
            })
        })
    });

    group.finish();
}

fn bench_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter");
    let n = 10000;

    let mut std_map = HashMap::new();
    for i in 0..n {
        std_map.insert(i, i);
    }

    group.bench_function("std::HashMap", |b| {
        b.iter(|| {
            for (k, v) in &std_map {
                black_box((k, v));
            }
        })
    });

    group.bench_function("BrandedIndexMap", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedIndexMap::new();
            for i in 0..n {
                map.insert(i, i);
            }

            b.iter(|| {
                for (k, v) in map.iter(&token) {
                    black_box((k, v));
                }
            })
        })
    });

    group.finish();
}

fn bench_get_index(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_index");
    let n = 10000;

    group.bench_function("BrandedIndexMap", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedIndexMap::new();
            for i in 0..n {
                map.insert(i, i);
            }

            b.iter(|| {
                for i in 0..n {
                    black_box(map.get_index(&token, i));
                }
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_insert, bench_lookup, bench_iter, bench_get_index);
criterion_main!(benches);
