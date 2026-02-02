# Backlog

## High Priority
- [ ] **Property Testing Infrastructure**: Add `proptest` to `dev-dependencies` and create initial property tests for `BrandedBPlusTree` and `BrandedChunkedVec`.
- [ ] **Mathematical Specifications**: Document formal invariants for `AdjListGraph` (connectivity, acyclicity where applicable) and `BPlusTree` (B-factor, ordering).
- [ ] **Documentation Audit**: Ensure all public items in `src/collections` and `src/graph` have rustdoc comments with Invariants sections.

## Medium Priority
- [ ] **Performance Validation**: Run benchmarks (`criterion`) and compare against standard library equivalents (as seen in `benches/`).
- [ ] **Negative Testing**: Add tests specifically designed to fail (e.g., accessing with wrong token, although this should be a compile-time failure, we can test "compile_fail" scenarios).

## Low Priority
- [ ] **Example Expansion**: Add more complex usage examples in `examples/` demonstrating the hierarchical token usage.
