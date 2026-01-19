use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{BrandedHashMap, BrandedSlotMap};
use halo::GhostToken;

fn bench_slot_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("slot_map_vs_hash_map");

    // Insert 1000 items
    group.bench_function("branded_slot_map_insert_1000", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedSlotMap::new();
                for i in 0..1000 {
                    map.insert(&mut token, black_box(i));
                }
            });
        });
    });

    group.bench_function("branded_hash_map_insert_1000", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedHashMap::new();
                for i in 0..1000 {
                    map.insert(black_box(i), black_box(i));
                }
            });
        });
    });

    // Lookup 1000 items (sequential)
    group.bench_function("branded_slot_map_lookup", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedSlotMap::new();
            let mut keys = Vec::with_capacity(1000);
            for i in 0..1000 {
                keys.push(map.insert(&mut token, i));
            }

            b.iter(|| {
                for key in &keys {
                    black_box(map.get(&token, *key));
                }
            });
        });
    });

    group.bench_function("branded_hash_map_lookup", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedHashMap::new();
            let mut keys = Vec::with_capacity(1000);
            for i in 0..1000 {
                map.insert(i, i);
                keys.push(i);
            }

            b.iter(|| {
                for key in &keys {
                    black_box(map.get(&token, key));
                }
            });
        });
    });

    // Removal
    group.bench_function("branded_slot_map_remove", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedSlotMap::new();
                let mut keys = Vec::with_capacity(1000);
                for i in 0..1000 {
                    keys.push(map.insert(&mut token, i));
                }

                for key in keys {
                    map.remove(&mut token, key);
                }
            });
        });
    });

    group.bench_function("branded_hash_map_remove", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedHashMap::new();
                let mut keys = Vec::with_capacity(1000);
                for i in 0..1000 {
                    map.insert(i, i);
                    keys.push(i);
                }

                for key in keys {
                    map.remove(&key);
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_slot_map);
criterion_main!(benches);
