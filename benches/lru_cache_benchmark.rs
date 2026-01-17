use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedLruCache;
use halo::GhostToken;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;

// Minimal LRU implementation using RefCell + HashMap + Vec (Arena)
// This simulates the "standard" interior mutability approach.
struct StdLruCache<K, V> {
    map: HashMap<K, usize>,
    nodes: Vec<Node<K, V>>,
    head: Option<usize>,
    tail: Option<usize>,
    free_head: Option<usize>,
    capacity: usize,
}

struct Node<K, V> {
    key: Option<K>,
    val: Option<V>,
    prev: Option<usize>,
    next: Option<usize>,
}

impl<K: Clone + Hash + Eq, V: Clone> StdLruCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            nodes: Vec::with_capacity(capacity),
            head: None,
            tail: None,
            free_head: None,
            capacity,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        if let Some(&idx) = self.map.get(key) {
            self.move_to_front(idx);
            self.nodes[idx].val.clone()
        } else {
            None
        }
    }

    fn put(&mut self, key: K, val: V) {
        if let Some(&idx) = self.map.get(&key) {
            self.move_to_front(idx);
            self.nodes[idx].val = Some(val);
        } else {
            if self.map.len() >= self.capacity {
                self.pop_back();
            }
            let idx = self.alloc(key.clone(), val);
            self.map.insert(key, idx);
            self.attach_front(idx);
        }
    }

    fn alloc(&mut self, key: K, val: V) -> usize {
         if let Some(idx) = self.free_head {
             let node = &mut self.nodes[idx];
             let next_free = node.next;
             node.key = Some(key);
             node.val = Some(val);
             node.prev = None;
             node.next = None;
             self.free_head = next_free;
             idx
         } else {
             let idx = self.nodes.len();
             self.nodes.push(Node { key: Some(key), val: Some(val), prev: None, next: None });
             idx
         }
    }

    fn pop_back(&mut self) {
        if let Some(idx) = self.tail {
             let key = self.nodes[idx].key.take().unwrap();
             self.map.remove(&key);
             self.detach(idx);
             // free idx
             self.nodes[idx].next = self.free_head;
             self.free_head = Some(idx);
        }
    }

    fn move_to_front(&mut self, idx: usize) {
        if self.head == Some(idx) { return; }
        self.detach(idx);
        self.attach_front(idx);
    }

    fn detach(&mut self, idx: usize) {
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;

        if let Some(p) = prev { self.nodes[p].next = next; } else { self.head = next; }
        if let Some(n) = next { self.nodes[n].prev = prev; } else { self.tail = prev; }

        self.nodes[idx].prev = None;
        self.nodes[idx].next = None;
    }

    fn attach_front(&mut self, idx: usize) {
        if let Some(h) = self.head {
            self.nodes[h].prev = Some(idx);
            self.nodes[idx].next = Some(h);
            self.head = Some(idx);
        } else {
            self.head = Some(idx);
            self.tail = Some(idx);
        }
    }
}

fn bench_lru_cache_put_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("lru_cache_put_get");
    let size = 1000;

    group.bench_function("refcell_std_lru", |b| {
        b.iter(|| {
             let cache = RefCell::new(StdLruCache::new(size));
             for i in 0..size {
                 cache.borrow_mut().put(i, i);
             }
             for i in 0..size {
                 black_box(cache.borrow_mut().get(&i));
             }
        });
    });

    group.bench_function("branded_lru", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut cache = BrandedLruCache::new(size);
                for i in 0..size {
                    cache.put(&mut token, i, i);
                }
                for i in 0..size {
                    black_box(cache.get(&mut token, &i));
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_lru_cache_put_get);
criterion_main!(benches);
