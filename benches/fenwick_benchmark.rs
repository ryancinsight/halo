use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::other::active::ActiveFenwickTree;
use halo::collections::other::active::ActivateFenwickTree;
use halo::collections::other::BrandedFenwickTree;
use halo::collections::trie::active::ActivateRadixTrieMap;
use halo::collections::trie::ActiveRadixTrieMap;
use halo::collections::trie::BrandedRadixTrieMap;
use halo::GhostToken;
use std::collections::BTreeMap;

// Naive implementation for comparison
struct NaiveFenwickTree {
    tree: Vec<i64>,
}

impl NaiveFenwickTree {
    fn new(size: usize) -> Self {
        Self {
            tree: vec![0; size + 1],
        }
    }

    fn add(&mut self, index: usize, delta: i64) {
        let n = self.tree.len() - 1;
        let mut idx = index + 1;
        while idx <= n {
            self.tree[idx] += delta;
            idx += idx & (!idx + 1); // equivalent to idx & -idx for unsigned two's complement
        }
    }

    fn prefix_sum(&self, index: usize) -> i64 {
        let mut sum = 0;
        let mut idx = index + 1;
        while idx > 0 {
            sum += self.tree[idx];
            idx -= idx & (!idx + 1);
        }
        sum
    }
}

fn bench_fenwick_tree(c: &mut Criterion) {
    let size = 100_000;

    let mut group = c.benchmark_group("FenwickTree");

    group.bench_function("Naive Add", |b| {
        b.iter_batched(
            || NaiveFenwickTree::new(size),
            |mut ft| {
                for i in 0..1000 {
                    ft.add(black_box(i * 7 % size), 1);
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("Branded Add (Active)", |b| {
        b.iter_batched(
            || {
                std::iter::repeat(0i64).take(size).collect::<BrandedFenwickTree<_>>()
            },
            |mut ft| {
                GhostToken::new(|mut token| {
                    // Safety: We transmute the brand of the tree to match the token.
                    // This is safe in this context because we have exclusive access to the tree
                    // and we are simply allowing the token to govern it for this scope.
                    let ft_ptr = &mut ft as *mut BrandedFenwickTree<'_, i64>;
                    let ft_casted: &mut BrandedFenwickTree<'_, i64> = unsafe { std::mem::transmute(ft_ptr) };

                    let mut active = ft_casted.activate(&mut token);
                    for i in 0..1000 {
                        active.add(black_box(i * 7 % size), 1);
                    }
                });
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("Naive Query", |b| {
        let mut ft = NaiveFenwickTree::new(size);
        for i in 0..size {
            ft.add(i, 1);
        }
        b.iter(|| {
            black_box(ft.prefix_sum(black_box(size / 2)));
        })
    });

    group.bench_function("Branded Query (Active)", |b| {
        GhostToken::new(|mut token| {
            let mut ft: BrandedFenwickTree<i64> = std::iter::repeat(1i64).take(size).collect();
            let active = ft.activate(&mut token);
            b.iter(|| {
                black_box(active.prefix_sum(black_box(size / 2)));
            })
        });
    });

    group.finish();
}

fn bench_radix_trie(c: &mut Criterion) {
    let mut group = c.benchmark_group("Map Insert String");

    let keys: Vec<String> = (0..1000).map(|i| format!("key_{}", i)).collect();

    group.bench_function("Std BTreeMap Insert", |b| {
        b.iter_batched(
            || BTreeMap::new(),
            |mut map| {
                for key in &keys {
                    map.insert(key.clone(), 0);
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("Branded RadixTrie Insert (Active)", |b| {
        b.iter_batched(
            || BrandedRadixTrieMap::new(),
            |mut map| {
                GhostToken::new(|mut token| {
                    // Safety: Transmute brand to match token
                    let map_ptr = &mut map as *mut BrandedRadixTrieMap<'_, &[u8], i32>;
                    let map_casted: &mut BrandedRadixTrieMap<'_, &[u8], i32> = unsafe { std::mem::transmute(map_ptr) };

                    let mut active = map_casted.activate(&mut token);
                    for key in &keys {
                        active.insert(key.as_bytes(), 0);
                    }
                });
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_fenwick_tree, bench_radix_trie);
criterion_main!(benches);
