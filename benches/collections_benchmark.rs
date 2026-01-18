use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{
    BrandedArena, BrandedBTreeMap, BrandedChunkedVec, BrandedCowStrings, BrandedHashMap,
    BrandedHashSet, BrandedString, BrandedVec, BrandedVecDeque,
};
use halo::GhostToken;

fn bench_branded_chunked_vec_32(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_32", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 32> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_64(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_64", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 64> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_128(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_128", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 128> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_256(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_256", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 256> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_512(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_512", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 512> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_1024(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_1024", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 1024> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_std_vs_branded_vec(c: &mut Criterion) {
    let mut group = c.benchmark_group("std_vs_branded_vec");

    // Push/Pop operations - create fresh collections each time
    group.bench_function("std_vec_push_pop_1000", |b| {
        b.iter(|| {
            let mut vec = Vec::with_capacity(1000);
            for i in 0..1000 {
                vec.push(black_box(i));
            }
            for _ in 0..1000 {
                black_box(vec.pop());
            }
        });
    });

    group.bench_function("branded_vec_push_pop_1000", |b| {
        b.iter(|| {
            let mut vec = BrandedVec::with_capacity(1000);
            for i in 0..1000 {
                vec.push(black_box(i));
            }
            for _ in 0..1000 {
                black_box(vec.pop());
            }
        });
    });

    // Random access - pre-populate collections outside the benchmark loop
    let std_vec: Vec<i32> = (0..1000).collect();
    let branded_vec: BrandedVec<i32> = GhostToken::new(|_| {
        let mut vec = BrandedVec::with_capacity(1000);
        for i in 0..1000 {
            vec.push(i);
        }
        vec
    });

    group.bench_function("std_vec_random_access_1000", |b| {
        b.iter(|| {
            let mut sum = 0;
            for i in 0..1000 {
                sum += std_vec[i % 1000];
            }
            black_box(sum);
        });
    });

    group.bench_function("branded_vec_random_access_1000", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut sum = 0;
                for i in 0..1000 {
                    sum += *branded_vec.get(&token, i % 1000).unwrap();
                }
                black_box(sum);
            });
        });
    });

    // Bulk mutation - use fresh collections each time to avoid mutation accumulation
    group.bench_function("std_vec_bulk_mutation_1000", |b| {
        b.iter(|| {
            let mut vec = std_vec.clone();
            for x in &mut vec {
                *x = black_box(*x + 1);
            }
            black_box(vec);
        });
    });

    group.bench_function("branded_vec_bulk_mutation_1000", |b| {
        GhostToken::new(|mut token| {
            b.iter(|| {
                let mut sum = 0;
                for i in 0..branded_vec.len() {
                    if let Some(val) = branded_vec.get_mut(&mut token, i) {
                        *val = black_box(*val + 1);
                        sum += *val;
                    }
                }
                black_box(sum);
            });
        });
    });

    group.finish();
}

fn bench_comprehensive_stdlib_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("comprehensive_stdlib_comparison");

    // Memory overhead comparison
    group.bench_function("stdlib_vec_memory_per_element", |b| {
        b.iter(|| {
            let mut vec = Vec::with_capacity(1000);
            for i in 0..1000 {
                vec.push(i);
            }
            black_box(vec);
        });
    });

    group.bench_function("branded_vec_memory_per_element", |b| {
        b.iter(|| {
            let mut vec = BrandedVec::with_capacity(1000);
            for i in 0..1000 {
                vec.push(i);
            }
            black_box(vec);
        });
    });

    // Cache performance comparison (different access patterns)
    let std_vec: Vec<i32> = (0..10000).collect();
    let branded_vec: BrandedVec<i32> = GhostToken::new(|_| {
        let mut vec = BrandedVec::with_capacity(10000);
        for i in 0..10000 {
            vec.push(i);
        }
        vec
    });

    group.bench_function("stdlib_vec_sequential_access", |b| {
        b.iter(|| {
            let mut sum = 0;
            for &x in &std_vec {
                sum += x;
            }
            black_box(sum);
        });
    });

    group.bench_function("branded_vec_sequential_access", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut sum = 0;
                for i in 0..branded_vec.len() {
                    sum += *branded_vec.get(&token, i).unwrap();
                }
                black_box(sum);
            });
        });
    });

    group.bench_function("stdlib_vec_random_access", |b| {
        b.iter(|| {
            let mut sum = 0;
            for i in (0..10000).step_by(37) { // Pseudo-random access pattern
                sum += std_vec[i];
            }
            black_box(sum);
        });
    });

    group.bench_function("branded_vec_random_access", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut sum = 0;
                for i in (0..10000).step_by(37) {
                    sum += *branded_vec.get(&token, i).unwrap();
                }
                black_box(sum);
            });
        });
    });

    // Bulk operation comparison
    group.bench_function("stdlib_vec_bulk_transform", |b| {
        let mut vec = std_vec.clone();
        b.iter(|| {
            for x in &mut vec {
                *x = *x * 2 + 1;
            }
            black_box(&vec);
        });
    });

    group.bench_function("branded_vec_bulk_transform", |b| {
        GhostToken::new(|mut token| {
            b.iter(|| {
                for i in 0..branded_vec.len() {
                    if let Some(val) = branded_vec.get_mut(&mut token, i) {
                        *val = black_box(*val * 2 + 1);
                    }
                }
                black_box(&branded_vec);
            });
        });
    });

    group.finish();
}

fn bench_edge_case_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("edge_case_performance");

    // Empty collection operations
    group.bench_function("stdlib_vec_empty_ops", |b| {
        let vec = Vec::<i32>::new();
        b.iter(|| {
            black_box(vec.is_empty());
            black_box(vec.len());
            black_box(vec.get(0));
            black_box(vec.first());
            black_box(vec.last());
        });
    });

    group.bench_function("branded_vec_empty_ops", |b| {
        GhostToken::new(|token| {
            let vec = BrandedVec::<i32>::new();
            b.iter(|| {
                black_box(vec.is_empty());
                black_box(vec.len());
                black_box(vec.get(&token, 0));
            });
        });
    });

    // Large index operations
    let std_vec: Vec<i32> = (0..100000).collect();
    let branded_vec: BrandedVec<i32> = GhostToken::new(|_| {
        let mut vec = BrandedVec::with_capacity(100000);
        for i in 0..100000 {
            vec.push(i);
        }
        vec
    });

    group.bench_function("stdlib_vec_large_index_access", |b| {
        b.iter(|| {
            black_box(std_vec.get(99999));
            black_box(std_vec.get(50000));
            black_box(std_vec.get(0));
        });
    });

    group.bench_function("branded_vec_large_index_access", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                black_box(branded_vec.get(&token, 99999));
                black_box(branded_vec.get(&token, 50000));
                black_box(branded_vec.get(&token, 0));
            });
        });
    });

    group.finish();
}

fn bench_branded_vec_deque_operations(c: &mut Criterion) {
    c.bench_function("branded_vec_deque_push_pop", |b| {
        b.iter(|| {
            let mut deque = BrandedVecDeque::new();
            for i in 0..500 {
                deque.push_back(i);
                deque.push_front(i);
            }
            for _ in 0..500 {
                deque.pop_front();
                deque.pop_back();
            }
        });
    });

    c.bench_function("branded_vec_deque_access", |b| {
        GhostToken::new(|token| {
            let mut deque = BrandedVecDeque::new();
            for i in 0..1000 {
                deque.push_back(i);
            }
            b.iter(|| {
                for i in 0..1000 {
                    black_box(deque.get(&token, i));
                }
            });
        });
    });
}

fn bench_branded_hash_map_operations(c: &mut Criterion) {
    c.bench_function("branded_hash_map_insert_lookup", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut map = BrandedHashMap::new();
                for i in 0..500 {
                    map.insert(i, i * 2);
                }
                for i in 0..500 {
                    black_box(map.get(&token, &i));
                }
            });
        });
    });

    c.bench_function("branded_hash_map_mixed_operations", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedHashMap::new();
            b.iter(|| {
                // Insert phase
                for i in 0..200 {
                    map.insert(i, i);
                }
                // Lookup phase
                for i in 0..200 {
                    black_box(map.get(&token, &i));
                }
                // Remove phase
                for i in 100..200 {
                    map.remove(&i);
                }
            });
        });
    });
}

fn bench_branded_hash_set_operations(c: &mut Criterion) {
    c.bench_function("branded_hash_set_operations", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut set = BrandedHashSet::new();
                for i in 0..500 {
                    set.insert(i);
                }
                for i in 0..500 {
                    black_box(set.contains_gated(&token, &i));
                }
                for i in 250..500 {
                    set.remove(&i);
                }
            });
        });
    });
}

fn bench_branded_arena_operations(c: &mut Criterion) {
    c.bench_function("branded_arena_allocation", |b| {
        b.iter(|| {
            let mut arena: BrandedArena<'_, usize, 1024> = BrandedArena::new();
            let mut keys = Vec::new();
            for i in 0..1000 {
                keys.push(arena.alloc(i));
            }
            black_box(keys.len());
        });
    });

    c.bench_function("branded_arena_access", |b| {
        GhostToken::new(|mut token| {
            let mut arena: BrandedArena<'_, usize, 1024> = BrandedArena::new();
            let mut keys = Vec::new();
            for i in 0..1000 {
                keys.push(arena.alloc(i));
            }
            b.iter(|| {
                for &key in &keys {
                    black_box(arena.get_key(&token, key));
                    *arena.get_key_mut(&mut token, key) += 1;
                }
            });
        });
    });
}

fn bench_branded_btree_map_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("branded_btree_map_operations");

    group.bench_function("branded_btree_map_insert_1000", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut map = BrandedBTreeMap::new();
                for i in 0..1000 {
                    map.insert(i, i * 10);
                }
                black_box(&map);
            });
        });
    });

    group.bench_function("std_btree_map_insert_1000", |b| {
        b.iter(|| {
            let mut map = std::collections::BTreeMap::new();
            for i in 0..1000 {
                map.insert(i, i * 10);
            }
            black_box(&map);
        });
    });

    group.bench_function("branded_btree_map_get_1000", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedBTreeMap::new();
            for i in 0..1000 {
                map.insert(i, i * 10);
            }
            b.iter(|| {
                for i in 0..1000 {
                    black_box(map.get(&token, &i));
                }
            });
        });
    });

    group.bench_function("std_btree_map_get_1000", |b| {
        let mut map = std::collections::BTreeMap::new();
        for i in 0..1000 {
            map.insert(i, i * 10);
        }
        b.iter(|| {
            for i in 0..1000 {
                black_box(map.get(&i));
            }
        });
    });

    group.finish();
}

fn bench_branded_hash_map_vs_std(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_map_comparison");

    group.bench_function("branded_hash_map_insert_1000", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut map = BrandedHashMap::new();
                for i in 0..1000 {
                    map.insert(i, i);
                }
                black_box(&map);
            });
        });
    });

    group.bench_function("std_hash_map_insert_1000", |b| {
        b.iter(|| {
            let mut map = std::collections::HashMap::new();
            for i in 0..1000 {
                map.insert(i, i);
            }
            black_box(&map);
        });
    });

    group.finish();
}

fn bench_branded_cow_strings_operations(c: &mut Criterion) {
    c.bench_function("branded_cow_strings_insert_lookup", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut strings = BrandedCowStrings::new();
                for i in 0..500 {
                    let s = format!("string_{}", i);
                    strings.insert_owned(&token, s);
                }
                for i in 0..500 {
                    let s = format!("string_{}", i);
                    black_box(strings.get_by_value(&token, &s));
                }
            });
        });
    });
}

fn bench_branded_string_operations(c: &mut Criterion) {
    c.bench_function("branded_string_push", |b| {
        GhostToken::new(|mut token| {
            b.iter(|| {
                let s = BrandedString::new();
                for _ in 0..100 {
                    s.push_str(&mut token, "hello");
                }
                black_box(s.len(&token));
            });
        });
    });
}

criterion_group!(
    benches,
    bench_branded_chunked_vec_32,
    bench_branded_chunked_vec_64,
    bench_branded_chunked_vec_128,
    bench_branded_chunked_vec_256,
    bench_branded_chunked_vec_512,
    bench_branded_chunked_vec_1024,
    bench_std_vs_branded_vec,
    bench_branded_vec_deque_operations,
    bench_branded_hash_map_operations,
    bench_comprehensive_stdlib_comparison,
    bench_edge_case_performance,
    bench_branded_hash_set_operations,
    bench_branded_arena_operations,
    bench_branded_btree_map_operations,
    bench_branded_hash_map_vs_std,
    bench_branded_cow_strings_operations,
    bench_branded_string_operations
);
criterion_main!(benches);