use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::graph::GhostDag;
use halo::GhostToken;

// Simple stdlib-based DAG implementation for comparison
struct StdDag {
    adj: Vec<Vec<usize>>,
    topo: Option<Vec<usize>>,
}

impl StdDag {
    fn from_adjacency(adj: &[Vec<usize>]) -> Self {
        Self {
            adj: adj.to_vec(),
            topo: None,
        }
    }

    fn topological_sort(&mut self) -> Option<&[usize]> {
        if self.topo.is_some() {
            return self.topo.as_deref();
        }

        let n = self.adj.len();
        let mut indeg = vec![0usize; n];
        for u in 0..n {
            for &v in &self.adj[u] {
                indeg[v] += 1;
            }
        }

        let mut q = std::collections::VecDeque::new();
        for u in 0..n {
            if indeg[u] == 0 {
                q.push_back(u);
            }
        }

        let mut topo_order = Vec::with_capacity(n);
        while let Some(u) = q.pop_front() {
            topo_order.push(u);
            for &v in &self.adj[u] {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    q.push_back(v);
                }
            }
        }

        if topo_order.len() == n {
            self.topo = Some(topo_order);
            self.topo.as_deref()
        } else {
            None
        }
    }

    fn longest_path_lengths(&mut self) -> Option<Vec<usize>> {
        let topo = self.topological_sort()?.to_vec();
        let n = self.adj.len();
        let mut dist = vec![0usize; n];

        for &u in &topo {
            for &v in &self.adj[u] {
                dist[v] = dist[v].max(dist[u] + 1);
            }
        }

        Some(dist)
    }
}

fn create_test_graphs() -> Vec<Vec<Vec<usize>>> {
    vec![
        // Small chain: 0 -> 1 -> 2 -> 3
        vec![vec![1], vec![2], vec![3], vec![]],
        // Diamond: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
        vec![vec![1, 2], vec![3], vec![3], vec![]],
        // Tree: 0 -> 1,2,3; 1 -> 4,5; 2 -> 6
        vec![
            vec![1, 2, 3],
            vec![4, 5],
            vec![6],
            vec![],
            vec![],
            vec![],
            vec![],
        ],
        // Larger graph (10 nodes)
        vec![
            vec![1, 2, 3],
            vec![4, 5],
            vec![5, 6],
            vec![6, 7],
            vec![8],
            vec![8],
            vec![9],
            vec![9],
            vec![],
            vec![],
        ],
    ]
}

fn bench_ghost_dag_topological_sort(c: &mut Criterion) {
    let graphs = create_test_graphs();

    c.bench_function("ghost_dag_topological_sort", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                for adj in &graphs {
                    let mut dag = GhostDag::<1024>::from_adjacency(adj);
                    black_box(dag.topological_sort());
                }
                // Consume token to avoid unused variable warning
                let _ = token;
            });
        });
    });
}

fn bench_std_dag_topological_sort(c: &mut Criterion) {
    let graphs = create_test_graphs();

    c.bench_function("std_dag_topological_sort", |b| {
        b.iter(|| {
            for adj in &graphs {
                let mut dag = StdDag::from_adjacency(adj);
                black_box(dag.topological_sort());
            }
            black_box(&graphs); // Ensure graphs is not optimized away
        });
    });
}

fn bench_ghost_dag_longest_path(c: &mut Criterion) {
    let graphs = create_test_graphs();

    c.bench_function("ghost_dag_longest_path", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                for adj in &graphs {
                    let mut dag = GhostDag::<1024>::from_adjacency(adj);
                    black_box(dag.longest_path_lengths());
                }
            });
        });
    });
}

fn bench_std_dag_longest_path(c: &mut Criterion) {
    let graphs = create_test_graphs();

    c.bench_function("std_dag_longest_path", |b| {
        b.iter(|| {
            for adj in &graphs {
                let mut dag = StdDag::from_adjacency(adj);
                black_box(dag.longest_path_lengths());
            }
        });
    });
}

fn bench_ghost_dag_critical_path(c: &mut Criterion) {
    let graphs = create_test_graphs();

    c.bench_function("ghost_dag_critical_path", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                for adj in &graphs {
                    let mut dag = GhostDag::<1024>::from_adjacency(adj);
                    black_box(dag.critical_path());
                }
            });
        });
    });
}

fn bench_ghost_dag_dp_compute(c: &mut Criterion) {
    let graphs = create_test_graphs();

    c.bench_function("ghost_dag_dp_compute", |b| {
        b.iter(|| {
            GhostToken::new(|_token| {
                for adj in &graphs {
                    let mut dag = GhostDag::<1024>::from_adjacency(adj);
                    black_box(dag.dp_compute(|_node, preds| {
                        if preds.is_empty() {
                            1usize
                        } else {
                            preds.iter().map(|(_, v)| **v).sum()
                        }
                    }));
                }
            });
        });
    });
}

criterion_group!(
    benches,
    bench_ghost_dag_topological_sort,
    bench_std_dag_topological_sort,
    bench_ghost_dag_longest_path,
    bench_std_dag_longest_path,
    bench_ghost_dag_critical_path,
    bench_ghost_dag_dp_compute
);
criterion_main!(benches);
