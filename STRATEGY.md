# Halo Implementation Strategy

## Audit of Existing State

The current implementation of `halo` has established a robust foundation of GhostCell primitives:
- **Core Markers**: `GhostToken`, `GhostCell`, `InvariantLifetime` are mature.
- **Memory Layouts**: `BrandedVec`, `BrandedPool`, `BrandedBox` provide safe memory arenas.
- **Pointers**: `StaticRc` implements branded reference counting.
- **Collections**: A comprehensive suite including `Vec`, `HashMap`, `BTreeMap`, `Rope`, `SkipList`, etc.
- **Concurrency**:
    - **Atomics**: `GhostAtomicUsize`, etc.
    - **Worklists**: `GhostChaseLevDeque`, `GhostTreiberStack`.
    - **Missing**: Generalized Sync primitives (MPMC Channel), IO primitives.

## Architectural Mandate

This project aims to reimplement standard library abstractions using the GhostCell paradigm to achieve zero-cost safety and high performance.

### Key Principles
1.  **Hierarchical Branding**: Use `GhostToken` to enforce access rights at compile time.
2.  **Manual Foundations**: Avoid wrappers around `std` types. Use `std::alloc` and `NonNull` directly to control layout and performance.
3.  **Zero-Cost**: Abstractions must optimize away.
4.  **Lock-Free**: Prefer lock-free algorithms for concurrency.

## Implementation Plan

### Phase 1: Sync Primitives (Current)
The focus is on completing the concurrency toolkit.
-   **MPMC Queue (`GhostRingBuffer`)**: A lock-free bounded queue is required for general-purpose thread communication. This serves as the "Sync" primitive foundation.
    -   **Status**: Implemented (lock-free, bounded, manual allocation, cache-aligned).

### Phase 2: IO Primitives (Next)
Once Sync is solid, the next logical step is IO, which often relies on buffering and synchronization.
-   **Branded IO Buffers**: Zero-copy IO using branded buffers.
-   **Async Runtime Integration**: Potential future work.

## Implementation Details for GhostRingBuffer

`GhostRingBuffer` is a lock-free MPMC queue based on Dmitry Vyukov's algorithm.
-   **Memory**: Uses `std::alloc` for a contiguous buffer of `Slot<T>`.
-   **Alignment**: Slots are padded/aligned to 64 bytes to prevent false sharing.
-   **Safety**: Uses `GhostAtomicUsize` for sequence checking and `UnsafeCell` for data storage.
-   **Branding**: The queue carries a `'brand` lifetime to ensure it is only used within the scope of a compatible `GhostToken` (though the token itself is not strictly required for atomic ops, the branding prevents cross-domain usage).
