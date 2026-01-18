use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{BrandedRadixTrieMap};
use halo::GhostToken;
use std::collections::{BTreeMap, HashMap};

fn bench_trie_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_insert");

    // Generate some keys
    let keys: Vec<String> = (0..1000).map(|i| format!("key_{:04}", i)).collect();

    group.bench_function("branded_trie_insert", |b| {
        GhostToken::new(|mut token| {
            b.iter(|| {
                let mut map = BrandedRadixTrieMap::new();
                for (i, key) in keys.iter().enumerate() {
                    map.insert(&mut token, key.as_bytes(), i);
                }
                black_box(map);
            });
        });
    });

    group.bench_function("std_btreemap_insert", |b| {
        b.iter(|| {
            let mut map = BTreeMap::new();
            for (i, key) in keys.iter().enumerate() {
                map.insert(key.as_bytes(), i);
            }
            black_box(map);
        });
    });

    group.bench_function("std_hashmap_insert", |b| {
        b.iter(|| {
            let mut map = HashMap::new();
            for (i, key) in keys.iter().enumerate() {
                map.insert(key.as_bytes(), i);
            }
            black_box(map);
        });
    });

    group.finish();
}

fn bench_trie_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("trie_get");

    let keys: Vec<String> = (0..1000).map(|i| format!("key_{:04}", i)).collect();

    group.bench_function("branded_trie_get", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedRadixTrieMap::new();
            for (i, key) in keys.iter().enumerate() {
                map.insert(&mut token, key.as_bytes(), i);
            }

            b.iter(|| {
                for key in &keys {
                    black_box(map.get(&token, key.as_bytes()));
                }
            });
        });
    });

    group.bench_function("std_btreemap_get", |b| {
        let mut map = BTreeMap::new();
        for (i, key) in keys.iter().enumerate() {
            map.insert(key.as_bytes(), i);
        }

        b.iter(|| {
            for key in &keys {
                black_box(map.get(key.as_bytes()));
            }
        });
    });

    group.bench_function("std_hashmap_get", |b| {
        let mut map = HashMap::new();
        for (i, key) in keys.iter().enumerate() {
            map.insert(key.as_bytes(), i);
        }

        b.iter(|| {
            for key in &keys {
                black_box(map.get(key.as_bytes()));
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_trie_insert, bench_trie_get);
criterion_main!(benches);
