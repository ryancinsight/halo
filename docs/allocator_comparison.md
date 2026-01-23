# Allocator Comparison: Halo vs. Snmalloc vs. Mimalloc

This document compares **Halo**, a ghost-token-based protective toolkit, with two state-of-the-art high-performance memory allocators: **Snmalloc** and **Mimalloc**.

## 1. Mimalloc (Microsoft)
**Mimalloc** is a general-purpose allocator focusing on **small object optimization**, **free list sharding**, and **performance**.

*   **Key Concept**: **Free List Sharding**. The heap is divided into small pages, each dedicated to blocks of a specific size.
*   **Concurrency**: Uses **Thread-Local Heaps (TLH)**. Each thread has its own heap, avoiding lock contention for local allocations.
*   **Deallocation**: Local frees are fast. Remote frees (freeing memory allocated by another thread) use atomic operations to add to a shared free list, which the owning thread periodically reclaims.
*   **Safety**: Standard C/C++ safety (none inherent). Relies on correct usage.

## 2. Snmalloc (Microsoft Research)
**Snmalloc** is a message-passing allocator designed for **producer-consumer** workloads.

*   **Key Concept**: **Message Passing**. Deallocation across threads is handled by sending "messages" (batches of freed pointers) to the owning thread's queue.
*   **Concurrency**: Extremely efficient for cross-thread deallocation. It avoids the contention often found in allocators that use a global lock or simple atomic lists for remote frees.
*   **Deallocation**: When a thread frees an object owned by another, it doesn't touch the remote heap directly. It appends to a message queue.
*   **Safety**: Standard C/C++ safety.

## 3. Halo (Ghost Token Paradigm)
**Halo** is not primarily an allocator but a toolkit for **safe, high-performance data structures** using **Ghost Tokens**. However, its principles extend to memory allocation.

*   **Key Concept**: **Ghost Tokens (`GhostToken<'brand>`)**. A unique, zero-sized token that represents "permission" to access a specific set of data (branded data).
*   **Concurrency**:
    *   **Single-Threaded/Exclusive**: A `GhostToken` is linear (non-cloneable). A thread holding `&mut GhostToken` has **exclusive access** to all data branded with that lifetime. This eliminates the need for locks entirely for "local" operations.
    *   **Shared Access**: A `SharedGhostToken` allows concurrent access, but mutation is restricted to atomic types (`GhostAtomic`).
    *   **Lock-Free Parallelism**: By using `SharedGhostToken` and `GhostAtomic` types, Halo enables lock-free algorithms where the "lock" is replaced by the token's compile-time guarantee.
*   **Allocation Strategy**:
    *   **Branded Heaps**: An allocator (like `BrandedSlab`) can be tied to a brand. Since only one mutable token exists, the allocator is effectively **thread-local** (or scope-local) without using Thread Local Storage (TLS). It is "passed down" the stack.
    *   **Safety**: **Compile-time Memory Safety**. You cannot use a pointer from one brand with an allocator of another brand. Use-after-free and data races are prevented by the type system.

## 4. Comparison

| Feature | Mimalloc | Snmalloc | Halo (Branded Allocator) |
| :--- | :--- | :--- | :--- |
| **Primary Goal** | General Performance, Compactness | Cross-thread Dealloc Performance | **Safety**, Correctness, Zero-Cost Abstraction |
| **Thread Safety** | Implicit (TLS) | Implicit (TLS + Msg Passing) | **Explicit** (Token passed as arg) |
| **Locks** | None (Fast path), Atomics (Remote) | None (Fast path), Atomics (Remote) | **None** (Proven exclusive by token) |
| **Remote Free** | Atomic list | Message Queue | **Restricted** (Must have token or use shared primitive) |
| **Overhead** | Low Runtime Overhead | Low Runtime Overhead | **Zero Runtime Overhead** (Compile-time checks) |

## 5. Halo's Unique Value: Lock-Free Parallelism via Tokens

Halo explores a unique niche: **Statically Verified Lock-Free Concurrency**.

1.  **Scope-Local Heaps**: Instead of relying on OS threads and TLS, Halo uses "scopes" defined by `GhostToken::new()`. An allocator created within a scope is local to that scope. This is finer-grained than a thread.
2.  **Token-Gated Parallelism**:
    *   In traditional lock-free code, you must be careful about memory ordering and ABA problems everywhere.
    *   In Halo, if you have `&mut GhostToken`, you *know* you are the only one. You can use non-atomic operations safely.
    *   If you have `&GhostToken` (shared), you are forced to use `GhostAtomic`, preventing accidental data races.
    *   **ABA Prevention**: Halo's `GenerationalPool` and other structures use the token brand to prevent ABA issues at the type level (a pointer from an old brand cannot be used with a new brand).

## Conclusion

While Mimalloc and Snmalloc focus on raw throughput and efficient cross-thread memory management, **Halo** focuses on **correctness by construction**. By leveraging the Ghost Token paradigm, Halo allows building allocators that are:
*   **Inherently Lock-Free** (for the owning scope).
*   **Type-Safe** (preventing mixing pointers from different heaps).
*   **Performance Competitive** (by removing runtime safety checks and locks).
