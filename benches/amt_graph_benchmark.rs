use criterion::{criterion_group, criterion_main, Criterion, BatchSize};
use halo::graph::specialized::amt_graph::GhostAmtGraph;

fn bench_amt_upgrade(c: &mut Criterion) {
    // Existing benchmark
    c.bench_function("amt_upgrade_full_lifecycle", |b| {
        b.iter(|| {
             let node_count = 2000;
             let mut graph = GhostAmtGraph::<32>::new(node_count);
             for i in 1..1050 {
                 graph.add_edge(0, i);
             }
        });
    });

    // Focused benchmark
    c.bench_function("amt_upgrade_dense_only", |b| {
        b.iter_batched(
            || {
                 let node_count = 2000;
                 let mut graph = GhostAmtGraph::<32>::new(node_count);
                 // Prepare state right before upgrade
                 // We add 1023 edges: 1..1024
                 for i in 1..1024 {
                     graph.add_edge(0, i);
                 }
                 graph
            },
            |mut graph| {
                // This adds the 1024th edge, triggering upgrade to Dense (threshold is 1024)
                graph.add_edge(0, 1024);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_amt_upgrade);
criterion_main!(benches);
