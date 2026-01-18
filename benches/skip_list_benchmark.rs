use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{BrandedSkipList, BrandedBTreeMap};
use halo::GhostToken;
use std::collections::BTreeMap;

fn bench_skip_list(c: &mut Criterion) {
    let mut group = c.benchmark_group("skip_list_vs_btree");

    // Insert 1000 items
    group.bench_function("std_btreemap_insert_1000", |b| {
        b.iter(|| {
            let mut map = BTreeMap::new();
            for i in 0..1000 {
                map.insert(black_box(i), black_box(i));
            }
        });
    });

    group.bench_function("branded_btreemap_insert_1000", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut map = BrandedBTreeMap::new();
                for i in 0..1000 {
                    map.insert(black_box(i), black_box(i));
                }
                drop(token);
            });
        });
    });

    group.bench_function("branded_skip_list_insert_1000", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut list = BrandedSkipList::new();
                for i in 0..1000 {
                    list.insert(&mut token, black_box(i), black_box(i));
                }
                drop(token);
            });
        });
    });

    // Lookup
    group.bench_function("std_btreemap_lookup", |b| {
        let mut map = BTreeMap::new();
        for i in 0..1000 {
            map.insert(i, i);
        }
        b.iter(|| {
             for i in 0..1000 {
                 black_box(map.get(&i));
             }
        });
    });

    group.bench_function("branded_btreemap_lookup", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedBTreeMap::new();
            for i in 0..1000 {
                map.insert(i, i);
            }
            b.iter(|| {
                for i in 0..1000 {
                    black_box(map.get(&token, &i));
                }
            });
        });
    });

    group.bench_function("branded_skip_list_lookup", |b| {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            for i in 0..1000 {
                list.insert(&mut token, i, i);
            }
            let token = token; // drop mutability
            b.iter(|| {
                for i in 0..1000 {
                    black_box(list.get(&token, &i));
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_skip_list);
criterion_main!(benches);
