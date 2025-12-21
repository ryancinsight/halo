//! A compact CSR (compressed sparse row) graph with branded, lock-free visited flags.
//!
//! The primary goal is predictable memory layout:
//! - `offsets`: `Vec<usize>` of length `n + 1`
//! - `edges`: chunked contiguous `usize` edge list
//! - `visited`: `Vec<GhostAtomicBool<'brand>>` for lock-free concurrent traversals

use core::sync::atomic::Ordering;

use crate::{
    collections::ChunkedVec,
    concurrency::atomic::GhostAtomicBitset,
    concurrency::atomic::GhostAtomicBool,
    concurrency::worklist::GhostChaseLevDeque,
    concurrency::worklist::GhostTreiberStack,
};

use core::sync::atomic::AtomicUsize;

/// A CSR graph whose visited bitmap is branded.
///
/// The branding is *not* required for atomic correctness; it is used to keep this
/// graph inside the Ghost branded ecosystem and prevent accidental mixing of state
/// across unrelated token scopes in larger designs.
pub struct GhostCsrGraph<'brand, const EDGE_CHUNK: usize> {
    offsets: Vec<usize>,
    edges: ChunkedVec<usize, EDGE_CHUNK>,
    visited: Vec<GhostAtomicBool<'brand>>,
}

impl<'brand, const EDGE_CHUNK: usize> GhostCsrGraph<'brand, EDGE_CHUNK> {
    /// Builds a CSR graph from an adjacency list.
    ///
    /// # Panics
    ///
    /// Panics if any edge references a node index out of bounds.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let n = adjacency.len();

        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0);

        let mut total_edges = 0usize;
        for nbrs in adjacency {
            total_edges = total_edges.saturating_add(nbrs.len());
            offsets.push(total_edges);
        }

        let mut edges: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        edges.reserve(total_edges);

        for (u, nbrs) in adjacency.iter().enumerate() {
            for &v in nbrs {
                assert!(v < n, "edge {u}->{v} is out of bounds for n={n}");
                edges.push(v);
            }
        }

        let visited = (0..n).map(|_| GhostAtomicBool::new(false)).collect();

        Self {
            offsets,
            edges,
            visited,
        }
    }

    /// Builds a CSR graph directly from CSR parts.
    ///
    /// # Panics
    /// - if `offsets.len() < 2`
    /// - if offsets are not monotone
    /// - if `offsets.last() != edges.len()`
    pub fn from_csr_parts(offsets: Vec<usize>, edges: Vec<usize>) -> Self {
        assert!(offsets.len() >= 2, "offsets must have length n+1");
        let n = offsets.len() - 1;
        for w in offsets.windows(2) {
            assert!(w[0] <= w[1], "offsets must be monotone");
        }
        let m = *offsets.last().expect("offsets non-empty");
        assert!(m == edges.len(), "offsets last must equal edges length");
        for &v in &edges {
            assert!(v < n, "edge to {v} out of bounds for n={n}");
        }

        let mut e: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        e.reserve(edges.len());
        for v in edges {
            e.push(v);
        }
        let visited = (0..n).map(|_| GhostAtomicBool::new(false)).collect();
        Self {
            offsets,
            edges: e,
            visited,
        }
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.visited.len()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Clears the visited bitmap.
    #[inline]
    pub fn reset_visited(&self) {
        for f in &self.visited {
            f.store(false, Ordering::Relaxed);
        }
    }

    /// Parallel reachability count using a caller-provided atomic bitset for visited.
    ///
    /// This enables **zero-copy reuse** of visited storage across runs and is typically
    /// more memory-efficient than a per-node `AtomicBool`.
    pub fn parallel_reachable_count_batched_with_stack_bitset(
        &self,
        start: usize,
        threads: usize,
        stack: &GhostTreiberStack<'brand>,
        batch: usize,
        visited: &GhostAtomicBitset<'brand>,
    ) -> usize {
        assert!(threads != 0, "threads must be > 0");
        assert!(batch != 0, "batch must be > 0");
        assert!(start < self.node_count(), "start out of bounds");
        assert!(
            visited.len_bits() >= self.node_count(),
            "bitset too small for node_count"
        );

        visited.clear_all();
        stack.clear();

        let count = AtomicUsize::new(0);

        if visited.test_and_set(start, Ordering::Relaxed) {
            stack.push(start);
        } else {
            return 0;
        }

        #[derive(Copy, Clone)]
        struct WordEntry {
            word: usize,
            mask: usize,
            prev: usize,
        }

        #[derive(Copy, Clone)]
        struct Cand {
            word_idx: usize,
            mask: usize,
            v: usize,
        }

        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    let mut local = Vec::<usize>::with_capacity(batch);
                    // Per-node temporary buffers. Reused across iterations to avoid allocations.
                    // `words` stores unique word indices + OR'd masks; `cands` stores each neighbor
                    // along with the index into `words` that it belongs to.
                    let mut words: Vec<WordEntry> = Vec::with_capacity(16);
                    let mut cands: Vec<Cand> = Vec::with_capacity(64);
                    while let Some(u) = stack.pop() {
                        count.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        let start_i = self.offsets[u];
                        let end_i = self.offsets[u + 1];
                        let mut i = end_i;
                        while i > start_i {
                            i -= 1;
                            let v = unsafe { *self.edges.get_unchecked(i) };
                            // Stage bit operations per-word to reduce atomic contention:
                            // - group neighbors by bitset word
                            // - do one `fetch_or` per word
                            // - then push only newly-set neighbors.
                            //
                            // This is especially important because bitsets induce word sharing
                            // (many nodes per word), which can amplify contention.
                            let word_bits = usize::BITS as usize;
                            let w = v / word_bits;
                            let m = 1usize << (v % word_bits);

                            let mut found = None;
                            for (idx, e) in words.iter_mut().enumerate() {
                                if e.word == w {
                                    e.mask |= m;
                                    found = Some(idx);
                                    break;
                                }
                            }
                            let word_idx = match found {
                                Some(idx) => idx,
                                None => {
                                    words.push(WordEntry { word: w, mask: m, prev: 0 });
                                    words.len() - 1
                                }
                            };
                            cands.push(Cand {
                                word_idx,
                                mask: m,
                                v,
                            });
                        }

                        // Apply one fetch_or per word to determine which bits are newly set.
                        for e in &mut words {
                            // SAFETY: visited sized to node_count, and `word` derived from `v < node_count`.
                            e.prev = unsafe {
                                visited.fetch_or_word_unchecked(e.word, e.mask, Ordering::Relaxed)
                            };
                        }

                        // Push only the nodes whose bit transitioned 0->1.
                        for c in &cands {
                            if (words[c.word_idx].prev & c.mask) == 0 {
                                local.push(c.v);
                                if local.len() == batch {
                                    stack.push_batch(&local);
                                    local.clear();
                                }
                            }
                        }
                        if !local.is_empty() {
                            stack.push_batch(&local);
                            local.clear();
                        }
                        words.clear();
                        cands.clear();
                    }
                });
            }
        });

        count.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Returns `true` if `node` is currently marked visited.
    #[inline]
    pub fn is_visited(&self, node: usize) -> bool {
        self.visited[node].load(Ordering::Relaxed)
    }

    /// Marks `node` as visited and returns whether this call performed the first visit.
    #[inline]
    pub fn try_visit(&self, node: usize) -> bool {
        !self.visited[node].swap(true, Ordering::Relaxed)
    }

    /// Like `try_visit`, but without bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `node < self.node_count()`.
    #[inline(always)]
    unsafe fn try_visit_unchecked(&self, node: usize) -> bool {
        !self
            .visited
            .get_unchecked(node)
            .swap(true, Ordering::Relaxed)
    }

    /// Returns the out-neighbors of `node`.
    ///
    /// This returns an iterator to avoid allocating a `Vec`.
    pub fn neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        let start = self.offsets[node];
        let end = self.offsets[node + 1];
        (start..end).map(move |i| unsafe {
            // SAFETY: CSR construction ensures `i < edge_count()`.
            *self.edges.get_unchecked(i)
        })
    }

    /// Returns the in-neighbors of `node` (all `u` such that `u -> node`).
    ///
    /// This is \(O(m)\) (scan of all edges) for CSR.
    pub fn in_neighbors(&self, node: usize) -> Vec<usize> {
        assert!(node < self.node_count(), "node {node} out of bounds");
        let mut preds = Vec::new();
        for u in 0..self.node_count() {
            if self.neighbors(u).any(|v| v == node) {
                preds.push(u);
            }
        }
        preds
    }

    /// Returns the out-degree of a node.
    pub fn degree(&self, node: usize) -> usize {
        assert!(node < self.node_count(), "node {node} out of bounds");
        let start = self.offsets[node];
        let end = self.offsets[node + 1];
        end - start
    }

    /// Returns the in-degree of a node.
    pub fn in_degree(&self, node: usize) -> usize {
        self.in_neighbors(node).len()
    }

    /// Checks if an edge exists from `from` to `to`.
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        assert!(from < self.node_count(), "from vertex {from} out of bounds");
        assert!(to < self.node_count(), "to vertex {to} out of bounds");
        self.neighbors(from).any(|v| v == to)
    }

    /// Concurrent DFS traversal.
    pub fn dfs_reachable_count(&self, start: usize, stack: &crate::concurrency::worklist::GhostTreiberStack<'brand>) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        self.reset_visited();
        self.visited[start].store(true, Ordering::Relaxed);
        stack.push(start);

        let mut count = 1;

        while let Some(node) = stack.pop() {
            for neighbor in self.neighbors(node) {
                if !self.visited[neighbor].load(Ordering::Relaxed) {
                    self.visited[neighbor].store(true, Ordering::Relaxed);
                    stack.push(neighbor);
                    count += 1;
                }
            }
        }

        count
    }

    /// Concurrent BFS traversal.
    pub fn bfs_reachable_count(&self, start: usize, deque: &crate::concurrency::worklist::GhostChaseLevDeque<'brand>) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        self.reset_visited();
        self.visited[start].store(true, Ordering::Relaxed);
        assert!(deque.push_bottom(start), "deque capacity too small");

        let mut count = 1;

        while let Some(node) = deque.steal() {
            for neighbor in self.neighbors(node) {
                if !self.visited[neighbor].load(Ordering::Relaxed) {
                    self.visited[neighbor].store(true, Ordering::Relaxed);
                    assert!(deque.push_bottom(neighbor), "deque capacity too small");
                    count += 1;
                }
            }
        }

        count
    }

    /// Depth-first traversal using an explicit stack, guarded by an atomic visited bitmap.
    ///
    /// This is safe to run concurrently from multiple threads: the only shared mutation
    /// is `visited` (atomics). The returned order is deterministic only for single-threaded
    /// execution.
    pub fn dfs(&self, start: usize) -> Vec<usize> {
        assert!(start < self.node_count(), "start out of bounds");

        let mut out = Vec::new();
        let mut stack = Vec::new();

        if self.try_visit(start) {
            stack.push(start);
        } else {
            return out;
        }

        while let Some(u) = stack.pop() {
            out.push(u);

            // Push neighbors in reverse for a more conventional DFS order when adjacency
            // is in ascending order.
            let start_i = unsafe { *self.offsets.get_unchecked(u) };
            let end_i = unsafe { *self.offsets.get_unchecked(u + 1) };
            let mut i = end_i;
            while i > start_i {
                i -= 1;
                let v = unsafe { *self.edges.get_unchecked(i) };
                // SAFETY: `from_*` constructors ensure all `v < node_count()`.
                if unsafe { self.try_visit_unchecked(v) } {
                    stack.push(v);
                }
            }
        }

        out
    }

    /// Depth-first traversal that returns only the reachable node count.
    ///
    /// This is the same traversal as [`dfs`](Self::dfs), but avoids building an output
    /// vector and therefore is a better baseline for benchmarking “reachable count”
    /// style algorithms.
    pub fn dfs_count(&self, start: usize) -> usize {
        assert!(start < self.node_count(), "start out of bounds");

        let mut stack = Vec::new();
        let mut count = 0usize;

        if self.try_visit(start) {
            stack.push(start);
        } else {
            return 0;
        }

        while let Some(u) = stack.pop() {
            count += 1;

            // Push neighbors in reverse for a more conventional DFS order when adjacency
            // is in ascending order.
            let start_i = unsafe { *self.offsets.get_unchecked(u) };
            let end_i = unsafe { *self.offsets.get_unchecked(u + 1) };
            let mut i = end_i;
            while i > start_i {
                i -= 1;
                let v = unsafe { *self.edges.get_unchecked(i) };
                // SAFETY: `from_*` constructors ensure all `v < node_count()`.
                if unsafe { self.try_visit_unchecked(v) } {
                    stack.push(v);
                }
            }
        }

        count
    }

    /// Parallel reachability count using a lock-free worklist + atomic visited.
    ///
    /// This is a "real" parallel traversal: multiple threads pop nodes from a shared
    /// lock-free stack and push newly discovered neighbors.
    ///
    /// # Panics
    /// Panics if `threads == 0` or `start` is out of bounds.
    pub fn parallel_reachable_count(&self, start: usize, threads: usize) -> usize {
        let stack: GhostTreiberStack<'brand> = GhostTreiberStack::new(self.node_count());
        self.parallel_reachable_count_with_stack(start, threads, &stack)
    }

    /// Parallel reachability count using a caller-provided lock-free worklist.
    ///
    /// This exists to let benchmarks amortize allocation of the worklist.
    pub fn parallel_reachable_count_with_stack(
        &self,
        start: usize,
        threads: usize,
        stack: &GhostTreiberStack<'brand>,
    ) -> usize {
        assert!(threads != 0, "threads must be > 0");
        assert!(start < self.node_count(), "start out of bounds");

        self.reset_visited();
        stack.clear();

        let count = AtomicUsize::new(0);

        if self.try_visit(start) {
            stack.push(start);
        } else {
            return 0;
        }

        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    while let Some(u) = stack.pop() {
                        count.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        let start_i = self.offsets[u];
                        let end_i = self.offsets[u + 1];
                        let mut i = end_i;
                        while i > start_i {
                            i -= 1;
                            let v = unsafe { *self.edges.get_unchecked(i) };
                            // SAFETY: constructors ensure all edges are in-bounds.
                            if unsafe { self.try_visit_unchecked(v) } {
                                stack.push(v);
                            }
                        }
                    }
                });
            }
        });

        count.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Parallel reachability count with per-thread batching to reduce shared worklist contention.
    ///
    /// Each worker accumulates discovered nodes into a local buffer and flushes them
    /// to the shared worklist using a single-CAS batch splice.
    pub fn parallel_reachable_count_batched_with_stack(
        &self,
        start: usize,
        threads: usize,
        stack: &GhostTreiberStack<'brand>,
        batch: usize,
    ) -> usize {
        assert!(threads != 0, "threads must be > 0");
        assert!(batch != 0, "batch must be > 0");
        assert!(start < self.node_count(), "start out of bounds");

        self.reset_visited();
        stack.clear();

        let count = AtomicUsize::new(0);

        if self.try_visit(start) {
            stack.push(start);
        } else {
            return 0;
        }

        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    let mut local = Vec::<usize>::with_capacity(batch);
                    while let Some(u) = stack.pop() {
                        count.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

                        let start_i = self.offsets[u];
                        let end_i = self.offsets[u + 1];
                        let mut i = end_i;
                        while i > start_i {
                            i -= 1;
                            let v = unsafe { *self.edges.get_unchecked(i) };
                            // SAFETY: constructors ensure all edges are in-bounds.
                            if unsafe { self.try_visit_unchecked(v) } {
                                local.push(v);
                                if local.len() == batch {
                                    stack.push_batch(&local);
                                    local.clear();
                                }
                            }
                        }

                        if !local.is_empty() {
                            stack.push_batch(&local);
                            local.clear();
                        }
                    }
                });
            }
        });

        count.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Parallel reachability count using Chase–Lev work-stealing deques.
    ///
    /// Each worker owns one deque (push/pop bottom). When its deque is empty it
    /// attempts to steal from others (steal top). A global `outstanding` counter
    /// provides termination without locks.
    pub fn parallel_reachable_count_workstealing_with_deques(
        &self,
        start: usize,
        deques: &[GhostChaseLevDeque<'brand>],
    ) -> usize {
        let threads = deques.len();
        assert!(threads != 0, "need at least 1 deque");
        assert!(start < self.node_count(), "start out of bounds");

        self.reset_visited();
        for d in deques {
            d.clear();
        }

        let outstanding = AtomicUsize::new(0);
        let count = AtomicUsize::new(0);

        if self.try_visit(start) {
            assert!(deques[0].push_bottom(start), "deque capacity too small");
            outstanding.store(1, core::sync::atomic::Ordering::Relaxed);
        } else {
            return 0;
        }

        std::thread::scope(|scope| {
            let outstanding = &outstanding;
            let count = &count;
            for tid in 0..threads {
                scope.spawn(move || {
                    let me = &deques[tid];
                    loop {
                        let task = me.pop_bottom().or_else(|| {
                            // steal round-robin
                            for k in 1..threads {
                                let victim = &deques[(tid + k) % threads];
                                if let Some(x) = victim.steal() {
                                    return Some(x);
                                }
                            }
                            None
                        });

                        let Some(u) = task else {
                            if outstanding.load(core::sync::atomic::Ordering::Acquire) == 0 {
                                break;
                            }
                            core::hint::spin_loop();
                            continue;
                        };

                        count.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

                        let start_i = self.offsets[u];
                        let end_i = self.offsets[u + 1];
                        let mut i = end_i;
                        while i > start_i {
                            i -= 1;
                            let v = unsafe { *self.edges.get_unchecked(i) };
                            // SAFETY: constructors ensure all edges are in-bounds.
                            if unsafe { self.try_visit_unchecked(v) } {
                                // Account for new work first, then push.
                                outstanding.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                                let ok = me.push_bottom(v);
                                assert!(ok, "deque capacity too small");
                            }
                        }

                        outstanding.fetch_sub(1, core::sync::atomic::Ordering::Release);
                    }
                });
            }
        });

        count.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Convenience wrapper that allocates deques of size `capacity` (power-of-two) and runs work-stealing.
    pub fn parallel_reachable_count_workstealing(&self, start: usize, threads: usize) -> usize {
        assert!(threads != 0);
        let cap = self.node_count().next_power_of_two().max(64);
        let deques: Vec<GhostChaseLevDeque<'brand>> =
            (0..threads).map(|_| GhostChaseLevDeque::new(cap)).collect();
        self.parallel_reachable_count_workstealing_with_deques(start, &deques)
    }
}


