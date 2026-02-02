# Checklist

## Phase 3: Closure & Verification

- [ ] **Property Testing**
    - [ ] Add `proptest` to `Cargo.toml`.
    - [ ] Implement `proptest` strategy for `BrandedBPlusTree`.
    - [ ] Verify `insert`/`get` consistency against `std::collections::BTreeMap`.

- [ ] **Documentation**
    - [ ] Add Invariant documentation to `BrandedBPlusTree::insert`.
    - [ ] Add Invariant documentation to `BrandedChunkedVec::push`.
    - [ ] Add Invariant documentation to `AdjListGraph::dfs`.

- [ ] **Mathematical Specs**
    - [ ] Define Invariants for `BPlusTree` (node utilization, sorted keys).
    - [ ] Define Invariants for `ChunkedVec` (chunk capacity, indexing).
