//! Compressed graph traversal algorithms.

/// Breadth-first traversal optimized for compressed format.
///
/// Uses the compressed representation efficiently while maintaining
/// good cache performance through batched decompression.
#[inline]
pub fn bfs<'brand, const EDGE_CHUNK: usize>(
    graph: &super::GhostCompressedGraph<'brand, EDGE_CHUNK>,
    start: usize,
) -> Vec<usize> {
    assert!(start < graph.node_count(), "start out of bounds");

    let mut out = Vec::with_capacity(graph.node_count());
    let mut q = std::collections::VecDeque::with_capacity(64);

    if graph.try_visit(start) {
        q.push_back(start);
    } else {
        return out;
    }

    while let Some(u) = q.pop_front() {
        out.push(u);

        // Process neighbors from compressed format
        for v in graph.neighbors(u) {
            if graph.try_visit(v) {
                q.push_back(v);
            }
        }
    }

    out
}

/// Returns compression statistics for analysis.
pub fn compression_stats<'brand, const EDGE_CHUNK: usize>(
    graph: &super::GhostCompressedGraph<'brand, EDGE_CHUNK>,
) -> CompressionStats {
    let original_offsets_size = (graph.node_count + 1) * std::mem::size_of::<usize>();
    let compressed_offsets_size = graph.offsets.values_len() * std::mem::size_of::<usize>() +
                                graph.offsets.runs_len() * std::mem::size_of::<usize>();

    let original_edges_size = graph.edge_count * std::mem::size_of::<usize>();
    let compressed_edges_size = graph.edges.len() * std::mem::size_of::<usize>(); // Edges uncompressed

    CompressionStats {
        original_size: original_offsets_size + original_edges_size,
        compressed_size: compressed_offsets_size + compressed_edges_size,
        node_count: graph.node_count,
        edge_count: graph.edge_count,
    }
}

/// Compression statistics for analysis and optimization
#[derive(Debug, Clone)]
pub struct CompressionStats {
    /// Original uncompressed size in bytes
    pub original_size: usize,
    /// Compressed size in bytes
    pub compressed_size: usize,
    /// Number of nodes
    pub node_count: usize,
    /// Number of edges
    pub edge_count: usize,
}

impl CompressionStats {
    /// Returns the compression ratio (higher is better)
    #[inline]
    pub fn compression_ratio(&self) -> f64 {
        if self.compressed_size == 0 {
            0.0
        } else {
            self.original_size as f64 / self.compressed_size as f64
        }
    }

    /// Returns memory savings as a percentage
    #[inline]
    pub fn memory_savings_percent(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            let diff = if self.compressed_size > self.original_size {
                -((self.compressed_size - self.original_size) as i64)
            } else {
                (self.original_size - self.compressed_size) as i64
            };
            (diff as f64 / self.original_size as f64) * 100.0
        }
    }
}
