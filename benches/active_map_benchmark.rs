use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::hash::ActivateHashMap;
use halo::{BrandedHashMap, GhostToken};
use std::collections::HashMap;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("Map Insert");
    let size = 1000;

    group.bench_function("std::HashMap", |b| {
        b.iter(|| {
            let mut map = HashMap::new();
            for i in 0..size {
                map.insert(black_box(i), black_box(i));
            }
        })
    });

    group.bench_function("BrandedHashMap", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                let mut map = BrandedHashMap::new();
                for i in 0..size {
                    map.insert(black_box(i), black_box(i));
                }
            })
        })
    });

    group.bench_function("ActiveHashMap", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedHashMap::new();
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

fn bench_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("Map Get");
    let size = 1000;

    let mut std_map = HashMap::new();
    for i in 0..size {
        std_map.insert(i, i);
    }

    group.bench_function("std::HashMap", |b| {
        b.iter(|| {
            for i in 0..size {
                black_box(std_map.get(&i));
            }
        })
    });

    GhostToken::new(|mut token| {
        let mut branded_map = BrandedHashMap::new();
        for i in 0..size {
            branded_map.insert(i, i);
        }

        group.bench_function("BrandedHashMap", |b| {
            b.iter(|| {
                for i in 0..size {
                    black_box(branded_map.get(&token, &i));
                }
            })
        });

        group.bench_function("ActiveHashMap", |b| {
            b.iter(|| {
                // Activate inside the loop to simulate usage scope
                let active = branded_map.activate(&mut token);
                for i in 0..size {
                    black_box(active.get(&i));
                }
            })
        });
    });

    group.finish();
}

fn bench_get_mut(c: &mut Criterion) {
    let mut group = c.benchmark_group("Map Get Mut");
    let size = 1000;

    let mut std_map = HashMap::new();
    for i in 0..size {
        std_map.insert(i, i);
    }

    group.bench_function("std::HashMap", |b| {
        b.iter(|| {
            for i in 0..size {
                if let Some(x) = std_map.get_mut(&i) {
                    *x += 1;
                }
            }
        })
    });

    GhostToken::new(|mut token| {
        let mut branded_map = BrandedHashMap::new();
        for i in 0..size {
            branded_map.insert(i, i);
        }

        group.bench_function("BrandedHashMap", |b| {
            b.iter(|| {
                for i in 0..size {
                    if let Some(x) = branded_map.get_mut(&mut token, &i) {
                        *x += 1;
                    }
                }
            })
        });

        group.bench_function("ActiveHashMap", |b| {
            b.iter(|| {
                let mut active = branded_map.activate(&mut token);
                for i in 0..size {
                    if let Some(x) = active.get_mut(&i) {
                        *x += 1;
                    }
                }
            })
        });
    });

    group.finish();
}

criterion_group!(benches, bench_insert, bench_get, bench_get_mut);
criterion_main!(benches);
