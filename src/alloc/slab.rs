//! `BrandedSlab` — a token-gated slab allocator.
//!
//! Implements a slab allocator where memory blocks are managed in pages.
//! Access is protected by `GhostToken`, ensuring exclusive access without locks,
//! or concurrent access via `GhostAlloc`.

use crate::{GhostToken, GhostCell};
use crate::token::traits::GhostBorrow;
use crate::alloc::{GhostAlloc, AllocError};
use crate::alloc::page::{PAGE_SIZE, align_up};
use crate::alloc::segregated::size_class::{SLAB_CLASS_COUNT, get_size_class_index, get_block_size};
use crate::concurrency::{CachePadded, SHARD_COUNT, SHARD_MASK, current_shard_index};
use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc};
use std::cell::UnsafeCell;
use std::sync::Mutex;

// Global Page Pool (Treiber Stack) for eager return.
// Stores pages that were freed eagerly.
static GLOBAL_PAGE_POOL: CachePadded<AtomicUsize> = CachePadded::new(AtomicUsize::new(0));

// Unique Thread ID generator for Page Ownership
static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Caches a unique 64-bit ID for the current thread.
    static LOCAL_THREAD_ID: u64 = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
}

#[inline(always)]
fn current_thread_id() -> u64 {
    LOCAL_THREAD_ID.with(|id| *id)
}

// Tag constants for ABA prevention in free list
const TAG_SHIFT: usize = 32;
const INDEX_MASK: usize = (1 << TAG_SHIFT) - 1;
const NONE: usize = INDEX_MASK;

// Page State Constants
const STATE_DETACHED: usize = 0;
const STATE_LOCKED: usize = 1;
const STATE_IN_LIST: usize = 2;
const STATE_FULL: usize = 3;

/// A memory page containing blocks of a specific size.
///
/// This struct is embedded at the START of the allocated 4KB page.
#[repr(C)]
pub(crate) struct Page {
    next: AtomicUsize, // Linked list of available pages (Stack link)
    all_next: AtomicUsize, // Linked list of all pages (for Drop/Compact)
    all_prev: AtomicUsize, // Previous page in all_heads list (Doubly linked)
    block_size: usize,
    remote_free_head: AtomicUsize, // Stack of blocks freed by other threads (Atomic)
    local_free_head: UnsafeCell<usize>, // Stack of blocks available for local allocation (Non-Atomic)
    capacity: usize,
    allocated_count: AtomicUsize,
    shard_index: usize,
    in_stack: AtomicUsize, // STATE_DETACHED, etc.
    owner_thread: AtomicU64, // Unique ID of the thread that owns `local_free_head`
    // Padding to ensure the header size is a multiple of cache line (128 bytes)
    // to prevent false sharing between the header (metadata) and the first block (user data).
    // Current size: 11*8 = 88 bytes (on 64-bit).
    // We need 40 bytes of padding.
    _padding: [u8; 40],
}

// Safety: `local_free_head` is only accessed by the thread that owns the Page (removed from `heads`).
// `remote_free_head` is Atomic.
unsafe impl Sync for Page {}

impl Page {
    /// Allocates a new 4KB page and initializes it as a Page with blocks of `block_size`.
    /// `next` (available stack) is initialized to 0 (detached).
    fn new(block_size: usize, shard_index: usize) -> Option<NonNull<Page>> {
        // Ensure alignment is 4KB so we can find the header via masking
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }

            match Self::init_from_ptr(ptr, block_size, shard_index) {
                Some(p) => Some(p),
                None => {
                    dealloc(ptr, layout);
                    None
                }
            }
        }
    }

    /// Initializes a page at the given pointer.
    ///
    /// # Safety
    /// `ptr` must be valid, at least PAGE_SIZE bytes, and aligned to PAGE_SIZE.
    pub(crate) unsafe fn init_from_ptr(
        ptr: *mut u8,
        block_size: usize,
        shard_index: usize
    ) -> Option<NonNull<Page>> {
        let page_ptr = ptr as *mut Page;

        // Calculate where blocks start
        let header_size = std::mem::size_of::<Page>();
        let start_offset = align_up(header_size, block_size);

        if start_offset >= PAGE_SIZE {
            return None;
        }

        let available_bytes = PAGE_SIZE - start_offset;
        let capacity = available_bytes / block_size;

        if capacity == 0 {
            return None;
        }

        // Initialize free list
        // Blocks are indexed 0..capacity.
        // We write the index of the next free block into the block memory itself (as u32).
        let base_ptr = ptr.add(start_offset);
        for i in 0..capacity - 1 {
            let block_ptr = base_ptr.add(i * block_size);
            *(block_ptr as *mut u32) = (i + 1) as u32;
        }
        let last_block_ptr = base_ptr.add((capacity - 1) * block_size);
        *(last_block_ptr as *mut u32) = NONE as u32;

        // Write the header
        // Note: local_free_head starts at 0. remote_free_head at NONE.
        std::ptr::write(page_ptr, Page {
            next: AtomicUsize::new(0), // Initially detached from available stack
            all_next: AtomicUsize::new(0),
            all_prev: AtomicUsize::new(0),
            block_size,
            remote_free_head: AtomicUsize::new(NONE),
            local_free_head: UnsafeCell::new(0),
            capacity,
            allocated_count: AtomicUsize::new(0),
            shard_index,
            in_stack: AtomicUsize::new(STATE_DETACHED), // Initially detached
            owner_thread: AtomicU64::new(current_thread_id()),
            _padding: [0; 40],
        });

        Some(NonNull::new_unchecked(page_ptr))
    }

    /// Allocates a block from the local free list.
    /// If empty, steals from the remote free list.
    /// Requires exclusive access to `local_free_head` (guaranteed by popping page from `heads`).
    unsafe fn alloc_local(&self) -> Option<NonNull<u8>> {
        let head_ptr = self.local_free_head.get();
        let mut idx = *head_ptr;

        if idx == NONE {
            // Steal remote
            let remote_val = self.remote_free_head.swap(NONE, Ordering::AcqRel);
            let (remote_idx, _) = Self::unpack(remote_val);
            if remote_idx == NONE {
                return None;
            }
            idx = remote_idx;
        }

        let block_ptr = self.get_block_ptr(idx);
        let next_idx = *(block_ptr as *const u32) as usize;
        *head_ptr = next_idx;

        Some(NonNull::new_unchecked(block_ptr))
    }

    /// Deallocates a block to the remote free list (Atomic).
    unsafe fn dealloc_remote(&self, ptr: NonNull<u8>) {
        let idx = self.get_block_index(ptr);
        let block_ptr = ptr.as_ptr();

        let mut current = self.remote_free_head.load(Ordering::Acquire);
        loop {
            let (curr_head_idx, tag) = Self::unpack(current);
            let new_tag = tag.wrapping_add(1);
            let new_head = Self::pack(idx, new_tag);

            // Link this block to current head
            *(block_ptr as *mut u32) = curr_head_idx as u32;

            match self.remote_free_head.compare_exchange_weak(
                current,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    /// Deallocates a block to the local free list (Exclusive).
    unsafe fn dealloc_local(&self, ptr: NonNull<u8>) {
        let idx = self.get_block_index(ptr);
        let block_ptr = ptr.as_ptr();
        let head_ptr = self.local_free_head.get();

        *(block_ptr as *mut u32) = *head_ptr as u32;
        *head_ptr = idx;
    }

    // --- Helpers ---

    unsafe fn get_block_ptr(&self, idx: usize) -> *mut u8 {
        let page_addr = self as *const Page as usize;
        let header_size = std::mem::size_of::<Page>();
        let start_offset = align_up(header_size, self.block_size);
        let block_offset = start_offset + idx * self.block_size;
        (page_addr + block_offset) as *mut u8
    }

    unsafe fn get_block_index(&self, ptr: NonNull<u8>) -> usize {
        let page_addr = self as *const Page as usize;
        let ptr_addr = ptr.as_ptr() as usize;
        let header_size = std::mem::size_of::<Page>();
        let start_offset = align_up(header_size, self.block_size);
        let offset = ptr_addr - page_addr - start_offset;
        offset / self.block_size
    }

    fn unpack(val: usize) -> (usize, usize) {
        (val & INDEX_MASK, val >> TAG_SHIFT)
    }

    fn pack(index: usize, tag: usize) -> usize {
        (tag << TAG_SHIFT) | (index & INDEX_MASK)
    }

    unsafe fn from_ptr(ptr: NonNull<u8>) -> NonNull<Page> {
        let addr = ptr.as_ptr() as usize;
        let page_addr = addr & !(PAGE_SIZE - 1);
        NonNull::new_unchecked(page_addr as *mut Page)
    }
}

/// Internal state of the slab allocator.
struct SlabState {
    // Array of available page lists (Treiber Stack), one for each size class.
    // Stored as AtomicUsize (pointers to Page).
    heads: [[CachePadded<AtomicUsize>; SHARD_COUNT]; SLAB_CLASS_COUNT],
    // Array of all page lists, one for each size class.
    // Used for memory reclamation on Drop.
    // Protected by Mutex to allow concurrent removal (for eager return).
    all_heads: [[CachePadded<Mutex<usize>>; SHARD_COUNT]; SLAB_CLASS_COUNT],
}

impl SlabState {
    fn new() -> Self {
        Self {
            heads: core::array::from_fn(|_| core::array::from_fn(|_| CachePadded::new(AtomicUsize::new(0)))),
            all_heads: core::array::from_fn(|_| core::array::from_fn(|_| CachePadded::new(Mutex::new(0)))),
        }
    }
}

impl Drop for SlabState {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
            for class_heads in &mut self.all_heads {
                for head_mutex in class_heads {
                    // We have exclusive access in Drop, so we can use get_mut
                    let mut page_ptr_val = *head_mutex.get_mut().unwrap();
                    while page_ptr_val != 0 {
                        let page = &mut *(page_ptr_val as *mut Page);
                        let next_val = *page.all_next.get_mut(); // Iterate all_next list

                        // Drop the page memory
                        dealloc(page_ptr_val as *mut u8, layout);
                        page_ptr_val = next_val;
                    }
                }
            }
        }
    }
}

// --- Stack / List Helpers ---

unsafe fn push_global(page_ptr: NonNull<Page>) {
    let page = page_ptr.as_ref();
    let mut current = GLOBAL_PAGE_POOL.load(Ordering::Acquire);
    loop {
        page.all_next.store(current, Ordering::Relaxed);
        match GLOBAL_PAGE_POOL.compare_exchange_weak(
            current,
            page_ptr.as_ptr() as usize,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

unsafe fn pop_global() -> Option<NonNull<Page>> {
    let mut current = GLOBAL_PAGE_POOL.load(Ordering::Acquire);
    loop {
        if current == 0 {
            return None;
        }
        let page = &*(current as *const Page);
        let next = page.all_next.load(Ordering::Relaxed);
        match GLOBAL_PAGE_POOL.compare_exchange_weak(
            current,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return NonNull::new(current as *mut Page),
            Err(actual) => current = actual,
        }
    }
}

unsafe fn push_page(head_atomic: &AtomicUsize, page_ptr: NonNull<Page>) {
    let page = page_ptr.as_ref();
    let mut current = head_atomic.load(Ordering::Acquire);
    loop {
        page.next.store(current, Ordering::Relaxed);
        match head_atomic.compare_exchange_weak(
            current,
            page_ptr.as_ptr() as usize,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

unsafe fn link_to_all_heads(head_mutex: &Mutex<usize>, page_ptr: NonNull<Page>) {
    let mut head_guard = head_mutex.lock().unwrap();
    link_to_all_heads_internal(&mut *head_guard, page_ptr);
}

unsafe fn link_to_all_heads_mut(head_mutex: &mut Mutex<usize>, page_ptr: NonNull<Page>) {
    let head_ptr = head_mutex.get_mut().unwrap();
    link_to_all_heads_internal(head_ptr, page_ptr);
}

unsafe fn link_to_all_heads_internal(head_ptr: &mut usize, page_ptr: NonNull<Page>) {
    let current_head_ptr = *head_ptr;
    let page = page_ptr.as_ref();

    // Link next
    page.all_next.store(current_head_ptr, Ordering::Relaxed);
    // Link prev (new head has no prev)
    page.all_prev.store(0, Ordering::Relaxed);

    if current_head_ptr != 0 {
        let current_head = &*(current_head_ptr as *const Page);
        current_head.all_prev.store(page_ptr.as_ptr() as usize, Ordering::Relaxed);
    }

    *head_ptr = page_ptr.as_ptr() as usize;
}

unsafe fn try_unlink_from_all_heads(head_mutex: &Mutex<usize>, page_ptr: NonNull<Page>) -> bool {
    if let Ok(mut head_guard) = head_mutex.try_lock() {
        unlink_from_all_heads_internal(&mut *head_guard, page_ptr);
        true
    } else {
        false
    }
}

unsafe fn unlink_from_all_heads(head_mutex: &Mutex<usize>, page_ptr: NonNull<Page>) {
    let mut head_guard = head_mutex.lock().unwrap();
    unlink_from_all_heads_internal(&mut *head_guard, page_ptr);
}

unsafe fn unlink_from_all_heads_mut(head_mutex: &mut Mutex<usize>, page_ptr: NonNull<Page>) {
    let head_ptr = head_mutex.get_mut().unwrap();
    unlink_from_all_heads_internal(head_ptr, page_ptr);
}

unsafe fn unlink_from_all_heads_internal(head_ptr: &mut usize, page_ptr: NonNull<Page>) {
    let page = page_ptr.as_ref();

    let prev_ptr = page.all_prev.load(Ordering::Relaxed);
    let next_ptr = page.all_next.load(Ordering::Relaxed);

    if prev_ptr != 0 {
        let prev_page = &*(prev_ptr as *const Page);
        prev_page.all_next.store(next_ptr, Ordering::Relaxed);
    } else {
        // We were head
        *head_ptr = next_ptr;
    }

    if next_ptr != 0 {
        let next_page = &*(next_ptr as *const Page);
        next_page.all_prev.store(prev_ptr, Ordering::Relaxed);
    }

    // Clear links
    page.all_next.store(0, Ordering::Relaxed);
    page.all_prev.store(0, Ordering::Relaxed);
}

/// A branded slab allocator.
///
/// # TODOs and Future Optimizations
///
/// - **TODO(perf): Thread-Local Caching (TLH)**:
///   Currently, `GhostAlloc` implementation suffers from contention on the `head` page of each size class.
///   Implementing a thread-local cache (via `ThreadLocal` or `SharedGhostToken` sharding) would
///   significantly improve throughput for high-concurrency workloads, similar to `mimalloc`'s free list sharding.
///
/// - **TODO(perf): Restartable Sequences (RSEQ)**:
///   On Linux, using RSEQ could allow faster per-cpu operations without atomics for the fast path.
pub struct BrandedSlab<'brand> {
    state: GhostCell<'brand, SlabState>,
}

impl<'brand> BrandedSlab<'brand> {
    /// Creates a new branded slab allocator.
    pub fn new() -> Self {
        Self {
            state: GhostCell::new(SlabState::new()),
        }
    }

    /// Injects a pre-initialized page into the slab allocator.
    ///
    /// # Safety
    /// The page must be valid, initialized via `Page::init_from_ptr`, and not currently in any list.
    pub unsafe fn inject_page(&self, token: &mut GhostToken<'brand>, page_ptr: NonNull<u8>) {
        let page = (page_ptr.as_ptr() as *mut Page).as_mut().unwrap();
        let size = page.block_size;

        let state = self.state.borrow_mut(token);

        if let Some(class_idx) = get_size_class_index(size) {
            let shard_idx = page.shard_index;
            let head_atomic = &mut state.heads[class_idx][shard_idx];
            let all_head_mutex = &mut state.all_heads[class_idx][shard_idx];

            // Link into all_heads
            link_to_all_heads(all_head_mutex, NonNull::new_unchecked(page_ptr.as_ptr() as *mut Page));

            // Link into heads (Available)
            *page.next.get_mut() = *head_atomic.get_mut();
            *head_atomic.get_mut() = page_ptr.as_ptr() as usize;
            *page.in_stack.get_mut() = STATE_IN_LIST;
        }
    }

    /// Manually triggers reclamation of empty pages.
    /// This is useful when mostly using concurrent allocation/deallocation, where pages are not automatically returned to OS.
    pub fn compact(&self, token: &mut GhostToken<'brand>) {
        let state = self.state.borrow_mut(token);
        unsafe {
            let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
            for class_idx in 0..SLAB_CLASS_COUNT {
                for shard_idx in 0..SHARD_COUNT {
                    let head_atomic = &mut state.heads[class_idx][shard_idx];
                    let mut current_ptr_val = *head_atomic.get_mut();
                    let mut prev_ptr_val = 0;

                    while current_ptr_val != 0 {
                        let page = &mut *(current_ptr_val as *mut Page);
                        let next_val = *page.next.get_mut();

                        if *page.allocated_count.get_mut() == 0 {
                            // Unlink from heads
                            if prev_ptr_val == 0 {
                                *head_atomic.get_mut() = next_val;
                            } else {
                                let prev_page = &mut *(prev_ptr_val as *mut Page);
                                *prev_page.next.get_mut() = next_val;
                            }

                            // Unlink from all_heads
                            let all_head_mutex = &mut state.all_heads[class_idx][shard_idx];
                            unlink_from_all_heads_mut(all_head_mutex, NonNull::new_unchecked(current_ptr_val as *mut Page));

                            // Free
                            dealloc(current_ptr_val as *mut u8, layout);

                            // Move to next
                            current_ptr_val = next_val;
                        } else {
                            prev_ptr_val = current_ptr_val;
                            current_ptr_val = next_val;
                        }
                    }
                }
            }
        }
    }

    /// Allocates memory with exclusive access.
    ///
    /// This method is faster than `GhostAlloc::allocate` as it avoids atomic operations.
    pub fn allocate_mut(
        &self,
        token: &mut GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());
        let state = self.state.borrow_mut(token);

        if let Some(class_idx) = get_size_class_index(size) {
            let block_size = get_block_size(class_idx);
            let shard_idx = current_shard_index();
            let head_atomic = &mut state.heads[class_idx][shard_idx];
            let all_heads_mutex = &mut state.all_heads[class_idx][shard_idx];

            // Iterate and drain full pages from the stack head
            loop {
                let page_ptr_val = *head_atomic.get_mut();
                if page_ptr_val != 0 {
                    unsafe {
                        let page = &mut *(page_ptr_val as *mut Page);
                        if let Some(ptr) = page.alloc_local() {
                            *page.allocated_count.get_mut() += 1;

                            // Check if it became full
                            if *page.allocated_count.get_mut() == page.capacity {
                                // Full. Remove from available stack.
                                let next_val = *page.next.get_mut();
                                *head_atomic.get_mut() = next_val;
                                *page.in_stack.get_mut() = STATE_DETACHED;
                            } else {
                                *page.in_stack.get_mut() = STATE_IN_LIST;
                            }
                            return Ok(ptr);
                        } else {
                            // Page was full. Remove and continue.
                            let next_val = *page.next.get_mut();
                            *head_atomic.get_mut() = next_val;
                            *page.in_stack.get_mut() = STATE_DETACHED;
                        }
                    }
                } else {
                    break;
                }
            }

            // Head full or empty, allocate new page
            // Try global pool first
            let mut new_page_opt = unsafe { pop_global() };
            if let Some(p) = new_page_opt {
                unsafe { Page::init_from_ptr(p.as_ptr() as *mut u8, block_size, shard_idx); }
                new_page_opt = Some(p);
            } else {
                new_page_opt = Page::new(block_size, shard_idx);
            }

            if let Some(mut new_page) = new_page_opt {
                unsafe {
                    link_to_all_heads_mut(all_heads_mutex, new_page);

                    let page = new_page.as_mut();
                    let ptr = page.alloc_local().ok_or(AllocError)?;
                    *page.allocated_count.get_mut() += 1;

                    // Push to heads (Available)
                    *page.next.get_mut() = *head_atomic.get_mut();
                    *head_atomic.get_mut() = new_page.as_ptr() as usize;
                    *page.in_stack.get_mut() = STATE_IN_LIST;

                    Ok(ptr)
                }
            } else {
                Err(AllocError)
            }
        } else {
            // Large allocation
            unsafe {
                let ptr = alloc(layout);
                NonNull::new(ptr).ok_or(AllocError)
            }
        }
    }

    /// Deallocates memory with exclusive access.
    pub unsafe fn deallocate_mut(
        &self,
        token: &mut GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        if let Some(class_idx) = get_size_class_index(size) {
            let mut page_ptr = Page::from_ptr(ptr);
            let page = page_ptr.as_mut();
            page.dealloc_local(ptr);
            *page.allocated_count.get_mut() -= 1;

            if *page.allocated_count.get_mut() == 0 {
                self.unlink_and_free(token, page_ptr, page.shard_index, class_idx);
            } else if *page.allocated_count.get_mut() == page.capacity - 1 {
                // Transition Full -> Available. Push to heads.
                let state = self.state.borrow_mut(token);
                let head_atomic = &mut state.heads[class_idx][page.shard_index];

                *page.next.get_mut() = *head_atomic.get_mut();
                *head_atomic.get_mut() = page_ptr.as_ptr() as usize;
                *page.in_stack.get_mut() = STATE_IN_LIST;
            }
        } else {
            dealloc(ptr.as_ptr(), layout);
        }
    }

    unsafe fn unlink_and_free(
        &self,
        token: &mut GhostToken<'brand>,
        page_ptr: NonNull<Page>,
        shard_idx: usize,
        class_idx: usize,
    ) {
        let state = self.state.borrow_mut(token);
        let target_ptr_val = page_ptr.as_ptr() as usize;

        // Unlink from heads (Available)
        // If it was empty, it should be in `heads` (Available).
        {
            let head_atomic = &mut state.heads[class_idx][shard_idx];
            let mut current_ptr_val = *head_atomic.get_mut();

            if current_ptr_val == target_ptr_val {
                 let page = &mut *(current_ptr_val as *mut Page);
                 *head_atomic.get_mut() = *page.next.get_mut();
            } else {
                while current_ptr_val != 0 {
                    let page = &mut *(current_ptr_val as *mut Page);
                    let next_val = *page.next.get_mut();
                    if next_val == target_ptr_val {
                        let target_page = &mut *(target_ptr_val as *mut Page);
                        *page.next.get_mut() = *target_page.next.get_mut();
                        break;
                    }
                    current_ptr_val = next_val;
                }
            }
        }

        // Unlink from all_heads (All)
        {
            let head_mutex = &mut state.all_heads[class_idx][shard_idx];
            unlink_from_all_heads(head_mutex, page_ptr);
        }

        // Free the page
        let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
        dealloc(page_ptr.as_ptr() as *mut u8, layout);
    }
}

impl<'brand> Default for BrandedSlab<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

// --- Concurrent Access Implementation ---
// This is now the default GhostAlloc implementation.
impl<'brand> GhostAlloc<'brand> for BrandedSlab<'brand> {
    fn allocate(
        &self,
        token: &impl GhostBorrow<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        self.allocate_in(token, layout, None)
    }

    fn allocate_in(
        &self,
        token: &impl GhostBorrow<'brand>,
        layout: Layout,
        shard_hint: Option<usize>,
    ) -> Result<NonNull<u8>, AllocError> {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());
        let state = self.state.borrow(token);

        if let Some(class_idx) = get_size_class_index(size) {
            let shard_idx = if let Some(hint) = shard_hint {
                hint & SHARD_MASK
            } else {
                current_shard_index()
            };
            let head_atomic = &state.heads[class_idx][shard_idx];

            let my_id = current_thread_id();

            // 1. Iterate and try to allocate
            let mut page_ptr_val = head_atomic.load(Ordering::Acquire);

            while page_ptr_val != 0 {
                unsafe {
                    let page = &*(page_ptr_val as *const Page);

                    // Check ownership
                    if page.owner_thread.load(Ordering::Relaxed) == my_id {
                        // Fast path: I own the page. No one else touches `local_free_head`.
                        if let Some(ptr) = page.alloc_local() {
                            let prev_allocated = page.allocated_count.fetch_add(1, Ordering::Relaxed);

                            if prev_allocated + 1 == page.capacity {
                                // Became Full. Try to Pop if at head.
                                // We don't need a lock, but we need to update in_stack logic?
                                // If we remove it, we mark it DETACHED (or effectively so).
                                // If dealloc sees DETACHED, it will add it back.

                                let current_head = head_atomic.load(Ordering::Relaxed);
                                if current_head == page_ptr_val {
                                     let next = page.next.load(Ordering::Relaxed);
                                     if head_atomic.compare_exchange(
                                         current_head, next, Ordering::Release, Ordering::Relaxed
                                     ).is_ok() {
                                         // Successfully removed.
                                         page.in_stack.store(STATE_DETACHED, Ordering::Release);
                                     } else {
                                         // Failed to remove. Full but in list.
                                         // We leave it as STATE_IN_LIST (implied).
                                     }
                                } else {
                                     // Not at head. Full but in list.
                                }
                            }
                            return Ok(ptr);
                        } else {
                            // Alloc failed (Full).
                            // Mark as FULL if we can?
                            // Only if we pop it. But alloc_local failed, so it IS full.
                            // We should probably skip it next time.
                            // But we leave it in the list.
                        }
                    }

                    page_ptr_val = page.next.load(Ordering::Acquire);
                }
            }

            // 2. Create new page
            let block_size = get_block_size(class_idx);
            let all_heads_mutex = &state.all_heads[class_idx][shard_idx];

            let mut new_page_opt = unsafe { pop_global() };
            if let Some(p) = new_page_opt {
                unsafe { Page::init_from_ptr(p.as_ptr() as *mut u8, block_size, shard_idx); }
                new_page_opt = Some(p);
            } else {
                new_page_opt = Page::new(block_size, shard_idx);
            }

            if let Some(mut new_page) = new_page_opt {
                unsafe {
                    link_to_all_heads(all_heads_mutex, new_page);

                    let page = new_page.as_mut();
                    let ptr = page.alloc_local().ok_or(AllocError)?;
                    page.allocated_count.fetch_add(1, Ordering::Relaxed);

                    // Push to available stack
                    page.in_stack.store(STATE_IN_LIST, Ordering::Relaxed);
                    push_page(head_atomic, new_page);

                    Ok(ptr)
                }
            } else {
                Err(AllocError)
            }
        } else {
            unsafe {
                let ptr = alloc(layout);
                NonNull::new(ptr).ok_or(AllocError)
            }
        }
    }

    unsafe fn deallocate(
        &self,
        token: &impl GhostBorrow<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        if let Some(class_idx) = get_size_class_index(size) {
            let page_ptr = Page::from_ptr(ptr);
            let page = page_ptr.as_ref();
            page.dealloc_remote(ptr);
            let prev = page.allocated_count.fetch_sub(1, Ordering::Relaxed);

            // Eager return: If allocated_count becomes 0
            if prev == 1 {
                let state = self.state.borrow(token);
                let head_atomic = &state.heads[class_idx][page.shard_index];
                let current_head = head_atomic.load(Ordering::Acquire);

                // Only detach if at head
                if current_head == page_ptr.as_ptr() as usize {
                    let next = page.next.load(Ordering::Relaxed);
                    if head_atomic.compare_exchange(current_head, next, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                        // We own the page now.
                        let all_heads_mutex = &state.all_heads[class_idx][page.shard_index];
                        if try_unlink_from_all_heads(all_heads_mutex, page_ptr) {
                            // Success: Global pool
                            push_global(page_ptr);
                            return;
                        } else {
                            // Failed to lock all_heads: Push back to heads (abort eager return)
                            push_page(head_atomic, page_ptr);
                        }
                    }
                }
            }

            // If transitioned from Full (capacity) to Available (capacity - 1)
            if prev == page.capacity {
                loop {
                    let current_status = page.in_stack.load(Ordering::Acquire);
                    match current_status {
                        STATE_DETACHED => {
                            if page.in_stack.compare_exchange(
                                STATE_DETACHED,
                                STATE_IN_LIST,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            ).is_ok() {
                                let state = self.state.borrow(token);
                                let head_atomic = &state.heads[class_idx][page.shard_index];
                                push_page(head_atomic, page_ptr);
                                break;
                            }
                        },
                        _ => {
                            break;
                        }
                    }
                }
            }
        } else {
            dealloc(ptr.as_ptr(), layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GhostToken, SharedGhostToken};
    use std::thread;

    #[test]
    fn test_branded_slab_basic() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();

            // Concurrent alloc with exclusive token reborrow
            let ptr1 = slab.allocate(&token, layout).unwrap();
            let ptr2 = slab.allocate(&token, layout).unwrap();

            unsafe {
                *(ptr1.as_ptr() as *mut u64) = 123;
                *(ptr2.as_ptr() as *mut u64) = 456;
                assert_eq!(*(ptr1.as_ptr() as *mut u64), 123);
                assert_eq!(*(ptr2.as_ptr() as *mut u64), 456);

                slab.deallocate(&token, ptr1, layout);
                slab.deallocate(&token, ptr2, layout);
            }

            // Exclusive alloc (optimized)
            let ptr3 = slab.allocate_mut(&mut token, layout).unwrap();
            unsafe {
                *(ptr3.as_ptr() as *mut u64) = 789;
                slab.deallocate_mut(&mut token, ptr3, layout);
            }
        });
    }

    #[test]
    fn test_concurrent_access() {
        GhostToken::new(|token| {
            let slab = BrandedSlab::new();
            let shared_token = SharedGhostToken::new(token);
            let slab_ref = &slab;
            let token_ref = &shared_token;

            thread::scope(|s| {
                for t_idx in 0..4 {
                    s.spawn(move || {
                        let guard = token_ref.read();
                        let layout = Layout::new::<u64>();
                        for i in 0..100 {
                            let ptr = slab_ref.allocate(&guard, layout).unwrap();
                            unsafe {
                                *(ptr.as_ptr() as *mut u64) = (t_idx * 1000 + i) as u64;
                                slab_ref.deallocate(&guard, ptr, layout);
                            }
                        }
                    });
                }
            });
        });
    }

    #[test]
    fn test_eager_page_return() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();

            // Allocate enough to fill a page and start a new one
            // Page size 4096. Block size 8 (min).
            // Page capacity ~ (4096 - 48 header - 80 padding) / 8 = 3968 / 8 = 496.

            let mut ptrs = Vec::new();
            for _ in 0..600 {
                ptrs.push(slab.allocate_mut(&mut token, layout).unwrap());
            }

            // Now free them all
            for ptr in ptrs {
                unsafe {
                    slab.deallocate_mut(&mut token, ptr, layout);
                }
            }

            // If eager return works, pages should be freed.
            // We can't easily assert on memory usage here, but we verify no crash/corruption.

            // Allocate again to see if it works
            let ptr = slab.allocate_mut(&mut token, layout).unwrap();
            unsafe { slab.deallocate_mut(&mut token, ptr, layout); }
        });
    }

    #[test]
    fn test_compact() {
        GhostToken::new(|token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();
            let shared_token = SharedGhostToken::new(token);
            let slab_ref = &slab;
            let token_ref = &shared_token;

            // Use concurrent allocation/deallocation to leave empty pages
            thread::scope(|s| {
                for _ in 0..4 {
                    s.spawn(move || {
                        let guard = token_ref.read();
                        let mut ptrs = Vec::new();
                        for _ in 0..100 {
                            ptrs.push(slab_ref.allocate(&guard, layout).unwrap());
                        }
                        for ptr in ptrs {
                            unsafe { slab_ref.deallocate(&guard, ptr, layout); }
                        }
                    });
                }
            });

            // Now we have empty pages (likely).
            // Reclaim them.
            // Need mut token.
            let mut guard = shared_token.write();
            let token = &mut *guard;
            slab.compact(token);

            // Verify we can still allocate
            let ptr = slab.allocate_mut(token, layout).unwrap();
            unsafe { slab.deallocate_mut(token, ptr, layout); }
        });
    }
}

/// Initializes a raw memory region as a slab page.
///
/// # Safety
/// `ptr` must be valid for PAGE_SIZE and aligned.
pub unsafe fn init_slab_page(
    ptr: NonNull<u8>,
    block_size: usize,
    shard_index: usize
) -> bool {
    Page::init_from_ptr(ptr.as_ptr(), block_size, shard_index).is_some()
}
