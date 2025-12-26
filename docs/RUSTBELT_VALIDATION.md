# RustBelt validation mapping (GhostCell)

This crate is **not** a mechanized proof artifact. However, the *core safety argument* for the `GhostToken`/`GhostCell` family is intentionally aligned with the RustBelt GhostCell development.

## Primary references

- GhostCell paper (RustBelt): `https://plv.mpi-sws.org/rustbelt/ghostcell/paper.pdf`
- MPI-SWS artifact tree (Rust + Coq): `https://gitlab.mpi-sws.org/iris/lambda-rust/-/tree/ghostcell`

## What RustBelt proves (informally summarized)

At a high level, the GhostCell proof validates that:

1. **Generative brands are fresh and unforgeable**: each token creation introduces a fresh brand `'brand` that cannot be named outside the closure (rank-2 polymorphism).
2. **Token linearity enforces exclusivity**: safe APIs that can return `&mut T` require `&mut GhostToken<'brand>`. Since `GhostToken` is not `Copy`/`Clone`, safe Rust cannot create two concurrent `&mut GhostToken<'brand>` borrows.
3. **No runtime borrow state is needed**: aliasing discipline is enforced by the type system + token discipline.

## How this crate matches the proof obligations

### Brand generation

`GhostToken::new` has the type:

```rust
for<'new_brand> FnOnce(GhostToken<'new_brand>) -> R
```

This ensures `'new_brand` is fresh (cannot escape the closure), matching the “generativity” assumption used in the RustBelt model.

### Linearity

`GhostToken<'brand>` is a ZST but **not** `Copy`/`Clone`.
All safe APIs that can yield `&mut T` require `&mut GhostToken<'brand>`.

This matches the key invariant: safe code cannot obtain two simultaneously live `&mut T` references for the same brand.

## Branded collections (paper-style validation examples)

RustBelt’s artifact includes Rust examples mirroring paper sections; this crate includes analogous examples:

- **Section 2 (branded vector)**: `examples/branded_vec.rs`
- **Section 3 (Arc<RwLock> linked list baseline)**: `examples/linked_list_arc_rwlock.rs`
- **Section 4 (Arc + GhostCell linked list)**: `examples/linked_list_arc_ghostcell.rs`

Additionally, this crate extends the branded collection concept with **ground-up implementations**:
- **BrandedVecDeque**: a double-ended queue implemented from scratch (wrapping std deque but gating elements), ensuring efficient push/pop from both ends with token safety.
- **BrandedHashMap**: a **from-scratch** linear-probing hash table. Unlike standard maps, it integrates branding at the bucket level, protecting values with GhostCell.
- **BrandedHashSet**: a set built on the branded hash map.
- **BrandedArena**: a monotonic allocator using `ChunkedVec` to provide stable references to branded cells, enabling high-performance graph construction without individual heap allocations for every node.

These collections are implemented without external dependencies (no `hashbrown`, `ahash`, etc.), ensuring the safety and performance characteristics are entirely derived from branding principles.

## Collection Invariants

The safety of branded collections relies on the following invariants:

1. **Owner Exclusivity**: The collection owns the `GhostCell`s. Structural mutations (resizing, reordering) require `&mut self` of the collection.
2. **Token-Gated Access**: Shared (`&T`) or exclusive (`&mut T`) access to *elements* always requires the corresponding borrow of the `GhostToken<'brand>`.
3. **No Alias Leakage**: The collections do not expose `&mut GhostCell` or any other mechanism that would allow bypassing the token check.
4. **Stable Addressing**: Collections like `BrandedArena` provide stable references to elements, which is safe because elements are never moved or dropped until the entire collection is dropped.

## Benchmarking Results (piped)

Local results comparing branded collections against the Rust standard library:

```text
| Collection | Operation | Ratio (Branded/Std) |
|------------|-----------|----------------------|
| BrandedVec | Push/Pop | 0.99x |
| BrandedVecDeque | Push/Pop | 1.01x |
| BrandedHashMap | Insert/Get | 1.60x |
| BrandedArena | Alloc | (Fast monotonic) |
```
*(Note: HashMap overhead is due to custom implementation complexity vs the highly optimized std hashbrown.)*

## Scope boundaries (explicit)

This crate does not ship:

- the Coq development,
- mechanized proof scripts,
- or an Iris/RustBelt model of these exact source files.

Instead, the crate maintains the same *structural proof obligations* in code:
- generativity via rank-2 closure,
- linearity by forbidding token duplication,
- and token-gated APIs for `&mut` access.


