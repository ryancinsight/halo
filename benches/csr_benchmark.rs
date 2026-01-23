use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::graph::compressed::csr_graph::GhostCsrGraph;

fn bench_csr_in_neighbors(c: &mut Criterion) {
    let nodes = 1000;
    // Create a dense graph to emphasize the O(M) cost
    let mut adjacency = vec![Vec::new(); nodes];
    for i in 0..nodes {
        // Connect each node to 100 random other nodes (or fixed pattern)
        for j in 0..100 {
            let target = (i + j * 7) % nodes;
            adjacency[i].push(target);
        }
    }

    // Sort neighbors as typically required/good practice for CSR construction from adj
    for nbrs in &mut adjacency {
        nbrs.sort();
        nbrs.dedup();
    }

    // CSR Graph with chunk size 32
    let graph = GhostCsrGraph::<32>::from_adjacency(&adjacency);

    c.bench_function("csr_in_neighbors", |b| {
        b.iter(|| {
            // Check in-neighbors for a few nodes
            for i in 0..10 {
                let target = (i * 100) % nodes;
                black_box(graph.in_neighbors(target));
            }
        });
    });

    c.bench_function("csr_in_degree", |b| {
        b.iter(|| {
            for i in 0..10 {
                let target = (i * 100) % nodes;
                black_box(graph.in_degree(target));
            }
        });
    });
}

criterion_group!(benches, bench_csr_in_neighbors);
criterion_main!(benches);
