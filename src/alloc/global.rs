//! Global allocator integration for branded allocators.
//!
//! This module provides utilities to route the global allocator to a `GhostAlloc`
//! instance within a specific scope or thread.
//!
//! # Safety
//!
//! Replacing the global allocator is inherently unsafe if allocations escape the scope
//! where the custom allocator is active. The user must ensure that any memory allocated
//! via the global allocator (e.g., `Box::new`) while a custom allocator is active is
//! deallocated before the scope ends, or that the custom allocator can safely handle
//! deallocation after the scope (which is generally not true for branded allocators).

use crate::alloc::GhostAlloc;
use crate::GhostToken;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::ptr::NonNull;
use std::alloc::System;

/// A wrapper that dispatches `GlobalAlloc` calls to a thread-local `GhostAlloc`.
///
/// Use this as the global allocator:
///
/// ```rust,ignore
/// #[global_allocator]
/// static GLOBAL: halo::alloc::global::DispatchGlobalAlloc = halo::alloc::global::DispatchGlobalAlloc;
/// ```
pub struct DispatchGlobalAlloc;

// Wrapper to adapt (GhostAlloc + Token) to GlobalAlloc.
struct ScopedAdapter<'a, 'brand, A: GhostAlloc<'brand>> {
    allocator: &'a A,
    token: &'a GhostToken<'brand>,
}

unsafe impl<'a, 'brand, A: GhostAlloc<'brand>> GlobalAlloc for ScopedAdapter<'a, 'brand, A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.allocator.allocate(self.token, layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(ptr) = NonNull::new(ptr) {
            self.allocator.deallocate(self.token, ptr, layout);
        }
    }
}

// Thread-local storage for the current allocator.
// We use `Cell<Option<NonNull<dyn GlobalAlloc>>>`.
// Since `dyn GlobalAlloc` is a fat pointer (data + vtable), `NonNull` handles it correctly.
// We need to transmute the lifetime away because TLS requires 'static, but we ensure
// validity via scoping in `with_global_allocator`.
thread_local! {
    static CURRENT_ALLOCATOR: Cell<Option<NonNull<dyn GlobalAlloc>>> = const { Cell::new(None) };
    static IN_ALLOCATOR: Cell<bool> = const { Cell::new(false) };
}

unsafe impl GlobalAlloc for DispatchGlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Fast path: if no custom allocator is active, bypass TLS checks and guards
        // to minimize overhead for the common case.
        let alloc_ptr = if let Some(ptr) = CURRENT_ALLOCATOR.get() {
            ptr
        } else {
            return System.alloc(layout);
        };

        // Prevent recursion: if we are already in the allocator (e.g. BrandedSlab asking for a page),
        // fallback to System.
        if IN_ALLOCATOR.get() {
            return System.alloc(layout);
        }

        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                IN_ALLOCATOR.set(false);
            }
        }

        IN_ALLOCATOR.set(true);
        let _guard = Guard;

        alloc_ptr.as_ref().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Fast path: check if custom allocator is active.
        let alloc_ptr = if let Some(ptr) = CURRENT_ALLOCATOR.get() {
            ptr
        } else {
            return System.dealloc(ptr, layout);
        };

        // Prevent recursion during deallocation
        if IN_ALLOCATOR.get() {
            return System.dealloc(ptr, layout);
        }

        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                IN_ALLOCATOR.set(false);
            }
        }

        IN_ALLOCATOR.set(true);
        let _guard = Guard;

        alloc_ptr.as_ref().dealloc(ptr, layout);
    }
}

/// Executes a closure using the provided branded allocator as the global allocator.
///
/// # Safety
///
/// - The caller must ensure that no allocations made within `f` escape the scope,
///   unless the allocator can handle out-of-scope deallocation.
/// - The allocator provided must be able to handle "foreign" pointers if recursion occurs
///   (i.e., if it allocates small internal structures using `std::alloc`, it must
///   recognize and reject them during deallocation, or ensure they are never passed to it).
///   Note that `BrandedSlab` is safe because its internal allocations (pages) are large enough
///   to be passed through to `System` by its own logic.
pub unsafe fn with_global_allocator<'brand, A, R, F>(
    allocator: &A,
    token: &GhostToken<'brand>,
    f: F
) -> R
where
    A: GhostAlloc<'brand>,
    F: FnOnce() -> R,
{
    let adapter = ScopedAdapter {
        allocator,
        token,
    };

    // Construct fat pointer to dyn GlobalAlloc
    let trait_object: &dyn GlobalAlloc = &adapter;

    // Transmute to 'static to store in TLS.
    // This is safe because we clear it before the function returns (and thus before 'a ends).
    let ptr = unsafe {
        core::mem::transmute::<NonNull<dyn GlobalAlloc>, NonNull<dyn GlobalAlloc>>(
             NonNull::from(trait_object)
        )
    };

    let prev = CURRENT_ALLOCATOR.get();
    CURRENT_ALLOCATOR.set(Some(ptr));

    // Ensure we restore the previous allocator even if f panics
    struct RestoreGuard(Option<NonNull<dyn GlobalAlloc>>);
    impl Drop for RestoreGuard {
        fn drop(&mut self) {
            CURRENT_ALLOCATOR.set(self.0);
        }
    }
    let _guard = RestoreGuard(prev);

    f()
}
