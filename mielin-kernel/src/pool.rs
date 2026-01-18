//! Memory Pool Allocator
//!
//! Provides O(1) allocation and deallocation for fixed-size blocks.
//! Designed for no_std environments and suitable for tensor buffers.
//!
//! ## Features
//!
//! - Fixed-size block pools (32B, 64B, 128B, 256B, 512B, 1KB, 4KB)
//! - O(1) allocation via free list
//! - O(1) deallocation
//! - Lock-free per-CPU pool design (future)
//! - Integration with page allocator for backing storage

extern crate alloc;

use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};

/// Block size classes supported by the pool allocator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockSize {
    B32 = 0,
    B64 = 1,
    B128 = 2,
    B256 = 3,
    B512 = 4,
    K1 = 5,
    K4 = 6,
}

impl BlockSize {
    /// Get the actual size in bytes for this block class
    pub const fn bytes(self) -> usize {
        match self {
            BlockSize::B32 => 32,
            BlockSize::B64 => 64,
            BlockSize::B128 => 128,
            BlockSize::B256 => 256,
            BlockSize::B512 => 512,
            BlockSize::K1 => 1024,
            BlockSize::K4 => 4096,
        }
    }

    /// Find the smallest block size that can hold `size` bytes
    pub const fn for_size(size: usize) -> Option<BlockSize> {
        if size == 0 {
            None
        } else if size <= 32 {
            Some(BlockSize::B32)
        } else if size <= 64 {
            Some(BlockSize::B64)
        } else if size <= 128 {
            Some(BlockSize::B128)
        } else if size <= 256 {
            Some(BlockSize::B256)
        } else if size <= 512 {
            Some(BlockSize::B512)
        } else if size <= 1024 {
            Some(BlockSize::K1)
        } else if size <= 4096 {
            Some(BlockSize::K4)
        } else {
            None // Too large for pool allocator
        }
    }

    /// Get all block sizes as an array
    pub const fn all() -> [BlockSize; 7] {
        [
            BlockSize::B32,
            BlockSize::B64,
            BlockSize::B128,
            BlockSize::B256,
            BlockSize::B512,
            BlockSize::K1,
            BlockSize::K4,
        ]
    }

    /// Get the block size from an index
    pub const fn from_index(index: usize) -> Option<BlockSize> {
        match index {
            0 => Some(BlockSize::B32),
            1 => Some(BlockSize::B64),
            2 => Some(BlockSize::B128),
            3 => Some(BlockSize::B256),
            4 => Some(BlockSize::B512),
            5 => Some(BlockSize::K1),
            6 => Some(BlockSize::K4),
            _ => None,
        }
    }

    /// Get the index for this block size
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A free block in the pool
/// Each free block contains a pointer to the next free block
#[repr(C)]
struct FreeBlock {
    next: *mut FreeBlock,
}

/// A single pool for a specific block size
///
/// Uses a lock-free free list for O(1) allocation/deallocation
pub struct Pool {
    /// Head of the free list (lock-free via atomic operations)
    free_list: AtomicPtr<FreeBlock>,
    /// Block size for this pool
    block_size: usize,
    /// Total number of blocks in this pool
    total_blocks: AtomicU32,
    /// Number of allocated (in-use) blocks
    allocated_blocks: AtomicU32,
    /// Backing memory (heap-allocated arena)
    arena: UnsafeCell<Option<Box<[u8]>>>,
    /// Arena initialized flag
    initialized: AtomicU32,
}

impl Pool {
    /// Arena size per pool (64KB)
    const ARENA_SIZE: usize = 64 * 1024;

    /// Create a new uninitialized pool
    pub const fn new(block_size: usize) -> Self {
        Self {
            free_list: AtomicPtr::new(core::ptr::null_mut()),
            block_size,
            total_blocks: AtomicU32::new(0),
            allocated_blocks: AtomicU32::new(0),
            arena: UnsafeCell::new(None),
            initialized: AtomicU32::new(0),
        }
    }

    /// Initialize the pool by creating free blocks in the arena
    ///
    /// # Safety
    /// Must only be called once. Uses atomic CAS to ensure single initialization.
    pub fn init(&self) {
        // Atomic check-and-set to prevent double initialization
        if self
            .initialized
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return; // Already initialized
        }

        // Allocate arena on heap
        let arena = alloc::vec![0u8; Self::ARENA_SIZE].into_boxed_slice();
        let arena_ptr = arena.as_ptr() as *mut u8;

        // Store arena
        // SAFETY: We hold exclusive access during initialization (checked above via CAS).
        // The arena is allocated and valid.
        unsafe {
            *self.arena.get() = Some(arena);
        }

        let num_blocks = Self::ARENA_SIZE / self.block_size;
        self.total_blocks.store(num_blocks as u32, Ordering::SeqCst);

        // Build the free list
        for i in (0..num_blocks).rev() {
            // SAFETY: arena_ptr is valid and block_ptr points within the arena bounds.
            // We're iterating in reverse to build a forward-linked free list.
            let block_ptr = unsafe { arena_ptr.add(i * self.block_size) } as *mut FreeBlock;
            // SAFETY: block_ptr is valid and aligned (block_size >= size_of::<FreeBlock>).
            unsafe {
                (*block_ptr).next = self.free_list.load(Ordering::Relaxed);
            }
            self.free_list.store(block_ptr, Ordering::Release);
        }
    }

    /// Maximum retries for lock-free operations
    const MAX_RETRIES: usize = 1000;

    /// Allocate a block from this pool
    ///
    /// Returns None if the pool is exhausted or under extreme contention
    ///
    /// # Thread Safety
    /// Uses lock-free atomic operations for thread-safe allocation
    pub fn allocate(&self) -> Option<NonNull<u8>> {
        for _ in 0..Self::MAX_RETRIES {
            let head = self.free_list.load(Ordering::Acquire);
            if head.is_null() {
                return None; // Pool exhausted
            }

            // SAFETY: head was checked non-null above and points to a FreeBlock
            // in the arena. It's in the free list, so no other thread owns it.
            let next = unsafe { (*head).next };

            // Try to atomically update the head
            if self
                .free_list
                .compare_exchange_weak(head, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.allocated_blocks.fetch_add(1, Ordering::Relaxed);
                return NonNull::new(head as *mut u8);
            }
            // CAS failed, retry with backoff
            core::hint::spin_loop();
        }
        None // Too much contention
    }

    /// Deallocate a block back to this pool
    ///
    /// # Safety
    /// The pointer must have been allocated from this pool
    /// The pointer must not be used after deallocation
    pub unsafe fn deallocate(&self, ptr: NonNull<u8>) {
        let block = ptr.as_ptr() as *mut FreeBlock;

        for _ in 0..Self::MAX_RETRIES {
            let head = self.free_list.load(Ordering::Acquire);
            (*block).next = head;

            // Try to atomically push to the free list
            // CAS: if free_list == head, set free_list = block
            if self
                .free_list
                .compare_exchange_weak(head, block, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.allocated_blocks.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            // CAS failed, retry with backoff
            core::hint::spin_loop();
        }
        // If we fail too many times, just leak the block rather than hang
    }

    /// Get the block size for this pool
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Get the total number of blocks in this pool
    pub fn total_blocks(&self) -> u32 {
        self.total_blocks.load(Ordering::Relaxed)
    }

    /// Get the number of allocated blocks
    pub fn allocated_blocks(&self) -> u32 {
        self.allocated_blocks.load(Ordering::Relaxed)
    }

    /// Get the number of free blocks available
    pub fn free_blocks(&self) -> u32 {
        self.total_blocks().saturating_sub(self.allocated_blocks())
    }

    /// Check if this pool is exhausted
    pub fn is_exhausted(&self) -> bool {
        self.free_list.load(Ordering::Acquire).is_null()
    }
}

// SAFETY: Pool uses atomic operations (AtomicPtr, AtomicBool, AtomicU32) for all
// shared mutable state. The free_list uses lock-free CAS operations. The arena
// is only written during initialization when we have exclusive access.
unsafe impl Sync for Pool {}

/// Multi-size pool allocator
///
/// Manages multiple pools for different block sizes
pub struct PoolAllocator {
    pools: [Pool; 7],
    total_allocations: AtomicUsize,
    total_deallocations: AtomicUsize,
}

impl PoolAllocator {
    /// Create a new pool allocator
    pub const fn new() -> Self {
        Self {
            pools: [
                Pool::new(32),
                Pool::new(64),
                Pool::new(128),
                Pool::new(256),
                Pool::new(512),
                Pool::new(1024),
                Pool::new(4096),
            ],
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
        }
    }

    /// Initialize all pools
    pub fn init(&self) {
        for pool in &self.pools {
            pool.init();
        }
    }

    /// Allocate memory of the given size
    ///
    /// Returns None if size is too large or pool is exhausted
    pub fn allocate(&self, size: usize) -> Option<PoolAllocation> {
        let block_size = BlockSize::for_size(size)?;
        let pool = &self.pools[block_size.index()];
        let ptr = pool.allocate()?;

        self.total_allocations.fetch_add(1, Ordering::Relaxed);

        Some(PoolAllocation {
            ptr,
            block_size,
            requested_size: size,
        })
    }

    /// Deallocate a previous allocation
    ///
    /// # Safety
    /// The allocation must have been obtained from this allocator
    pub unsafe fn deallocate(&self, allocation: PoolAllocation) {
        let pool = &self.pools[allocation.block_size.index()];
        pool.deallocate(allocation.ptr);
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the pool for a specific block size
    pub fn pool(&self, block_size: BlockSize) -> &Pool {
        &self.pools[block_size.index()]
    }

    /// Get statistics about the allocator
    pub fn stats(&self) -> PoolStats {
        let mut pool_stats = [PoolSizeStats::default(); 7];

        for (i, pool) in self.pools.iter().enumerate() {
            pool_stats[i] = PoolSizeStats {
                block_size: pool.block_size(),
                total_blocks: pool.total_blocks(),
                allocated_blocks: pool.allocated_blocks(),
                free_blocks: pool.free_blocks(),
            };
        }

        PoolStats {
            pools: pool_stats,
            total_allocations: self.total_allocations.load(Ordering::Relaxed),
            total_deallocations: self.total_deallocations.load(Ordering::Relaxed),
        }
    }
}

impl Default for PoolAllocator {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: PoolAllocator uses atomic operations for allocation counts and
// delegates to Pool which is also Sync. Each Pool is independently thread-safe.
unsafe impl Sync for PoolAllocator {}

/// Represents an allocation from the pool
#[derive(Debug)]
pub struct PoolAllocation {
    /// Pointer to the allocated memory
    pub ptr: NonNull<u8>,
    /// The block size class used
    pub block_size: BlockSize,
    /// The originally requested size
    pub requested_size: usize,
}

impl PoolAllocation {
    /// Get the actual allocated size (may be larger than requested)
    pub fn actual_size(&self) -> usize {
        self.block_size.bytes()
    }

    /// Get a slice of the allocated memory (up to requested_size)
    ///
    /// # Safety
    /// The allocation must still be valid (not deallocated)
    pub unsafe fn as_slice(&self) -> &[u8] {
        core::slice::from_raw_parts(self.ptr.as_ptr(), self.requested_size)
    }

    /// Get a mutable slice of the allocated memory (up to requested_size)
    ///
    /// # Safety
    /// The allocation must still be valid (not deallocated)
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.requested_size)
    }
}

/// Statistics for a single pool size
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolSizeStats {
    pub block_size: usize,
    pub total_blocks: u32,
    pub allocated_blocks: u32,
    pub free_blocks: u32,
}

/// Overall allocator statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub pools: [PoolSizeStats; 7],
    pub total_allocations: usize,
    pub total_deallocations: usize,
}

impl PoolStats {
    /// Get total memory available across all pools
    pub fn total_memory(&self) -> usize {
        self.pools
            .iter()
            .map(|p| p.block_size * p.total_blocks as usize)
            .sum()
    }

    /// Get total allocated memory across all pools
    pub fn allocated_memory(&self) -> usize {
        self.pools
            .iter()
            .map(|p| p.block_size * p.allocated_blocks as usize)
            .sum()
    }

    /// Get total free memory across all pools
    pub fn free_memory(&self) -> usize {
        self.pools
            .iter()
            .map(|p| p.block_size * p.free_blocks as usize)
            .sum()
    }
}

/// Global pool allocator instance
static GLOBAL_POOL: PoolAllocator = PoolAllocator::new();

/// Initialize the global pool allocator
pub fn init() {
    GLOBAL_POOL.init();
}

/// Allocate from the global pool
pub fn allocate(size: usize) -> Option<PoolAllocation> {
    GLOBAL_POOL.allocate(size)
}

/// Deallocate to the global pool
///
/// # Safety
/// The allocation must have been obtained from the global pool
pub unsafe fn deallocate(allocation: PoolAllocation) {
    GLOBAL_POOL.deallocate(allocation);
}

/// Get statistics from the global pool
pub fn stats() -> PoolStats {
    GLOBAL_POOL.stats()
}

/// Get the global pool allocator reference
pub fn global_pool() -> &'static PoolAllocator {
    &GLOBAL_POOL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_size_bytes() {
        assert_eq!(BlockSize::B32.bytes(), 32);
        assert_eq!(BlockSize::B64.bytes(), 64);
        assert_eq!(BlockSize::B128.bytes(), 128);
        assert_eq!(BlockSize::B256.bytes(), 256);
        assert_eq!(BlockSize::B512.bytes(), 512);
        assert_eq!(BlockSize::K1.bytes(), 1024);
        assert_eq!(BlockSize::K4.bytes(), 4096);
    }

    #[test]
    fn test_block_size_for_size() {
        assert_eq!(BlockSize::for_size(0), None);
        assert_eq!(BlockSize::for_size(1), Some(BlockSize::B32));
        assert_eq!(BlockSize::for_size(32), Some(BlockSize::B32));
        assert_eq!(BlockSize::for_size(33), Some(BlockSize::B64));
        assert_eq!(BlockSize::for_size(64), Some(BlockSize::B64));
        assert_eq!(BlockSize::for_size(65), Some(BlockSize::B128));
        assert_eq!(BlockSize::for_size(4096), Some(BlockSize::K4));
        assert_eq!(BlockSize::for_size(4097), None);
    }

    #[test]
    fn test_pool_basic_allocation() {
        let pool = Pool::new(64);
        pool.init();

        assert!(pool.total_blocks() > 0);
        assert_eq!(pool.allocated_blocks(), 0);

        let alloc1 = pool.allocate();
        assert!(alloc1.is_some());
        assert_eq!(pool.allocated_blocks(), 1);

        let alloc2 = pool.allocate();
        assert!(alloc2.is_some());
        assert_eq!(pool.allocated_blocks(), 2);

        // Deallocate
        unsafe {
            pool.deallocate(alloc1.unwrap());
        }
        assert_eq!(pool.allocated_blocks(), 1);

        unsafe {
            pool.deallocate(alloc2.unwrap());
        }
        assert_eq!(pool.allocated_blocks(), 0);
    }

    #[test]
    fn test_pool_many_allocations() {
        let pool = Pool::new(128);
        pool.init();

        let total = pool.total_blocks();
        let mut allocs = alloc::vec::Vec::new();

        // Allocate all blocks
        for _ in 0..total {
            let alloc = pool.allocate();
            assert!(alloc.is_some());
            allocs.push(alloc.unwrap());
        }

        // Pool should be exhausted
        assert!(pool.is_exhausted());
        assert!(pool.allocate().is_none());

        // Deallocate all
        for alloc in allocs {
            unsafe {
                pool.deallocate(alloc);
            }
        }

        assert!(!pool.is_exhausted());
        assert_eq!(pool.allocated_blocks(), 0);
    }

    #[test]
    fn test_pool_allocator_basic() {
        let allocator = PoolAllocator::new();
        allocator.init();

        let alloc = allocator.allocate(50);
        assert!(alloc.is_some());
        let alloc = alloc.unwrap();
        assert_eq!(alloc.block_size, BlockSize::B64);
        assert_eq!(alloc.actual_size(), 64);
        assert_eq!(alloc.requested_size, 50);

        unsafe {
            allocator.deallocate(alloc);
        }
    }

    #[test]
    fn test_pool_allocator_different_sizes() {
        let allocator = PoolAllocator::new();
        allocator.init();

        let sizes = [16, 48, 100, 200, 400, 900, 3000];
        let expected_blocks = [
            BlockSize::B32,
            BlockSize::B64,
            BlockSize::B128,
            BlockSize::B256,
            BlockSize::B512,
            BlockSize::K1,
            BlockSize::K4,
        ];

        for (size, expected) in sizes.iter().zip(expected_blocks.iter()) {
            let alloc = allocator.allocate(*size).unwrap();
            assert_eq!(alloc.block_size, *expected);
            unsafe {
                allocator.deallocate(alloc);
            }
        }
    }

    #[test]
    fn test_pool_allocator_too_large() {
        let allocator = PoolAllocator::new();
        allocator.init();

        assert!(allocator.allocate(5000).is_none());
        assert!(allocator.allocate(10000).is_none());
    }

    #[test]
    fn test_pool_stats() {
        let allocator = PoolAllocator::new();
        allocator.init();

        let initial_stats = allocator.stats();
        assert!(initial_stats.total_memory() > 0);
        assert_eq!(initial_stats.allocated_memory(), 0);

        let alloc = allocator.allocate(64).unwrap();
        let after_alloc = allocator.stats();
        assert!(after_alloc.allocated_memory() > 0);
        assert_eq!(after_alloc.total_allocations, 1);

        unsafe {
            allocator.deallocate(alloc);
        }
        let after_dealloc = allocator.stats();
        assert_eq!(after_dealloc.allocated_memory(), 0);
        assert_eq!(after_dealloc.total_deallocations, 1);
    }

    #[test]
    fn test_global_pool() {
        init();

        let alloc = allocate(100).unwrap();
        assert!(alloc.ptr.as_ptr() as usize > 0);

        unsafe {
            deallocate(alloc);
        }
    }

    #[test]
    fn test_block_size_index_roundtrip() {
        for (i, bs) in BlockSize::all().iter().enumerate() {
            assert_eq!(BlockSize::from_index(i), Some(*bs));
            assert_eq!(bs.index(), i);
        }
        assert_eq!(BlockSize::from_index(7), None);
    }

    #[test]
    fn test_pool_double_init() {
        let pool = Pool::new(64);
        pool.init();
        let first_total = pool.total_blocks();

        // Second init should be no-op
        pool.init();
        assert_eq!(pool.total_blocks(), first_total);
    }

    #[test]
    fn test_allocation_slice_access() {
        let pool = Pool::new(128);
        pool.init();

        let ptr = pool.allocate().unwrap();

        // Create an allocation struct manually for testing
        let mut alloc = PoolAllocation {
            ptr,
            block_size: BlockSize::B128,
            requested_size: 64,
        };

        unsafe {
            // Write to the memory
            let slice = alloc.as_mut_slice();
            slice[0] = 42;
            slice[63] = 99;

            // Read back
            let read_slice = alloc.as_slice();
            assert_eq!(read_slice[0], 42);
            assert_eq!(read_slice[63], 99);

            pool.deallocate(ptr);
        }
    }

    #[test]
    fn test_pool_stats_total_memory() {
        let allocator = PoolAllocator::new();
        allocator.init();

        let stats = allocator.stats();

        // Each pool has 64KB arena
        // Total should be 7 * 64KB = 448KB
        assert_eq!(stats.total_memory(), 7 * 64 * 1024);
    }
}
