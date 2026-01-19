use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{ActivateSkipList, BrandedBTreeMap, BrandedSkipList};
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
            GhostToken::new(|_token| {
                let mut map = BrandedBTreeMap::new();
                for i in 0..1000 {
                    map.insert(black_box(i), black_box(i));
                }
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
            });
        });
    });

    group.bench_function("active_skip_list_insert_1000", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut list = BrandedSkipList::new();
                let mut active = list.activate(&mut token);
                for i in 0..1000 {
                    active.insert(black_box(i), black_box(i));
                }
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
        GhostToken::new(|token| {
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

    group.bench_function("active_skip_list_lookup", |b| {
        GhostToken::new(|mut token| {
            let mut list = BrandedSkipList::new();
            {
                let mut active = list.activate(&mut token);
                for i in 0..1000 {
                    active.insert(i, i);
                }
            }
            let active = list.activate(&mut token);
            b.iter(|| {
                for i in 0..1000 {
                    black_box(active.get(&i));
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_skip_list);
criterion_main!(benches);
