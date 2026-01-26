use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::graph::specialized::GhostLelGraph;

fn bench_lel_from_adjacency(c: &mut Criterion) {
    let size = 10_000;
    let edges_per_node = 50;

    // Create a random-ish adjacency list
    let mut adjacency = Vec::with_capacity(size);
    for i in 0..size {
        let mut neighbors = Vec::with_capacity(edges_per_node);
        for j in 0..edges_per_node {
            // Pseudo-random edges
            neighbors.push((i + j * 17) % size);
        }
        adjacency.push(neighbors);
    }

    c.bench_function("lel_graph_from_adjacency", |b| {
        b.iter(|| {
            black_box(GhostLelGraph::from_adjacency(&adjacency));
        });
    });
}

criterion_group!(benches, bench_lel_from_adjacency);
criterion_main!(benches);
