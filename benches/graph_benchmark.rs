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

    c.bench_function("adj_list_graph_fast_bfs", |b| {
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

                let start_id = graph.node_id_from_cell(&token, &*nodes[0]);
                let view = graph.as_fast_view(&token);
                let visited = view.bfs(start_id);
                black_box(visited.len());
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

fn bench_graph_dfs(c: &mut Criterion) {
    let size = 1000;

    c.bench_function("adj_list_graph_dfs", |b| {
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

                let start_id = graph.node_id_from_cell(&token, &*nodes[0]);
                let visited = graph.dfs(&token, start_id);
                black_box(visited.len());
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

    c.bench_function("adj_list_graph_bfs_optimized", |b| {
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

                let start_id = graph.node_id_from_cell(&token, &*nodes[0]);
                let visited = graph.bfs(&token, start_id);
                black_box(visited.len());
            })
        });
    });

    c.bench_function("adj_list_graph_bfs_iter", |b| {
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

                let start_id = graph.node_id_from_cell(&token, &*nodes[0]);
                let mut count = 0;
                for _ in graph.bfs_iter(&token, start_id) {
                    count += 1;
                }
                black_box(count);
            })
        });
    });

    c.bench_function("petgraph_bfs", |b| {
        use petgraph::visit::Bfs;
        use petgraph::Graph;

        b.iter(|| {
            let mut graph = Graph::<usize, (), petgraph::Directed>::new();
            let mut nodes = Vec::with_capacity(size);
            for i in 0..size {
                nodes.push(graph.add_node(i));
            }
            for i in 1..size {
                graph.add_edge(nodes[i / 2], nodes[i], ());
            }

            let mut bfs = Bfs::new(&graph, nodes[0]);
            let mut count = 0;
            while let Some(_) = bfs.next(&graph) {
                count += 1;
            }
            black_box(count);
        });
    });
}

fn bench_graph_snapshot(c: &mut Criterion) {
    let size = 1000;

    c.bench_function("adj_list_graph_snapshot", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = halo::graph::AdjListGraph::new();
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                for i in 1..size {
                    graph.add_edge(&mut token, &nodes[i / 2], &nodes[i], ());
                }

                GhostToken::new(|mut new_token| {
                    let (new_graph, map) = graph.snapshot(&token, &mut new_token);
                    black_box((new_graph, map));
                });
            })
        });
    });
}

fn bench_connected_components(c: &mut Criterion) {
    let size = 1000;

    c.bench_function("adj_list_graph_connected_components", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = halo::graph::AdjListGraph::new_undirected();
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                // Disconnected graph (pairs)
                for i in 0..size / 2 {
                    graph.add_undirected_edge(&mut token, &nodes[2 * i], &nodes[2 * i + 1], ());
                }

                let components = graph.connected_components(&token);
                black_box(components);
            })
        });
    });
}

fn bench_clique_remove(c: &mut Criterion) {
    let size = 100; // Increase size to see impact

    c.bench_function("adj_list_graph_clique_remove", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let graph = halo::graph::AdjListGraph::new();
                let mut nodes = Vec::with_capacity(size);
                for i in 0..size {
                    nodes.push(graph.add_node(&mut token, i));
                }
                // Create Clique
                for i in 0..size {
                    for j in 0..size {
                        if i != j {
                            graph.add_edge(&mut token, &nodes[i], &nodes[j], ());
                        }
                    }
                }

                // Remove half of the nodes
                let to_remove = size / 2;
                for _ in 0..to_remove {
                    black_box(graph.remove_node(&mut token, nodes.pop().unwrap()));
                }
            })
        });
    });
}

criterion_group!(
    benches,
    bench_graph_sparse_remove,
    bench_graph_bfs,
    bench_graph_dfs,
    bench_graph_snapshot,
    bench_connected_components,
    bench_clique_remove
);
criterion_main!(benches);
