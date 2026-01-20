use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::graph::{BrandedPoolGraph, GhostAdjacencyGraph};
use halo::GhostToken;

fn bench_graph_sparse_remove(c: &mut Criterion) {
    let size = 1000;

    c.bench_function("pool_graph_sparse_remove", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = BrandedPoolGraph::<usize, ()>::with_capacity(size);
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                // Chain: 0->1->...->N
                for i in 0..size - 1 {
                    graph.add_edge(&mut token, nodes[i], nodes[i + 1], ());
                }

                // Remove middle node
                black_box(graph.remove_node(&mut token, nodes[size / 2]));
            })
        });
    });

    c.bench_function("adj_graph_sparse_remove", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut graph = GhostAdjacencyGraph::new(size);
                // Chain
                for i in 0..size - 1 {
                    graph.add_edge(&mut token, i, i + 1);
                }

                // Remove middle node
                black_box(graph.remove_vertex(&mut token, size / 2));
            })
        });
    });

    c.bench_function("adj_list_graph_sparse_remove", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = halo::graph::AdjListGraph::new();
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                // Chain
                for i in 0..size - 1 {
                    graph.add_edge(&mut token, &nodes[i], &nodes[i + 1], ());
                }

                // Remove middle node.
                // Since NodeHandle is linear, we must take it from the vector.
                let mid_node = nodes.swap_remove(size / 2);
                black_box(graph.remove_node(&mut token, mid_node));

                // Cleanup remaining nodes
                for node in nodes {
                    graph.remove_node(&mut token, node);
                }
            })
        });
    });
}

fn bench_graph_bfs(c: &mut Criterion) {
    let size = 1000;

    c.bench_function("pool_graph_bfs", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = BrandedPoolGraph::<usize, ()>::with_capacity(size);
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                // Tree-like structure
                for i in 1..size {
                    graph.add_edge(&mut token, nodes[i / 2], nodes[i], ());
                }

                // BFS manually
                let mut q = std::collections::VecDeque::new();
                let mut visited = std::collections::HashSet::new();
                q.push_back(nodes[0]);
                visited.insert(nodes[0]);

                let mut count = 0;
                while let Some(u) = q.pop_front() {
                    count += 1;
                    for (v, _) in graph.neighbors(&token, u) {
                        if visited.insert(v) {
                            q.push_back(v);
                        }
                    }
                }
                black_box(count);

                // Cleanup
                for node in nodes {
                    graph.remove_node(&mut token, node);
                }
            })
        });
    });

    c.bench_function("adj_list_graph_bfs", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = halo::graph::AdjListGraph::new();
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                // Tree-like
                for i in 1..size {
                    graph.add_edge(&mut token, &nodes[i / 2], &nodes[i], ());
                }

                let mut q = std::collections::VecDeque::new();
                let mut visited = std::collections::HashSet::new();

                // Use pointer address for visited set
                let start_node = &*nodes[0];
                q.push_back(start_node);
                visited.insert(std::ptr::from_ref(start_node) as usize);

                let mut count = 0;
                while let Some(u) = q.pop_front() {
                    count += 1;
                    for (v, _) in graph.neighbors(&token, u) {
                        if visited.insert(std::ptr::from_ref(v) as usize) {
                            q.push_back(v);
                        }
                    }
                }
                black_box(count);
            })
        });
    });

    c.bench_function("adj_graph_bfs", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut graph = GhostAdjacencyGraph::new(size);
                for i in 1..size {
                    graph.add_edge(&mut token, i / 2, i);
                }

                let mut q = std::collections::VecDeque::new();
                let mut visited = std::collections::HashSet::new();
                q.push_back(0);
                visited.insert(0);

                let mut count = 0;
                while let Some(u) = q.pop_front() {
                    count += 1;
                    for v in graph.out_neighbors(&token, u) {
                        if visited.insert(v) {
                            q.push_back(v);
                        }
                    }
                }
                black_box(count);
            })
        });
    });
}

criterion_group!(benches, bench_graph_sparse_remove, bench_graph_bfs);
criterion_main!(benches);
