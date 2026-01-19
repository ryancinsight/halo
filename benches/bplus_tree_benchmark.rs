use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::btree::active::ActivateBTreeMap;
use halo::collections::btree::{BrandedBPlusTree, BrandedBTreeMap};
use halo::GhostToken;
use std::collections::BTreeMap;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("BTree Insert");
    let size = 10000;

    group.bench_function("std::BTreeMap", |b| {
        b.iter(|| {
            let mut map = BTreeMap::new();
            for i in 0..size {
                map.insert(black_box(i), black_box(i));
            }
        })
    });

    group.bench_function("BrandedBTreeMap (Box)", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedBTreeMap::new();
                {
                    let mut active = map.activate(&mut token);
                    for i in 0..size {
                        active.insert(black_box(i), black_box(i));
                    }
                }
            })
        })
    });

    group.bench_function("BrandedBPlusTree (Pool)", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedBPlusTree::new();
                {
                    let mut active = map.activate(&mut token);
                    for i in 0..size {
                        active.insert(black_box(i), black_box(i));
                    }
                }
            })
        })
    });

    group.finish();
}

fn bench_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("BTree Iter");
    let size = 10000;

    let mut std_map = BTreeMap::new();
    for i in 0..size {
        std_map.insert(i, i);
    }

    group.bench_function("std::BTreeMap", |b| {
        b.iter(|| {
            let mut sum = 0;
            for (k, v) in std_map.iter() {
                sum += k + v;
            }
            black_box(sum);
        })
    });

    GhostToken::new(|mut token| {
        let mut map_box = BrandedBTreeMap::new();
        {
            let mut active = map_box.activate(&mut token);
            for i in 0..size {
                active.insert(i, i);
            }
        }

        let mut map_pool = BrandedBPlusTree::new();
        {
            let mut active = map_pool.activate(&mut token);
            for i in 0..size {
                active.insert(i, i);
            }
        }

        group.bench_function("BrandedBTreeMap (Box)", |b| {
            b.iter(|| {
                let mut sum = 0;
                for (k, v) in map_box.iter(&token) {
                    sum += k + v;
                }
                black_box(sum);
            })
        });

        group.bench_function("BrandedBPlusTree (Pool)", |b| {
            b.iter(|| {
                let mut sum = 0;
                for (k, v) in map_pool.iter(&token) {
                    sum += k + v;
                }
                black_box(sum);
            })
        });
    });

    group.finish();
}

criterion_group!(benches, bench_insert, bench_iter);
criterion_main!(benches);
