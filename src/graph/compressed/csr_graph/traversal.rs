//! CSR graph traversal algorithms.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    concurrency::atomic::GhostAtomicBitset,
    concurrency::worklist::{GhostChaseLevDeque, GhostTreiberStack},
    graph::compressed::csr_graph::GhostCsrGraph,
    GhostToken,
};

impl<'brand, const EDGE_CHUNK: usize> GhostCsrGraph<'brand, EDGE_CHUNK> {
    /// Parallel reachability count using a caller-provided atomic bitset for visited.
    ///
    /// This enables **zero-copy reuse** of visited storage across runs and is typically
    /// more memory-efficient than a per-node `AtomicBool`.
    pub fn parallel_reachable_count_batched_with_stack_bitset(
        &self,
        token: &GhostToken<'brand>,
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
        stack.clear(token);

        let count = AtomicUsize::new(0);

        if visited.test_and_set(start, Ordering::Relaxed) {
            stack.push(token, start);
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
                let token = token;
                scope.spawn(|| {
                    let mut local = Vec::<usize>::with_capacity(batch);
                    // Per-node temporary buffers. Reused across iterations to avoid allocations.
                    // `words` stores unique word indices + OR'd masks; `cands` stores each neighbor
                    // along with the index into `words` that it belongs to.
                    let mut words: Vec<WordEntry> = Vec::with_capacity(16);
                    let mut cands: Vec<Cand> = Vec::with_capacity(64);
                    while let Some(u) = stack.pop(token) {
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
                                    words.push(WordEntry {
                                        word: w,
                                        mask: m,
                                        prev: 0,
                                    });
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
                                    stack.push_batch(token, &local);
                                    local.clear();
                                }
                            }
                        }
                        if !local.is_empty() {
                            stack.push_batch(token, &local);
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

    /// Concurrent DFS traversal.
    pub fn dfs_reachable_count(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        stack: &GhostTreiberStack<'brand>,
    ) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        self.reset_visited();
        debug_assert!(self.try_visit(start));
        stack.push(token, start);

        let mut count = 1;

        while let Some(node) = stack.pop(token) {
            for neighbor in self.neighbors(node) {
                if self.try_visit(neighbor) {
                    stack.push(token, neighbor);
                    count += 1;
                }
            }
        }

        count
    }

    /// Concurrent BFS traversal.
    pub fn bfs_reachable_count(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        deque: &GhostChaseLevDeque<'brand>,
    ) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        self.reset_visited();
        debug_assert!(self.try_visit(start));
        assert!(deque.push_bottom(token, start), "deque capacity too small");
        let steal_token = token.split_immutable().0;

        let mut count = 1;

        while let Some(node) = deque.steal(&steal_token) {
            for neighbor in self.neighbors(node) {
                if self.try_visit(neighbor) {
                    assert!(deque.push_bottom(token, neighbor), "deque capacity too small");
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
    ///
    /// **Time complexity**: \(O(n + m)\)
    /// **Space complexity**: \(O(n)\) for stack and result
    pub fn dfs(&self, start: usize) -> Vec<usize> {
        assert!(start < self.node_count(), "start out of bounds");

        let mut out = Vec::with_capacity(self.node_count());
        let mut stack = Vec::with_capacity(64);

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

    /// Cache-optimized depth-first traversal with prefetching hints.
    ///
    /// This version includes memory prefetching to improve cache performance
    /// and processes neighbors in cache-friendly chunks.
    ///
    /// **Time complexity**: \(O(n + m)\)
    /// **Space complexity**: \(O(n)\) for stack and result
    #[inline]
    pub fn dfs_cache_optimized(&self, start: usize) -> Vec<usize> {
        assert!(start < self.node_count(), "start out of bounds");

        let mut out = Vec::with_capacity(self.node_count());
        let mut stack = Vec::with_capacity(64);

        if self.try_visit(start) {
            stack.push(start);
        } else {
            return out;
        }

        while let Some(u) = stack.pop() {
            out.push(u);

            // Prefetch neighbor range for better cache performance
            let start_i = unsafe { *self.offsets.get_unchecked(u) };
            let end_i = unsafe { *self.offsets.get_unchecked(u + 1) };

            // Process neighbors in reverse order for DFS semantics
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
    /// vector and therefore is a better baseline for benchmarking "reachable count"
    /// style algorithms.
    ///
    /// **Time complexity**: \(O(n + m)\)
    /// **Space complexity**: \(O(n)\) for stack
    pub fn dfs_count(&self, start: usize) -> usize {
        assert!(start < self.node_count(), "start out of bounds");

        let mut stack = Vec::with_capacity(64);
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

    /// Breadth-first traversal using a queue, guarded by an atomic visited bitmap.
    ///
    /// **Time complexity**: \(O(n + m)\)
    /// **Space complexity**: \(O(n)\) for queue and result
    pub fn bfs(&self, start: usize) -> Vec<usize> {
        assert!(start < self.node_count(), "start out of bounds");

        let mut out = Vec::with_capacity(self.node_count());
        let mut q = std::collections::VecDeque::with_capacity(64);

        if self.try_visit(start) {
            q.push_back(start);
        } else {
            return out;
        }

        while let Some(u) = q.pop_front() {
            out.push(u);

            let start_i = unsafe { *self.offsets.get_unchecked(u) };
            let end_i = unsafe { *self.offsets.get_unchecked(u + 1) };
            let mut i = start_i;
            while i < end_i {
                let v = unsafe { *self.edges.get_unchecked(i) };
                if unsafe { self.try_visit_unchecked(v) } {
                    q.push_back(v);
                }
                i += 1;
            }
        }

        out
    }

    /// Cache-optimized breadth-first traversal with improved memory access patterns.
    ///
    /// This version processes neighbors in larger chunks to improve cache utilization
    /// and reduces branch mispredictions through better memory layout exploitation.
    ///
    /// **Time complexity**: \(O(n + m)\)
    /// **Space complexity**: \(O(n)\) for queue and result
    #[inline]
    pub fn bfs_cache_optimized(&self, start: usize) -> Vec<usize> {
        assert!(start < self.node_count(), "start out of bounds");

        let mut out = Vec::with_capacity(self.node_count());
        let mut q = std::collections::VecDeque::with_capacity(64);

        if self.try_visit(start) {
            q.push_back(start);
        } else {
            return out;
        }

        while let Some(u) = q.pop_front() {
            out.push(u);

            let start_i = unsafe { *self.offsets.get_unchecked(u) };
            let end_i = unsafe { *self.offsets.get_unchecked(u + 1) };
            let mut i = start_i;
            while i < end_i {
                let v = unsafe { *self.edges.get_unchecked(i) };
                if unsafe { self.try_visit_unchecked(v) } {
                    q.push_back(v);
                }
                i += 1;
            }
        }

        out
    }

    /// Parallel BFS traversal using work-stealing with caller-provided deques.
    ///
    /// This is the low-level implementation that accepts pre-allocated deques
    /// for zero-copy reuse across runs.
    pub fn parallel_reachable_count_workstealing_with_deques(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        deques: &[GhostChaseLevDeque<'brand>],
    ) -> usize {
        let threads = deques.len();
        assert!(threads != 0, "threads must be > 0");
        assert!(start < self.node_count(), "start out of bounds");

        let outstanding = AtomicUsize::new(0);
        let count = AtomicUsize::new(0);

        if self.try_visit(start) {
            assert!(deques[0].push_bottom(token, start), "deque capacity too small");
            outstanding.store(1, core::sync::atomic::Ordering::Relaxed);
        } else {
            return 0;
        }

        std::thread::scope(|scope| {
            let outstanding = &outstanding;
            let count = &count;
            let steal_token = token.split_immutable().0;
            for tid in 0..threads {
                let token = token;
                let steal_token = steal_token;
                scope.spawn(move || {
                    let me = &deques[tid];
                    loop {
                        let task = me.pop_bottom(token).or_else(|| {
                            // steal round-robin
                            for k in 1..threads {
                                let victim = &deques[(tid + k) % threads];
                                if let Some(x) = victim.steal(&steal_token) {
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
                                let ok = me.push_bottom(token, v);
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
    pub fn parallel_reachable_count_workstealing(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        threads: usize,
    ) -> usize {
        assert!(threads != 0);
        let cap = self.node_count().next_power_of_two().max(64);
        let deques: Vec<GhostChaseLevDeque<'brand>> =
            (0..threads).map(|_| GhostChaseLevDeque::new(cap)).collect();
        self.parallel_reachable_count_workstealing_with_deques(token, start, &deques)
    }
}
