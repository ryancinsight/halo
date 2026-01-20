use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::{ActiveDisjointSet, BrandedDisjointSet, GhostToken};
use std::cell::RefCell;

// Standard library implementation using RefCell
struct StdDisjointSet {
    parent: Vec<RefCell<usize>>,
    rank: Vec<u8>,
}

impl StdDisjointSet {
    fn new() -> Self {
        Self {
            parent: Vec::new(),
            rank: Vec::new(),
        }
    }

    fn make_set(&mut self) -> usize {
        let id = self.parent.len();
        self.parent.push(RefCell::new(id));
        self.rank.push(0);
        id
    }

    fn find(&self, id: usize) -> usize {
        let mut root = id;
        loop {
            let parent = *self.parent[root].borrow();
            if parent == root {
                break;
            }
            root = parent;
        }

        let mut curr = id;
        while curr != root {
            let mut parent_ref = self.parent[curr].borrow_mut();
            let parent = *parent_ref;
            *parent_ref = root;
            curr = parent;
        }

        root
    }

    fn union(&mut self, id1: usize, id2: usize) -> bool {
        let root1 = self.find(id1);
        let root2 = self.find(id2);

        if root1 == root2 {
            return false;
        }

        if self.rank[root1] < self.rank[root2] {
            *self.parent[root1].borrow_mut() = root2;
        } else if self.rank[root1] > self.rank[root2] {
            *self.parent[root2].borrow_mut() = root1;
        } else {
            *self.parent[root2].borrow_mut() = root1;
            self.rank[root1] += 1;
        }

        true
    }
}

fn bench_disjoint_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("Disjoint Set");

    const N: usize = 10_000;
    const OPS: usize = 100_000;

    group.bench_function("BrandedDisjointSet", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut ds = BrandedDisjointSet::new();
                let mut active = ActiveDisjointSet::new(&mut ds, &mut token);

                for _ in 0..N {
                    active.make_set();
                }

                for i in 0..OPS {
                    let a = (i * 3) % N;
                    let b = (i * 7) % N;
                    black_box(active.union(a, b));
                    black_box(active.find(a));
                }
            })
        })
    });

    group.bench_function("Std RefCell", |b| {
        b.iter(|| {
            let mut ds = StdDisjointSet::new();

            for _ in 0..N {
                ds.make_set();
            }

            for i in 0..OPS {
                let a = (i * 3) % N;
                let b = (i * 7) % N;
                black_box(ds.union(a, b));
                black_box(ds.find(a));
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bench_disjoint_set);
criterion_main!(benches);
