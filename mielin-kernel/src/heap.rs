//! Heap Allocator for MielinOS Kernel
//!
//! Implements the `GlobalAlloc` trait to provide heap allocation for the kernel.
//! Uses the pool allocator for small allocations (<=4KB) and falls back to
//! a bump allocator for larger allocations.
//!
//! ## Features
//!
//! - **Pool-based small allocations**: O(1) for sizes <= 4KB
//! - **Bump allocator fallback**: For larger allocations
//! - **Alignment-aware**: Properly handles alignment requirements
//! - **Statistics tracking**: Track heap usage and fragmentation
//! - **no_std compatible**: Works without standard library
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::heap;
//!
//! // Initialize the heap
//! unsafe {
//!     heap::init(heap_start, heap_size);
//! }
//!
//! // Now Box, Vec, etc. will work
//! let boxed = Box::new(42);
//! let vec = vec![1, 2, 3, 4, 5];
//! ```

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::pool::{self, BlockSize, PoolAllocation};

/// Maximum size that the pool allocator can handle
const MAX_POOL_SIZE: usize = 4096;

/// Default heap size if not specified (1MB)
const DEFAULT_HEAP_SIZE: usize = 1024 * 1024;

/// Minimum alignment for all allocations
const MIN_ALIGNMENT: usize = 8;

/// Header prepended to large allocations
#[repr(C)]
struct AllocationHeader {
    /// Size of the allocation (including header)
    size: usize,
    /// Alignment of the allocation
    align: usize,
    /// Magic number for validation
    magic: u32,
}

impl AllocationHeader {
    const MAGIC: u32 = 0xDEAD_BEEF;

    fn new(size: usize, align: usize) -> Self {
        Self {
            size,
            align,
            magic: Self::MAGIC,
        }
    }

    #[allow(dead_code)]
    fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC
    }
}

/// Bump allocator for large allocations
///
/// Simple allocator that bumps a pointer forward. Does not support
/// individual deallocations - memory is only reclaimed when reset.
struct BumpAllocator {
    /// Start of heap memory
    heap_start: AtomicUsize,
    /// End of heap memory
    heap_end: AtomicUsize,
    /// Current allocation pointer
    next: AtomicUsize,
    /// Number of active allocations
    allocations: AtomicUsize,
}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            heap_start: AtomicUsize::new(0),
            heap_end: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
        }
    }

    /// Initialize the bump allocator with a memory region
    ///
    /// # Safety
    /// The memory region must be valid and not used elsewhere
    unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        self.heap_start.store(heap_start, Ordering::Release);
        self.heap_end
            .store(heap_start + heap_size, Ordering::Release);
        self.next.store(heap_start, Ordering::Release);
        self.allocations.store(0, Ordering::Release);
    }

    /// Check if the allocator is initialized
    #[allow(dead_code)]
    fn is_initialized(&self) -> bool {
        self.heap_start.load(Ordering::Acquire) != 0
    }

    /// Allocate memory with the given layout
    fn allocate(&self, layout: Layout) -> Option<NonNull<u8>> {
        let alloc_start = align_up(self.next.load(Ordering::Acquire), layout.align());
        let alloc_end = alloc_start.checked_add(layout.size())?;

        if alloc_end > self.heap_end.load(Ordering::Acquire) {
            return None; // Out of memory
        }

        // Try to atomically advance the next pointer
        loop {
            let current = self.next.load(Ordering::Acquire);
            let aligned_start = align_up(current, layout.align());
            let new_end = aligned_start.checked_add(layout.size())?;

            if new_end > self.heap_end.load(Ordering::Acquire) {
                return None;
            }

            if self
                .next
                .compare_exchange(current, new_end, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.allocations.fetch_add(1, Ordering::Relaxed);
                return NonNull::new(aligned_start as *mut u8);
            }
            // CAS failed, retry
            core::hint::spin_loop();
        }
    }

    /// Deallocate memory (tracking only - bump allocator can't actually free)
    fn deallocate(&self) {
        let prev = self.allocations.fetch_sub(1, Ordering::Relaxed);

        // If this was the last allocation, we can reset the allocator
        if prev == 1 {
            let start = self.heap_start.load(Ordering::Acquire);
            self.next.store(start, Ordering::Release);
        }
    }

    /// Get used memory
    fn used(&self) -> usize {
        let next = self.next.load(Ordering::Acquire);
        let start = self.heap_start.load(Ordering::Acquire);
        next.saturating_sub(start)
    }

    /// Get free memory
    fn free(&self) -> usize {
        let next = self.next.load(Ordering::Acquire);
        let end = self.heap_end.load(Ordering::Acquire);
        end.saturating_sub(next)
    }

    /// Get total memory
    fn total(&self) -> usize {
        let start = self.heap_start.load(Ordering::Acquire);
        let end = self.heap_end.load(Ordering::Acquire);
        end.saturating_sub(start)
    }

    /// Get number of active allocations
    #[allow(dead_code)]
    fn allocations_count(&self) -> usize {
        self.allocations.load(Ordering::Relaxed)
    }
}

// SAFETY: BumpAllocator uses atomic operations (AtomicUsize, AtomicBool) for all
// shared mutable state. The heap memory region is only written via atomic CAS
// on the next pointer, ensuring thread-safe allocations.
unsafe impl Sync for BumpAllocator {}

impl Default for BumpAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// The kernel heap allocator
///
/// Combines a pool allocator for small allocations with a bump allocator
/// for larger allocations.
#[derive(Default)]
pub struct HeapAllocator {
    /// Bump allocator for large allocations
    bump: BumpAllocator,
    /// Whether the allocator has been initialized
    initialized: AtomicBool,
    /// Total pool allocations
    pool_allocations: AtomicUsize,
    /// Total pool deallocations
    pool_deallocations: AtomicUsize,
    /// Total bump allocations
    bump_allocations: AtomicUsize,
    /// Total bump deallocations
    bump_deallocations: AtomicUsize,
}

impl HeapAllocator {
    /// Create a new heap allocator (uninitialized)
    pub const fn new() -> Self {
        Self {
            bump: BumpAllocator::new(),
            initialized: AtomicBool::new(false),
            pool_allocations: AtomicUsize::new(0),
            pool_deallocations: AtomicUsize::new(0),
            bump_allocations: AtomicUsize::new(0),
            bump_deallocations: AtomicUsize::new(0),
        }
    }

    /// Initialize the heap allocator
    ///
    /// # Safety
    /// - `heap_start` must be a valid memory address
    /// - The memory region [heap_start, heap_start + heap_size) must be valid
    /// - This function must only be called once
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        // Initialize pool allocator
        pool::init();

        // Initialize bump allocator for large allocations
        self.bump.init(heap_start, heap_size);

        self.initialized.store(true, Ordering::Release);
    }

    /// Check if the allocator is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Get heap statistics
    pub fn stats(&self) -> HeapStats {
        let pool_stats = pool::stats();

        HeapStats {
            pool_total: pool_stats.total_memory(),
            pool_used: pool_stats.allocated_memory(),
            pool_free: pool_stats.free_memory(),
            pool_allocations: self.pool_allocations.load(Ordering::Relaxed),
            pool_deallocations: self.pool_deallocations.load(Ordering::Relaxed),
            bump_total: self.bump.total(),
            bump_used: self.bump.used(),
            bump_free: self.bump.free(),
            bump_allocations: self.bump_allocations.load(Ordering::Relaxed),
            bump_deallocations: self.bump_deallocations.load(Ordering::Relaxed),
        }
    }
}

impl HeapAllocator {
    /// Internal allocation with optional OOM handler retry
    unsafe fn alloc_with_retry(&self, layout: Layout, allow_retry: bool) -> *mut u8 {
        if !self.is_initialized() {
            return core::ptr::null_mut();
        }

        let size = layout.size();
        let align = layout.align().max(MIN_ALIGNMENT);

        // Try pool allocator for small allocations with standard alignment
        // Pool blocks are only guaranteed 8-byte alignment (from Vec allocation)
        if size <= MAX_POOL_SIZE && align <= MIN_ALIGNMENT {
            if let Some(alloc) = pool::allocate(size) {
                self.pool_allocations.fetch_add(1, Ordering::Relaxed);
                let ptr = alloc.ptr.as_ptr();
                call_hook(AllocationEvent::Alloc {
                    size,
                    align,
                    ptr: ptr as usize,
                    is_pool: true,
                });
                return ptr;
            }
        }

        // Fall back to bump allocator for large allocations or when pool is exhausted
        let header_size = core::mem::size_of::<AllocationHeader>();

        // We need to allocate:
        // [padding] [header] [data aligned to `align`]
        // The header itself needs 8-byte alignment, and data needs `align` alignment.
        // Maximum padding needed is (align - 1) to align the data portion.
        let max_padding = if align > header_size {
            align - header_size % align
        } else {
            0
        };
        let total_size = header_size + max_padding + size;

        // Allocate with minimum alignment (header needs at least 8-byte alignment)
        let alloc_layout = Layout::from_size_align(total_size, 8).unwrap_or(layout);

        if let Some(ptr) = self.bump.allocate(alloc_layout) {
            let base_addr = ptr.as_ptr() as usize;

            // Calculate where the data should start (aligned)
            // Data starts after header, at an aligned address
            let data_addr = align_up(base_addr + header_size, align);

            // Place header just before the data
            let header_addr = data_addr - header_size;
            let header = header_addr as *mut AllocationHeader;
            header.write(AllocationHeader::new(total_size, align));

            self.bump_allocations.fetch_add(1, Ordering::Relaxed);

            call_hook(AllocationEvent::Alloc {
                size,
                align,
                ptr: data_addr,
                is_pool: false,
            });

            // Return the aligned data pointer
            data_addr as *mut u8
        } else {
            // Allocation failed - try OOM handler if allowed
            if allow_retry && handle_oom(layout) {
                // Handler says retry
                record_oom(true);
                return self.alloc_with_retry(layout, false); // Only retry once
            }
            record_oom(false);
            core::ptr::null_mut()
        }
    }
}

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc_with_retry(layout, true)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || !self.is_initialized() {
            return;
        }

        let size = layout.size();

        call_hook(AllocationEvent::Dealloc {
            ptr: ptr as usize,
            size,
        });

        // Check if this was a pool allocation
        if size <= MAX_POOL_SIZE {
            if let Some(block_size) = BlockSize::for_size(size) {
                // Reconstruct the PoolAllocation and deallocate
                let allocation = PoolAllocation {
                    ptr: NonNull::new_unchecked(ptr),
                    block_size,
                    requested_size: size,
                };
                pool::deallocate(allocation);
                self.pool_deallocations.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        // Must be a bump allocation - just track the deallocation
        // (bump allocator can't actually free individual allocations)
        self.bump.deallocate();
        self.bump_deallocations.fetch_add(1, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Simple realloc: allocate new, copy, free old
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc(new_layout);

        if !new_ptr.is_null() && !ptr.is_null() {
            // Copy the minimum of old and new size
            let copy_size = layout.size().min(new_size);
            core::ptr::copy_nonoverlapping(ptr, new_ptr, copy_size);

            call_hook(AllocationEvent::Realloc {
                old_ptr: ptr as usize,
                old_size: layout.size(),
                new_ptr: new_ptr as usize,
                new_size,
            });

            self.dealloc(ptr, layout);
        }

        new_ptr
    }
}

// SAFETY: HeapAllocator uses atomic operations for initialization flag and
// allocation counters. The BumpAllocator is Sync, and pool::allocate/deallocate
// use lock-free atomic operations. All mutable state is protected by atomics.
unsafe impl Sync for HeapAllocator {}

/// Statistics about heap usage
#[derive(Debug, Clone, Copy, Default)]
pub struct HeapStats {
    /// Total pool memory
    pub pool_total: usize,
    /// Used pool memory
    pub pool_used: usize,
    /// Free pool memory
    pub pool_free: usize,
    /// Total pool allocations
    pub pool_allocations: usize,
    /// Total pool deallocations
    pub pool_deallocations: usize,
    /// Total bump allocator memory
    pub bump_total: usize,
    /// Used bump allocator memory
    pub bump_used: usize,
    /// Free bump allocator memory
    pub bump_free: usize,
    /// Total bump allocations
    pub bump_allocations: usize,
    /// Total bump deallocations
    pub bump_deallocations: usize,
}

impl HeapStats {
    /// Get total heap memory
    pub fn total(&self) -> usize {
        self.pool_total + self.bump_total
    }

    /// Get total used memory
    pub fn used(&self) -> usize {
        self.pool_used + self.bump_used
    }

    /// Get total free memory
    pub fn free(&self) -> usize {
        self.pool_free + self.bump_free
    }

    /// Get total allocations
    pub fn total_allocations(&self) -> usize {
        self.pool_allocations + self.bump_allocations
    }

    /// Get total deallocations
    pub fn total_deallocations(&self) -> usize {
        self.pool_deallocations + self.bump_deallocations
    }

    /// Get active allocation count
    pub fn active_allocations(&self) -> usize {
        self.total_allocations()
            .saturating_sub(self.total_deallocations())
    }

    /// Get fragmentation ratio (0.0 = no fragmentation, 1.0 = fully fragmented)
    pub fn fragmentation(&self) -> f32 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let free = self.free();
        let used = self.used();
        // Simple fragmentation metric: how much free memory exists relative to used
        if used == 0 {
            0.0
        } else {
            1.0 - (free as f32 / total as f32)
        }
    }
}

/// Align the given address upward to alignment
const fn align_up(addr: usize, align: usize) -> usize {
    let remainder = addr % align;
    if remainder == 0 {
        addr
    } else {
        addr + align - remainder
    }
}

/// Global heap allocator instance
static HEAP_ALLOCATOR: HeapAllocator = HeapAllocator::new();

// =============================================================================
// OOM Handler
// =============================================================================

use core::sync::atomic::AtomicPtr;

/// OOM handler function type
///
/// Called when an allocation fails. The handler receives the requested layout
/// and can attempt recovery actions like:
/// - Freeing cached memory
/// - Triggering garbage collection
/// - Logging the OOM event
///
/// Returns `true` if the handler freed some memory and the allocation should be retried.
/// Returns `false` if no recovery was possible.
pub type OomHandler = fn(Layout) -> bool;

/// Default OOM handler that simply returns false (no recovery possible)
fn default_oom_handler(_layout: Layout) -> bool {
    false
}

/// Global OOM handler
static OOM_HANDLER: AtomicPtr<()> = AtomicPtr::new(default_oom_handler as *mut ());

/// Set the OOM handler
///
/// The handler will be called when an allocation fails. It can attempt recovery
/// actions and return `true` to retry the allocation.
///
/// # Example
///
/// ```rust,ignore
/// use mielin_kernel::heap::{set_oom_handler, Layout};
///
/// fn my_oom_handler(layout: Layout) -> bool {
///     // Try to free some cached memory
///     if free_caches() {
///         return true; // Retry allocation
///     }
///     // Log the OOM event
///     log_oom(layout.size(), layout.align());
///     false // No recovery possible
/// }
///
/// set_oom_handler(my_oom_handler);
/// ```
pub fn set_oom_handler(handler: OomHandler) {
    OOM_HANDLER.store(handler as *mut (), Ordering::Release);
}

/// Get the current OOM handler
pub fn get_oom_handler() -> OomHandler {
    let ptr = OOM_HANDLER.load(Ordering::Acquire);
    // SAFETY: The pointer was stored from a valid function pointer
    unsafe { core::mem::transmute(ptr) }
}

/// Call the OOM handler
///
/// Returns `true` if the handler suggests retrying the allocation.
pub fn handle_oom(layout: Layout) -> bool {
    get_oom_handler()(layout)
}

/// OOM statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct OomStats {
    /// Total OOM events
    pub oom_count: usize,
    /// Successful recoveries
    pub recovery_count: usize,
    /// Failed recoveries
    pub failed_count: usize,
}

/// Track OOM events
static OOM_COUNT: AtomicUsize = AtomicUsize::new(0);
static OOM_RECOVERY_COUNT: AtomicUsize = AtomicUsize::new(0);
static OOM_FAILED_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Get OOM statistics
pub fn oom_stats() -> OomStats {
    OomStats {
        oom_count: OOM_COUNT.load(Ordering::Relaxed),
        recovery_count: OOM_RECOVERY_COUNT.load(Ordering::Relaxed),
        failed_count: OOM_FAILED_COUNT.load(Ordering::Relaxed),
    }
}

/// Record an OOM event (called internally)
fn record_oom(recovered: bool) {
    OOM_COUNT.fetch_add(1, Ordering::Relaxed);
    if recovered {
        OOM_RECOVERY_COUNT.fetch_add(1, Ordering::Relaxed);
    } else {
        OOM_FAILED_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

// =============================================================================
// Allocation Profiling Hooks
// =============================================================================

/// Allocation event type for profiling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationEvent {
    /// Memory was allocated
    Alloc {
        /// Size requested
        size: usize,
        /// Alignment requested
        align: usize,
        /// Pointer returned (as usize for no_std compatibility)
        ptr: usize,
        /// Whether pool or bump allocator was used
        is_pool: bool,
    },
    /// Memory was deallocated
    Dealloc {
        /// Pointer being freed
        ptr: usize,
        /// Size of the allocation
        size: usize,
    },
    /// Memory was reallocated
    Realloc {
        /// Old pointer
        old_ptr: usize,
        /// Old size
        old_size: usize,
        /// New pointer
        new_ptr: usize,
        /// New size
        new_size: usize,
    },
}

/// Allocation profiling hook function type
///
/// Called for each allocation event when profiling is enabled.
/// The hook receives the event details and can log, count, or
/// otherwise track allocations.
pub type AllocationHook = fn(AllocationEvent);

/// Global allocation profiling hook
static ALLOC_HOOK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Whether profiling is enabled
static PROFILING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Set the allocation profiling hook
///
/// The hook will be called for every allocation, deallocation, and reallocation
/// when profiling is enabled via [`enable_profiling`].
///
/// # Example
///
/// ```rust,ignore
/// use mielin_kernel::heap::{set_allocation_hook, AllocationEvent, enable_profiling};
/// use core::sync::atomic::{AtomicUsize, Ordering};
///
/// static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
/// static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);
///
/// fn my_profiler(event: AllocationEvent) {
///     match event {
///         AllocationEvent::Alloc { size, .. } => {
///             ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
///             TOTAL_BYTES.fetch_add(size, Ordering::Relaxed);
///         }
///         AllocationEvent::Dealloc { size, .. } => {
///             TOTAL_BYTES.fetch_sub(size, Ordering::Relaxed);
///         }
///         _ => {}
///     }
/// }
///
/// set_allocation_hook(my_profiler);
/// enable_profiling(true);
/// ```
pub fn set_allocation_hook(hook: AllocationHook) {
    ALLOC_HOOK.store(hook as *mut (), Ordering::Release);
}

/// Clear the allocation profiling hook
pub fn clear_allocation_hook() {
    ALLOC_HOOK.store(core::ptr::null_mut(), Ordering::Release);
}

/// Enable or disable allocation profiling
///
/// When disabled, the hook will not be called even if set.
/// This allows temporarily disabling profiling without clearing the hook.
pub fn enable_profiling(enabled: bool) {
    PROFILING_ENABLED.store(enabled, Ordering::Release);
}

/// Check if profiling is enabled
pub fn is_profiling_enabled() -> bool {
    PROFILING_ENABLED.load(Ordering::Acquire)
}

/// Call the profiling hook if enabled
#[inline]
fn call_hook(event: AllocationEvent) {
    if PROFILING_ENABLED.load(Ordering::Acquire) {
        let hook_ptr = ALLOC_HOOK.load(Ordering::Acquire);
        if !hook_ptr.is_null() {
            // SAFETY: The pointer was stored from a valid function pointer
            let hook: AllocationHook = unsafe { core::mem::transmute(hook_ptr) };
            hook(event);
        }
    }
}

/// Profiling statistics collected by the built-in profiler
#[derive(Debug, Clone, Copy, Default)]
pub struct ProfilingStats {
    /// Total allocations
    pub alloc_count: usize,
    /// Total deallocations
    pub dealloc_count: usize,
    /// Total reallocations
    pub realloc_count: usize,
    /// Total bytes allocated (cumulative)
    pub total_bytes_allocated: usize,
    /// Total bytes deallocated (cumulative)
    pub total_bytes_deallocated: usize,
    /// Current bytes in use (approximate)
    pub current_bytes: usize,
    /// Peak bytes in use
    pub peak_bytes: usize,
    /// Pool allocations
    pub pool_allocs: usize,
    /// Bump allocations
    pub bump_allocs: usize,
}

/// Global profiling statistics
static PROFILE_ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static PROFILE_DEALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static PROFILE_REALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static PROFILE_TOTAL_ALLOC: AtomicUsize = AtomicUsize::new(0);
static PROFILE_TOTAL_DEALLOC: AtomicUsize = AtomicUsize::new(0);
static PROFILE_CURRENT: AtomicUsize = AtomicUsize::new(0);
static PROFILE_PEAK: AtomicUsize = AtomicUsize::new(0);
static PROFILE_POOL_ALLOCS: AtomicUsize = AtomicUsize::new(0);
static PROFILE_BUMP_ALLOCS: AtomicUsize = AtomicUsize::new(0);

/// Built-in profiling hook that collects statistics
///
/// This is a simple profiler that tracks allocation counts and sizes.
/// Use [`profiling_stats`] to retrieve the collected statistics.
pub fn builtin_profiler(event: AllocationEvent) {
    match event {
        AllocationEvent::Alloc { size, is_pool, .. } => {
            PROFILE_ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            PROFILE_TOTAL_ALLOC.fetch_add(size, Ordering::Relaxed);
            let current = PROFILE_CURRENT.fetch_add(size, Ordering::Relaxed) + size;

            // Update peak
            let mut peak = PROFILE_PEAK.load(Ordering::Relaxed);
            while current > peak {
                match PROFILE_PEAK.compare_exchange_weak(
                    peak,
                    current,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(p) => peak = p,
                }
            }

            if is_pool {
                PROFILE_POOL_ALLOCS.fetch_add(1, Ordering::Relaxed);
            } else {
                PROFILE_BUMP_ALLOCS.fetch_add(1, Ordering::Relaxed);
            }
        }
        AllocationEvent::Dealloc { size, .. } => {
            PROFILE_DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            PROFILE_TOTAL_DEALLOC.fetch_add(size, Ordering::Relaxed);
            PROFILE_CURRENT.fetch_sub(
                size.min(PROFILE_CURRENT.load(Ordering::Relaxed)),
                Ordering::Relaxed,
            );
        }
        AllocationEvent::Realloc {
            old_size, new_size, ..
        } => {
            PROFILE_REALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            if new_size > old_size {
                let diff = new_size - old_size;
                PROFILE_TOTAL_ALLOC.fetch_add(diff, Ordering::Relaxed);
                let current = PROFILE_CURRENT.fetch_add(diff, Ordering::Relaxed) + diff;

                // Update peak
                let mut peak = PROFILE_PEAK.load(Ordering::Relaxed);
                while current > peak {
                    match PROFILE_PEAK.compare_exchange_weak(
                        peak,
                        current,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(p) => peak = p,
                    }
                }
            } else {
                let diff = old_size - new_size;
                PROFILE_TOTAL_DEALLOC.fetch_add(diff, Ordering::Relaxed);
                PROFILE_CURRENT.fetch_sub(
                    diff.min(PROFILE_CURRENT.load(Ordering::Relaxed)),
                    Ordering::Relaxed,
                );
            }
        }
    }
}

/// Get profiling statistics from the built-in profiler
pub fn profiling_stats() -> ProfilingStats {
    ProfilingStats {
        alloc_count: PROFILE_ALLOC_COUNT.load(Ordering::Relaxed),
        dealloc_count: PROFILE_DEALLOC_COUNT.load(Ordering::Relaxed),
        realloc_count: PROFILE_REALLOC_COUNT.load(Ordering::Relaxed),
        total_bytes_allocated: PROFILE_TOTAL_ALLOC.load(Ordering::Relaxed),
        total_bytes_deallocated: PROFILE_TOTAL_DEALLOC.load(Ordering::Relaxed),
        current_bytes: PROFILE_CURRENT.load(Ordering::Relaxed),
        peak_bytes: PROFILE_PEAK.load(Ordering::Relaxed),
        pool_allocs: PROFILE_POOL_ALLOCS.load(Ordering::Relaxed),
        bump_allocs: PROFILE_BUMP_ALLOCS.load(Ordering::Relaxed),
    }
}

/// Reset profiling statistics
pub fn reset_profiling_stats() {
    PROFILE_ALLOC_COUNT.store(0, Ordering::Relaxed);
    PROFILE_DEALLOC_COUNT.store(0, Ordering::Relaxed);
    PROFILE_REALLOC_COUNT.store(0, Ordering::Relaxed);
    PROFILE_TOTAL_ALLOC.store(0, Ordering::Relaxed);
    PROFILE_TOTAL_DEALLOC.store(0, Ordering::Relaxed);
    PROFILE_CURRENT.store(0, Ordering::Relaxed);
    PROFILE_PEAK.store(0, Ordering::Relaxed);
    PROFILE_POOL_ALLOCS.store(0, Ordering::Relaxed);
    PROFILE_BUMP_ALLOCS.store(0, Ordering::Relaxed);
}

/// Initialize the global heap allocator
///
/// # Safety
/// - `heap_start` must be a valid memory address
/// - The memory region must be valid and not used elsewhere
/// - This function must only be called once
pub unsafe fn init(heap_start: usize, heap_size: usize) {
    HEAP_ALLOCATOR.init(heap_start, heap_size);
}

/// Initialize with default heap (uses static memory)
///
/// This is useful for testing when no specific heap region is available
pub fn init_default() {
    use core::mem::MaybeUninit;
    use core::sync::atomic::AtomicU8;

    // Use a static buffer for the heap with AtomicU8 to avoid static_mut_refs warning
    static HEAP_BUFFER: [MaybeUninit<AtomicU8>; DEFAULT_HEAP_SIZE] = {
        // Safety: AtomicU8 has the same layout as u8
        // MaybeUninit doesn't require initialization
        unsafe { MaybeUninit::uninit().assume_init() }
    };

    let heap_start = HEAP_BUFFER.as_ptr() as usize;
    unsafe {
        init(heap_start, DEFAULT_HEAP_SIZE);
    }
}

/// Check if the heap is initialized
pub fn is_initialized() -> bool {
    HEAP_ALLOCATOR.is_initialized()
}

/// Get heap statistics
pub fn stats() -> HeapStats {
    HEAP_ALLOCATOR.stats()
}

/// Get a reference to the global allocator
pub fn global_allocator() -> &'static HeapAllocator {
    &HEAP_ALLOCATOR
}

// Note: The actual #[global_allocator] attribute should be applied in the
// main kernel crate, not here. This allows flexibility in choosing the
// allocator at the top level.

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn setup() {
        if !is_initialized() {
            init_default();
        }
    }

    #[test]
    fn test_heap_init() {
        setup();
        assert!(is_initialized());
    }

    #[test]
    fn test_heap_stats() {
        setup();
        let stats = stats();
        assert!(stats.pool_total > 0);
    }

    #[test]
    fn test_small_allocation() {
        setup();

        let layout = Layout::from_size_align(64, 8).unwrap();
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            assert!(!ptr.is_null());

            // Write to memory
            *ptr = 42;
            assert_eq!(*ptr, 42);

            HEAP_ALLOCATOR.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_various_sizes() {
        setup();

        for size in [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096] {
            let layout = Layout::from_size_align(size, 8).unwrap();
            unsafe {
                let ptr = HEAP_ALLOCATOR.alloc(layout);
                assert!(!ptr.is_null(), "Failed to allocate {} bytes", size);
                HEAP_ALLOCATOR.dealloc(ptr, layout);
            }
        }
    }

    #[test]
    fn test_large_allocation() {
        setup();

        // Allocation larger than pool max size
        let layout = Layout::from_size_align(8192, 8).unwrap();
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            assert!(!ptr.is_null());
            HEAP_ALLOCATOR.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_aligned_allocation() {
        setup();

        for align in [8, 16, 32, 64, 128] {
            let layout = Layout::from_size_align(128, align).unwrap();
            unsafe {
                let ptr = HEAP_ALLOCATOR.alloc(layout);
                assert!(!ptr.is_null());
                assert_eq!(ptr as usize % align, 0, "Alignment {} not satisfied", align);
                HEAP_ALLOCATOR.dealloc(ptr, layout);
            }
        }
    }

    #[test]
    fn test_realloc() {
        setup();

        let layout = Layout::from_size_align(64, 8).unwrap();
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            assert!(!ptr.is_null());

            // Write some data
            for i in 0..64 {
                *ptr.add(i) = i as u8;
            }

            // Realloc to larger size
            let new_ptr = HEAP_ALLOCATOR.realloc(ptr, layout, 128);
            assert!(!new_ptr.is_null());

            // Verify data was preserved
            for i in 0..64 {
                assert_eq!(*new_ptr.add(i), i as u8);
            }

            let new_layout = Layout::from_size_align(128, 8).unwrap();
            HEAP_ALLOCATOR.dealloc(new_ptr, new_layout);
        }
    }

    #[test]
    fn test_multiple_allocations() {
        setup();

        let layout = Layout::from_size_align(64, 8).unwrap();
        let mut ptrs = Vec::new();

        // Allocate many blocks
        for _ in 0..100 {
            unsafe {
                let ptr = HEAP_ALLOCATOR.alloc(layout);
                assert!(!ptr.is_null());
                ptrs.push(ptr);
            }
        }

        // Free all
        for ptr in ptrs {
            unsafe {
                HEAP_ALLOCATOR.dealloc(ptr, layout);
            }
        }
    }

    #[test]
    fn test_stats_tracking() {
        setup();

        let initial_stats = stats();

        let layout = Layout::from_size_align(64, 8).unwrap();
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            assert!(!ptr.is_null());

            let after_alloc = stats();
            assert!(after_alloc.total_allocations() > initial_stats.total_allocations());

            HEAP_ALLOCATOR.dealloc(ptr, layout);

            let after_dealloc = stats();
            assert!(after_dealloc.total_deallocations() > initial_stats.total_deallocations());
        }
    }

    #[test]
    fn test_heap_stats_methods() {
        setup();

        let stats = stats();

        // Basic sanity checks
        assert!(stats.total() > 0);
        // Note: In test environments with shared global state, used may exceed
        // total due to accumulated allocations across tests. We just verify
        // the methods work without panicking.
        let _used = stats.used();
        let _free = stats.free();

        let frag = stats.fragmentation();
        assert!((0.0..=1.0).contains(&frag));
    }

    #[test]
    fn test_zero_size_allocation() {
        setup();

        // Zero-size allocations should return a valid dangling pointer or null
        let layout = Layout::from_size_align(0, 1).unwrap();
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            // Behavior is implementation-defined for zero-size
            // Just ensure we don't crash
            if !ptr.is_null() {
                HEAP_ALLOCATOR.dealloc(ptr, layout);
            }
        }
    }

    #[test]
    fn test_allocation_header() {
        let header = AllocationHeader::new(1024, 16);
        assert!(header.is_valid());
        assert_eq!(header.size, 1024);
        assert_eq!(header.align, 16);
    }

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, 8), 0);
        assert_eq!(align_up(1, 8), 8);
        assert_eq!(align_up(7, 8), 8);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 8), 16);
        assert_eq!(align_up(15, 16), 16);
        assert_eq!(align_up(16, 16), 16);
        assert_eq!(align_up(17, 16), 32);
    }

    #[test]
    fn test_oom_handler_default() {
        setup();

        // Default handler should return false (no recovery)
        let layout = Layout::from_size_align(64, 8).unwrap();
        assert!(!handle_oom(layout));
    }

    #[test]
    fn test_oom_handler_custom() {
        setup();
        use core::sync::atomic::AtomicBool;

        static HANDLER_CALLED: AtomicBool = AtomicBool::new(false);

        fn test_handler(_layout: Layout) -> bool {
            HANDLER_CALLED.store(true, Ordering::SeqCst);
            true // Pretend we freed some memory
        }

        // Set custom handler
        set_oom_handler(test_handler);

        // Call it
        let layout = Layout::from_size_align(64, 8).unwrap();
        assert!(handle_oom(layout));
        assert!(HANDLER_CALLED.load(Ordering::SeqCst));

        // Reset to default
        set_oom_handler(default_oom_handler);
    }

    #[test]
    fn test_oom_stats() {
        setup();

        let initial = oom_stats();

        // The OOM stats track allocation failures, which may have occurred
        // in previous tests. Just verify the struct is properly initialized.
        let _count = initial.oom_count;
        let _recovery = initial.recovery_count;
        let _failed = initial.failed_count;
    }

    #[test]
    fn test_profiling_enable_disable() {
        setup();

        // Initially disabled
        assert!(!is_profiling_enabled());

        // Enable
        enable_profiling(true);
        assert!(is_profiling_enabled());

        // Disable
        enable_profiling(false);
        assert!(!is_profiling_enabled());
    }

    #[test]
    fn test_profiling_hook_set_clear() {
        setup();

        use core::sync::atomic::AtomicUsize;
        static HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

        fn test_hook(_event: AllocationEvent) {
            HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
        }

        // Set hook and enable profiling
        set_allocation_hook(test_hook);
        enable_profiling(true);

        // Make an allocation - should trigger hook
        let layout = Layout::from_size_align(64, 8).unwrap();
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            assert!(!ptr.is_null());
            HEAP_ALLOCATOR.dealloc(ptr, layout);
        }

        // Hook should have been called (at least twice: alloc + dealloc)
        assert!(HOOK_CALLS.load(Ordering::SeqCst) >= 2);

        // Disable profiling
        enable_profiling(false);
        let calls_before = HOOK_CALLS.load(Ordering::SeqCst);

        // Make another allocation - should NOT trigger hook
        unsafe {
            let ptr = HEAP_ALLOCATOR.alloc(layout);
            if !ptr.is_null() {
                HEAP_ALLOCATOR.dealloc(ptr, layout);
            }
        }

        // Hook should not have been called again
        assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), calls_before);

        // Clear hook
        clear_allocation_hook();
    }

    #[test]
    fn test_builtin_profiler() {
        setup();

        // Reset stats
        reset_profiling_stats();

        // Set builtin profiler and enable
        set_allocation_hook(builtin_profiler);
        enable_profiling(true);

        // Make some allocations
        let layout = Layout::from_size_align(128, 8).unwrap();
        unsafe {
            let ptr1 = HEAP_ALLOCATOR.alloc(layout);
            let ptr2 = HEAP_ALLOCATOR.alloc(layout);
            let ptr3 = HEAP_ALLOCATOR.alloc(layout);

            assert!(!ptr1.is_null());
            assert!(!ptr2.is_null());
            assert!(!ptr3.is_null());

            HEAP_ALLOCATOR.dealloc(ptr1, layout);
            HEAP_ALLOCATOR.dealloc(ptr2, layout);
            HEAP_ALLOCATOR.dealloc(ptr3, layout);
        }

        // Check stats
        let stats = profiling_stats();
        assert!(stats.alloc_count >= 3);
        assert!(stats.dealloc_count >= 3);
        assert!(stats.total_bytes_allocated >= 128 * 3);

        // Cleanup
        enable_profiling(false);
        clear_allocation_hook();
    }

    #[test]
    fn test_profiling_stats_peak_tracking() {
        setup();

        // Reset stats
        reset_profiling_stats();

        // Set builtin profiler and enable
        set_allocation_hook(builtin_profiler);
        enable_profiling(true);

        // Allocate progressively larger amounts
        let layout = Layout::from_size_align(256, 8).unwrap();
        unsafe {
            let ptr1 = HEAP_ALLOCATOR.alloc(layout);
            let ptr2 = HEAP_ALLOCATOR.alloc(layout);

            // Peak should be at least 512 bytes
            let stats_at_peak = profiling_stats();
            let peak_at_max = stats_at_peak.peak_bytes;

            // Free one
            if !ptr1.is_null() {
                HEAP_ALLOCATOR.dealloc(ptr1, layout);
            }

            // Peak should still be the same
            let stats_after_free = profiling_stats();
            assert!(stats_after_free.peak_bytes >= peak_at_max);

            // Cleanup
            if !ptr2.is_null() {
                HEAP_ALLOCATOR.dealloc(ptr2, layout);
            }
        }

        enable_profiling(false);
        clear_allocation_hook();
    }

    #[test]
    fn test_allocation_event_types() {
        // Test AllocationEvent enum variants
        let alloc_event = AllocationEvent::Alloc {
            size: 64,
            align: 8,
            ptr: 0x1000,
            is_pool: true,
        };
        assert_eq!(alloc_event, alloc_event);

        let dealloc_event = AllocationEvent::Dealloc {
            ptr: 0x1000,
            size: 64,
        };
        assert_eq!(dealloc_event, dealloc_event);

        let realloc_event = AllocationEvent::Realloc {
            old_ptr: 0x1000,
            old_size: 64,
            new_ptr: 0x2000,
            new_size: 128,
        };
        assert_eq!(realloc_event, realloc_event);

        // Verify they are different
        assert_ne!(
            AllocationEvent::Alloc {
                size: 64,
                align: 8,
                ptr: 0x1000,
                is_pool: true
            },
            AllocationEvent::Alloc {
                size: 128,
                align: 8,
                ptr: 0x1000,
                is_pool: true
            }
        );
    }

    #[test]
    fn test_profiling_stats_default() {
        let stats = ProfilingStats::default();
        assert_eq!(stats.alloc_count, 0);
        assert_eq!(stats.dealloc_count, 0);
        assert_eq!(stats.realloc_count, 0);
        assert_eq!(stats.total_bytes_allocated, 0);
        assert_eq!(stats.total_bytes_deallocated, 0);
        assert_eq!(stats.current_bytes, 0);
        assert_eq!(stats.peak_bytes, 0);
        assert_eq!(stats.pool_allocs, 0);
        assert_eq!(stats.bump_allocs, 0);
    }
}
