use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::btree::BrandedBTreeMap;
use halo::GhostToken;
use std::collections::BTreeMap;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("BTree Insert");

    group.bench_function("std_btree_map_insert", |b| {
        b.iter(|| {
            let mut map = BTreeMap::new();
            for i in 0..1000 {
                map.insert(black_box(i), black_box(i));
            }
            map
        });
    });

    group.bench_function("branded_btree_map_insert", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let mut map = BrandedBTreeMap::new();
                for i in 0..1000 {
                    map.insert(black_box(i), black_box(i));
                }
                map
            });
        });
    });

    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("BTree Lookup");

    let size = 1000;

    group.bench_function("std_btree_map_lookup", |b| {
        let mut map = BTreeMap::new();
        for i in 0..size {
            map.insert(i, i);
        }

        b.iter(|| {
            for i in 0..size {
                black_box(map.get(&i));
            }
        });
    });

    group.bench_function("branded_btree_map_lookup", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            for i in 0..size {
                map.insert(i, i);
            }

            b.iter(|| {
                for i in 0..size {
                    black_box(map.get(&token, &i));
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_insert, bench_lookup);
criterion_main!(benches);
