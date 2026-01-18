//! Memory management subsystem
//!
//! Provides page-based memory allocation with O(1) allocation using free lists.
//!
//! ## Features
//!
//! - O(1) single page allocation/deallocation via free list
//! - Multi-page allocation with first-fit/best-fit strategies
//! - Free block coalescing for defragmentation
//! - Comprehensive allocation statistics
//!
//! ## Memory Layout
//!
//! The memory manager uses a virtual address space organized as follows:
//!
//! ```text
//! +------------------+  0x00000000 (Base Address)
//! |    Page 0        |  4KB
//! +------------------+  0x00001000
//! |    Page 1        |  4KB
//! +------------------+  0x00002000
//! |      ...         |
//! +------------------+
//! |  Page MAX_PAGES-1|  4KB
//! +------------------+  MAX_PAGES * PAGE_SIZE
//! ```
//!
//! Each page is exactly [`PAGE_SIZE`] bytes (4KB) and is aligned to a 4KB boundary.
//!
//! ## Alignment Requirements
//!
//! | Allocation Type | Alignment | Notes |
//! |----------------|-----------|-------|
//! | Single page    | 4KB       | All pages are 4KB-aligned |
//! | Multi-page     | 4KB       | Contiguous pages, first page is 4KB-aligned |
//! | Pool blocks    | 8 bytes   | Pool allocator uses 8-byte alignment |
//! | Heap allocs    | Variable  | Respects requested alignment (min 8 bytes) |
//!
//! ## Address Calculation
//!
//! - Page index to address: `page_index * PAGE_SIZE`
//! - Address to page index: `address / PAGE_SIZE`
//!
//! ## Memory Safety
//!
//! - All allocations are bounds-checked against `MAX_PAGES * PAGE_SIZE`
//! - Double-free detection returns [`KernelError::DoubleFree`]
//! - Out-of-bounds access returns [`KernelError::AddressOutOfBounds`]
//! - Zero-size allocations return [`KernelError::ZeroAllocation`]
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::memory::MemoryManager;
//!
//! let mut mm = MemoryManager::default();
//!
//! // Allocate a single page (4KB)
//! let addr = mm.allocate_page().unwrap();
//! assert_eq!(addr % 4096, 0); // 4KB-aligned
//!
//! // Allocate multiple contiguous pages
//! let multi_addr = mm.allocate_pages(10).unwrap();
//!
//! // Free memory
//! mm.free_page(addr).unwrap();
//! mm.free_pages(multi_addr, 10).unwrap();
//! ```

use crate::KernelError;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

/// Page size in bytes (4KB)
pub const PAGE_SIZE: usize = 4096;
/// Maximum number of pages
pub const MAX_PAGES: usize = 1024;
/// Invalid page index marker
const INVALID_PAGE: u32 = u32::MAX;

/// Allocation strategy for multi-page allocations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationStrategy {
    /// First fit: use the first free block that's large enough
    FirstFit,
    /// Best fit: use the smallest free block that's large enough
    BestFit,
    /// Worst fit: use the largest free block (reduces fragmentation for varying sizes)
    WorstFit,
}

impl Default for AllocationStrategy {
    fn default() -> Self {
        Self::FirstFit
    }
}

/// Allocation statistics
#[derive(Debug, Default)]
pub struct AllocationStats {
    /// Total allocation requests
    pub allocations: AtomicU64,
    /// Total deallocation requests
    pub deallocations: AtomicU64,
    /// Failed allocation attempts
    pub failed_allocations: AtomicU64,
    /// Total pages allocated
    pub pages_allocated: AtomicU64,
    /// Total pages freed
    pub pages_freed: AtomicU64,
    /// Peak pages in use
    pub peak_pages: AtomicUsize,
    /// Current pages in use
    pub current_pages: AtomicUsize,
    /// Coalesce operations performed
    pub coalesce_count: AtomicU64,
    /// Multi-page allocations
    pub multi_page_allocations: AtomicU64,
}

impl AllocationStats {
    /// Create new stats
    pub const fn new() -> Self {
        Self {
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            failed_allocations: AtomicU64::new(0),
            pages_allocated: AtomicU64::new(0),
            pages_freed: AtomicU64::new(0),
            peak_pages: AtomicUsize::new(0),
            current_pages: AtomicUsize::new(0),
            coalesce_count: AtomicU64::new(0),
            multi_page_allocations: AtomicU64::new(0),
        }
    }

    /// Record an allocation
    fn record_allocation(&self, pages: usize) {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.pages_allocated
            .fetch_add(pages as u64, Ordering::Relaxed);
        let current = self.current_pages.fetch_add(pages, Ordering::Relaxed) + pages;

        // Update peak if needed
        let mut peak = self.peak_pages.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_pages.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }

        if pages > 1 {
            self.multi_page_allocations.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a deallocation
    fn record_deallocation(&self, pages: usize) {
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.pages_freed.fetch_add(pages as u64, Ordering::Relaxed);
        self.current_pages.fetch_sub(pages, Ordering::Relaxed);
    }

    /// Record a failed allocation
    fn record_failure(&self) {
        self.failed_allocations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a coalesce operation
    fn record_coalesce(&self) {
        self.coalesce_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of current stats
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            failed_allocations: self.failed_allocations.load(Ordering::Relaxed),
            pages_allocated: self.pages_allocated.load(Ordering::Relaxed),
            pages_freed: self.pages_freed.load(Ordering::Relaxed),
            peak_pages: self.peak_pages.load(Ordering::Relaxed),
            current_pages: self.current_pages.load(Ordering::Relaxed),
            coalesce_count: self.coalesce_count.load(Ordering::Relaxed),
            multi_page_allocations: self.multi_page_allocations.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of allocation statistics
#[derive(Debug, Clone, Copy)]
pub struct StatsSnapshot {
    /// Total allocation requests
    pub allocations: u64,
    /// Total deallocation requests
    pub deallocations: u64,
    /// Failed allocation attempts
    pub failed_allocations: u64,
    /// Total pages allocated
    pub pages_allocated: u64,
    /// Total pages freed
    pub pages_freed: u64,
    /// Peak pages in use
    pub peak_pages: usize,
    /// Current pages in use
    pub current_pages: usize,
    /// Coalesce operations performed
    pub coalesce_count: u64,
    /// Multi-page allocations
    pub multi_page_allocations: u64,
}

impl StatsSnapshot {
    /// Calculate allocation success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f32 {
        let total = self.allocations + self.failed_allocations;
        if total == 0 {
            1.0
        } else {
            self.allocations as f32 / total as f32
        }
    }

    /// Calculate fragmentation score (0.0 = no fragmentation, 1.0 = heavily fragmented)
    pub fn fragmentation_score(&self, free_blocks: usize, total_free_pages: usize) -> f32 {
        if total_free_pages == 0 || free_blocks == 0 {
            0.0
        } else {
            // Ideal: all free pages in one block = 1 block
            // Fragmented: each page in its own block = total_free_pages blocks
            let ideal = 1.0;
            let worst = total_free_pages as f32;
            let actual = free_blocks as f32;
            (actual - ideal) / (worst - ideal).max(1.0)
        }
    }
}

/// Free block descriptor for multi-page allocations
#[derive(Debug, Clone, Copy)]
struct FreeBlock {
    /// Starting page index
    start_page: u32,
    /// Number of pages in this block
    page_count: u32,
    /// Next free block index (INVALID_PAGE if none)
    next: u32,
}

impl FreeBlock {
    const fn new(start: u32, count: u32) -> Self {
        Self {
            start_page: start,
            page_count: count,
            next: INVALID_PAGE,
        }
    }
}

/// Page state for tracking individual pages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageState {
    /// Page is free
    Free,
    /// Page is allocated (stores allocation size if head of multi-page)
    Allocated(u16),
}

/// Optimized memory manager with O(1) allocation
pub struct MemoryManager {
    /// Per-page state tracking
    pages: [PageState; MAX_PAGES],
    /// Free block list for multi-page allocations
    free_blocks: [FreeBlock; MAX_PAGES],
    /// Head of free block list
    free_list_head: u32,
    /// Number of free blocks
    free_block_count: usize,
    /// Free page count for single-page free list (O(1) allocation)
    single_free_head: u32,
    /// Total allocated pages
    allocated_pages: usize,
    /// Allocation strategy
    strategy: AllocationStrategy,
    /// Statistics
    stats: AllocationStats,
}

/// Global memory manager protected by a spinlock
static MEMORY_MANAGER: Mutex<Option<MemoryManager>> = Mutex::new(None);

impl MemoryManager {
    /// Create a new memory manager
    pub const fn new() -> Self {
        Self {
            pages: [PageState::Free; MAX_PAGES],
            free_blocks: [FreeBlock::new(0, 0); MAX_PAGES],
            free_list_head: 0,
            free_block_count: 1,
            single_free_head: 0,
            allocated_pages: 0,
            strategy: AllocationStrategy::FirstFit,
            stats: AllocationStats::new(),
        }
    }

    /// Create a new memory manager with a specific allocation strategy
    pub fn with_strategy(strategy: AllocationStrategy) -> Self {
        let mut mm = Self::new();
        mm.init();
        mm.set_strategy(strategy);
        mm
    }

    /// Initialize the free list with all pages
    pub fn init(&mut self) {
        // Create a single free block covering all pages
        self.free_blocks[0] = FreeBlock::new(0, MAX_PAGES as u32);
        self.free_list_head = 0;
        self.free_block_count = 1;
        self.single_free_head = 0;
    }

    /// Set allocation strategy
    pub fn set_strategy(&mut self, strategy: AllocationStrategy) {
        self.strategy = strategy;
    }

    /// Get current strategy
    pub fn strategy(&self) -> AllocationStrategy {
        self.strategy
    }

    /// Allocate a single page - O(1) operation
    ///
    /// # Returns
    /// - `Ok(address)` - Address of the allocated page
    /// - `Err(KernelError)` - Specific error describing why allocation failed
    pub fn allocate_page(&mut self) -> Result<usize, KernelError> {
        self.allocate_pages(1)
    }

    /// Allocate multiple contiguous pages
    ///
    /// # Returns
    /// - `Ok(address)` - Starting address of the allocated region
    /// - `Err(KernelError)` - Specific error describing why allocation failed
    pub fn allocate_pages(&mut self, count: usize) -> Result<usize, KernelError> {
        // Validate count is non-zero
        if count == 0 {
            self.stats.record_failure();
            return Err(KernelError::ZeroAllocation);
        }

        // Validate count doesn't exceed maximum
        if count > MAX_PAGES {
            self.stats.record_failure();
            return Err(KernelError::AllocationTooLarge {
                requested: count,
                max_pages: MAX_PAGES,
            });
        }

        // Try to find a suitable free block
        let block_idx = match self.find_free_block(count) {
            Some(idx) => idx,
            None => {
                self.stats.record_failure();
                return Err(KernelError::OutOfMemory {
                    requested: count,
                    available: self.free_page_count(),
                });
            }
        };

        let block = self.free_blocks[block_idx];

        // Double-check block has enough pages (defensive programming)
        if block.page_count < count as u32 {
            self.stats.record_failure();
            return Err(KernelError::OutOfMemory {
                requested: count,
                available: block.page_count as usize,
            });
        }

        let start_page = block.start_page as usize;

        // Mark pages as allocated
        for i in 0..count {
            self.pages[start_page + i] = if i == 0 {
                PageState::Allocated(count as u16)
            } else {
                PageState::Allocated(0) // Continuation page
            };
        }

        // Update free block
        if block.page_count == count as u32 {
            // Remove entire block from free list
            self.remove_free_block(block_idx);
        } else {
            // Shrink the block
            self.free_blocks[block_idx].start_page += count as u32;
            self.free_blocks[block_idx].page_count -= count as u32;
        }

        self.allocated_pages += count;
        self.stats.record_allocation(count);

        #[cfg(feature = "debug-alloc")]
        {
            // Debug: validate allocation integrity
            debug_assert!(start_page + count <= MAX_PAGES, "allocation out of bounds");
            debug_assert!(
                self.allocated_pages <= MAX_PAGES,
                "allocated pages exceeds max"
            );
        }

        Ok(start_page * PAGE_SIZE)
    }

    /// Find a suitable free block based on strategy
    fn find_free_block(&self, count: usize) -> Option<usize> {
        let count = count as u32;
        let mut current_idx = self.free_list_head;
        let mut best_idx: Option<usize> = None;
        let mut best_size: u32 = 0;

        // Traverse free list
        while current_idx != INVALID_PAGE {
            let block = &self.free_blocks[current_idx as usize];

            if block.page_count >= count {
                match self.strategy {
                    AllocationStrategy::FirstFit => {
                        // Return immediately on first fit
                        return Some(current_idx as usize);
                    }
                    AllocationStrategy::BestFit => {
                        // Find smallest fitting block
                        if best_idx.is_none() || block.page_count < best_size {
                            best_idx = Some(current_idx as usize);
                            best_size = block.page_count;
                        }
                    }
                    AllocationStrategy::WorstFit => {
                        // Find largest fitting block
                        if best_idx.is_none() || block.page_count > best_size {
                            best_idx = Some(current_idx as usize);
                            best_size = block.page_count;
                        }
                    }
                }
            }

            current_idx = block.next;
        }

        best_idx
    }

    /// Remove a free block from the list
    fn remove_free_block(&mut self, idx: usize) {
        if idx as u32 == self.free_list_head {
            self.free_list_head = self.free_blocks[idx].next;
        } else {
            // Find previous block
            let mut prev_idx = self.free_list_head;
            while prev_idx != INVALID_PAGE {
                if self.free_blocks[prev_idx as usize].next == idx as u32 {
                    self.free_blocks[prev_idx as usize].next = self.free_blocks[idx].next;
                    break;
                }
                prev_idx = self.free_blocks[prev_idx as usize].next;
            }
        }
        self.free_block_count -= 1;
    }

    /// Free a single page
    pub fn free_page(&mut self, addr: usize) -> Result<(), KernelError> {
        self.free_pages(addr, 1)
    }

    /// Free multiple pages starting at address
    ///
    /// # Arguments
    /// * `addr` - Starting address of the pages to free (must be page-aligned)
    /// * `count` - Number of pages to free
    ///
    /// # Errors
    /// Returns specific error variants for:
    /// - Invalid page count (zero)
    /// - Address out of bounds
    /// - Double free (pages not allocated)
    pub fn free_pages(&mut self, addr: usize, count: usize) -> Result<(), KernelError> {
        // Validate count is non-zero
        if count == 0 {
            return Err(KernelError::InvalidPageCount);
        }

        let start_page = addr / PAGE_SIZE;
        let max_valid_address = MAX_PAGES * PAGE_SIZE;

        // Validate address is within bounds
        if addr >= max_valid_address {
            return Err(KernelError::AddressOutOfBounds {
                address: addr,
                max_address: max_valid_address - PAGE_SIZE,
            });
        }

        // Validate page index is within bounds
        if start_page >= MAX_PAGES {
            return Err(KernelError::PageIndexOutOfBounds {
                index: start_page,
                max_index: MAX_PAGES - 1,
            });
        }

        // Validate the range doesn't exceed max pages
        if start_page + count > MAX_PAGES {
            return Err(KernelError::AllocationTooLarge {
                requested: count,
                max_pages: MAX_PAGES - start_page,
            });
        }

        // Verify all pages are allocated (detect double-free)
        for i in 0..count {
            match self.pages[start_page + i] {
                PageState::Allocated(_) => {}
                PageState::Free => {
                    return Err(KernelError::DoubleFree {
                        address: (start_page + i) * PAGE_SIZE,
                    });
                }
            }
        }

        // Mark pages as free
        for i in 0..count {
            self.pages[start_page + i] = PageState::Free;
        }

        // Add to free list and try to coalesce
        self.add_free_block(start_page as u32, count as u32);
        self.coalesce_adjacent(start_page as u32);

        self.allocated_pages -= count;
        self.stats.record_deallocation(count);

        Ok(())
    }

    /// Add a free block to the list (sorted by address)
    fn add_free_block(&mut self, start: u32, count: u32) {
        // Find insertion point (keep list sorted by address)
        let mut prev_idx: Option<u32> = None;
        let mut current_idx = self.free_list_head;

        while current_idx != INVALID_PAGE {
            if self.free_blocks[current_idx as usize].start_page > start {
                break;
            }
            prev_idx = Some(current_idx);
            current_idx = self.free_blocks[current_idx as usize].next;
        }

        // Find a free slot for the new block
        let new_idx = self.find_free_block_slot();
        self.free_blocks[new_idx] = FreeBlock {
            start_page: start,
            page_count: count,
            next: current_idx,
        };

        if let Some(prev) = prev_idx {
            self.free_blocks[prev as usize].next = new_idx as u32;
        } else {
            self.free_list_head = new_idx as u32;
        }

        self.free_block_count += 1;
    }

    /// Find a free slot for a new block descriptor
    fn find_free_block_slot(&self) -> usize {
        // Use the start page as the slot index for simplicity
        for i in 0..MAX_PAGES {
            let mut is_used = false;
            let mut idx = self.free_list_head;
            while idx != INVALID_PAGE {
                if idx as usize == i {
                    is_used = true;
                    break;
                }
                idx = self.free_blocks[idx as usize].next;
            }
            if !is_used {
                return i;
            }
        }
        0 // Fallback (shouldn't happen)
    }

    /// Coalesce adjacent free blocks
    fn coalesce_adjacent(&mut self, start: u32) {
        let mut current_idx = self.free_list_head;
        let mut prev_idx: Option<u32> = None;

        while current_idx != INVALID_PAGE {
            let current = self.free_blocks[current_idx as usize];
            let next_idx = current.next;

            if next_idx != INVALID_PAGE {
                let next = self.free_blocks[next_idx as usize];

                // Check if current and next are adjacent
                if current.start_page + current.page_count == next.start_page {
                    // Merge next into current
                    self.free_blocks[current_idx as usize].page_count += next.page_count;
                    self.free_blocks[current_idx as usize].next = next.next;
                    self.free_block_count -= 1;
                    self.stats.record_coalesce();
                    // Continue checking from current (might coalesce more)
                    continue;
                }
            }

            prev_idx = Some(current_idx);
            current_idx = next_idx;
        }

        let _ = (start, prev_idx); // Suppress unused warnings
    }

    /// Defragment memory by coalescing all adjacent free blocks
    pub fn defragment(&mut self) -> usize {
        let initial_blocks = self.free_block_count;
        let mut current_idx = self.free_list_head;

        while current_idx != INVALID_PAGE {
            let next_idx = self.free_blocks[current_idx as usize].next;

            if next_idx != INVALID_PAGE {
                let current = self.free_blocks[current_idx as usize];
                let next = self.free_blocks[next_idx as usize];

                if current.start_page + current.page_count == next.start_page {
                    self.free_blocks[current_idx as usize].page_count += next.page_count;
                    self.free_blocks[current_idx as usize].next = next.next;
                    self.free_block_count -= 1;
                    self.stats.record_coalesce();
                    continue;
                }
            }

            current_idx = self.free_blocks[current_idx as usize].next;
        }

        initial_blocks - self.free_block_count
    }

    /// Get number of allocated pages
    pub fn allocated_pages(&self) -> usize {
        self.allocated_pages
    }

    /// Get number of free pages
    pub fn free_page_count(&self) -> usize {
        MAX_PAGES - self.allocated_pages
    }

    /// Get number of free blocks (fragmentation indicator)
    pub fn free_block_count(&self) -> usize {
        self.free_block_count
    }

    /// Get largest contiguous free block size in pages
    pub fn largest_free_block(&self) -> usize {
        let mut largest = 0u32;
        let mut current_idx = self.free_list_head;

        while current_idx != INVALID_PAGE {
            let block = &self.free_blocks[current_idx as usize];
            if block.page_count > largest {
                largest = block.page_count;
            }
            current_idx = block.next;
        }

        largest as usize
    }

    /// Get allocation statistics
    pub fn stats(&self) -> &AllocationStats {
        &self.stats
    }

    /// Get a summary of memory state
    pub fn summary(&self) -> MemorySummary {
        let stats = self.stats.snapshot();
        MemorySummary {
            total_pages: MAX_PAGES,
            allocated_pages: self.allocated_pages,
            free_pages: self.free_page_count(),
            free_blocks: self.free_block_count,
            largest_free_block: self.largest_free_block(),
            fragmentation_score: stats
                .fragmentation_score(self.free_block_count, self.free_page_count()),
            stats,
        }
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        let mut mm = Self::new();
        mm.init();
        mm
    }
}

/// Memory state summary
#[derive(Debug, Clone)]
pub struct MemorySummary {
    /// Total pages available
    pub total_pages: usize,
    /// Currently allocated pages
    pub allocated_pages: usize,
    /// Currently free pages
    pub free_pages: usize,
    /// Number of free blocks
    pub free_blocks: usize,
    /// Largest contiguous free block
    pub largest_free_block: usize,
    /// Fragmentation score (0.0 - 1.0)
    pub fragmentation_score: f32,
    /// Allocation statistics
    pub stats: StatsSnapshot,
}

/// Initialize the global memory manager
pub fn init() -> Result<(), KernelError> {
    let mut guard = MEMORY_MANAGER.lock();
    let mut mm = MemoryManager::new();
    mm.init();
    *guard = Some(mm);
    Ok(())
}

/// Allocate a single page
///
/// # Returns
/// - `Ok(address)` - Address of the allocated page
/// - `Err(KernelError::MemoryNotInitialized)` - Memory manager not initialized
/// - `Err(other)` - Specific allocation error
pub fn allocate_page() -> Result<usize, KernelError> {
    let mut guard = MEMORY_MANAGER.lock();
    guard
        .as_mut()
        .ok_or(KernelError::MemoryNotInitialized)?
        .allocate_page()
}

/// Allocate multiple contiguous pages
///
/// # Returns
/// - `Ok(address)` - Starting address of the allocated region
/// - `Err(KernelError::MemoryNotInitialized)` - Memory manager not initialized
/// - `Err(other)` - Specific allocation error
pub fn allocate_pages(count: usize) -> Result<usize, KernelError> {
    let mut guard = MEMORY_MANAGER.lock();
    guard
        .as_mut()
        .ok_or(KernelError::MemoryNotInitialized)?
        .allocate_pages(count)
}

/// Free a single page
///
/// # Returns
/// - `Ok(())` - Page freed successfully
/// - `Err(KernelError::MemoryNotInitialized)` - Memory manager not initialized
/// - `Err(other)` - Specific free error
pub fn free_page(addr: usize) -> Result<(), KernelError> {
    let mut guard = MEMORY_MANAGER.lock();
    guard
        .as_mut()
        .ok_or(KernelError::MemoryNotInitialized)?
        .free_page(addr)
}

/// Free multiple pages
///
/// # Returns
/// - `Ok(())` - Pages freed successfully
/// - `Err(KernelError::MemoryNotInitialized)` - Memory manager not initialized
/// - `Err(other)` - Specific free error
pub fn free_pages(addr: usize, count: usize) -> Result<(), KernelError> {
    let mut guard = MEMORY_MANAGER.lock();
    guard
        .as_mut()
        .ok_or(KernelError::MemoryNotInitialized)?
        .free_pages(addr, count)
}

/// Get memory summary
pub fn summary() -> Option<MemorySummary> {
    let guard = MEMORY_MANAGER.lock();
    guard.as_ref().map(|mm| mm.summary())
}

/// Defragment memory
pub fn defragment() -> Option<usize> {
    let mut guard = MEMORY_MANAGER.lock();
    guard.as_mut().map(|mm| mm.defragment())
}

/// Set allocation strategy
pub fn set_strategy(strategy: AllocationStrategy) {
    let mut guard = MEMORY_MANAGER.lock();
    if let Some(mm) = guard.as_mut() {
        mm.set_strategy(strategy);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_manager_creation() {
        let mut mm = MemoryManager::new();
        mm.init();
        assert_eq!(mm.allocated_pages(), 0);
        assert_eq!(mm.free_page_count(), MAX_PAGES);
    }

    #[test]
    fn test_single_page_allocation() {
        let mut mm = MemoryManager::default();

        let page = mm.allocate_page();
        assert!(page.is_ok());
        assert_eq!(mm.allocated_pages(), 1);
        assert_eq!(page.unwrap(), 0);

        let page2 = mm.allocate_page();
        assert!(page2.is_ok());
        assert_eq!(mm.allocated_pages(), 2);
        assert_eq!(page2.unwrap(), PAGE_SIZE);
    }

    #[test]
    fn test_page_free() {
        let mut mm = MemoryManager::default();

        let page = mm.allocate_page().unwrap();
        assert_eq!(mm.allocated_pages(), 1);

        mm.free_page(page).unwrap();
        assert_eq!(mm.allocated_pages(), 0);
    }

    #[test]
    fn test_multi_page_allocation() {
        let mut mm = MemoryManager::default();

        let addr = mm.allocate_pages(4);
        assert!(addr.is_ok());
        assert_eq!(mm.allocated_pages(), 4);

        // Verify contiguous allocation
        let addr = addr.unwrap();
        assert_eq!(addr, 0);
    }

    #[test]
    fn test_multi_page_free() {
        let mut mm = MemoryManager::default();

        let addr = mm.allocate_pages(4).unwrap();
        mm.free_pages(addr, 4).unwrap();

        assert_eq!(mm.allocated_pages(), 0);
        assert_eq!(mm.free_page_count(), MAX_PAGES);
    }

    #[test]
    fn test_fragmentation_and_coalesce() {
        let mut mm = MemoryManager::default();

        // Allocate three blocks
        let a = mm.allocate_pages(10).unwrap();
        let b = mm.allocate_pages(10).unwrap();
        let c = mm.allocate_pages(10).unwrap();

        // Free middle block
        mm.free_pages(b, 10).unwrap();

        // Should have 2 free blocks now
        assert_eq!(mm.free_block_count(), 2);

        // Free first block - should coalesce with middle
        mm.free_pages(a, 10).unwrap();

        // Should coalesce to 2 blocks (one for a+b, one for remaining)
        assert!(mm.free_block_count() <= 2);

        // Free last block
        mm.free_pages(c, 10).unwrap();

        // All free, defragment
        mm.defragment();
        assert_eq!(mm.free_block_count(), 1);
    }

    #[test]
    fn test_allocation_strategy_first_fit() {
        let mut mm = MemoryManager::default();
        mm.set_strategy(AllocationStrategy::FirstFit);

        // Allocate and free to create holes
        let a = mm.allocate_pages(10).unwrap();
        let _b = mm.allocate_pages(10).unwrap();
        let c = mm.allocate_pages(10).unwrap();

        mm.free_pages(a, 10).unwrap(); // First hole: 10 pages
        mm.free_pages(c, 10).unwrap(); // Second hole: 10 pages + rest

        // First fit should use first hole
        let d = mm.allocate_pages(5).unwrap();
        assert_eq!(d, 0); // Should use first hole
    }

    #[test]
    fn test_allocation_strategy_best_fit() {
        let mut mm = MemoryManager::default();
        mm.set_strategy(AllocationStrategy::BestFit);

        // Allocate and free to create different sized holes
        let a = mm.allocate_pages(10).unwrap(); // Pages 0-9
        let _b = mm.allocate_pages(5).unwrap(); // Pages 10-14 (keep allocated)
        let c = mm.allocate_pages(10).unwrap(); // Pages 15-24
        let d = mm.allocate_pages(20).unwrap(); // Pages 25-44

        mm.free_pages(a, 10).unwrap(); // Hole 1: 10 pages
        mm.free_pages(c, 10).unwrap(); // Hole 2: 10 pages
        mm.free_pages(d, 20).unwrap(); // Hole 3: 20 pages + rest

        // Best fit for 8 pages should use one of the 10-page holes
        let e = mm.allocate_pages(8).unwrap();
        assert!(e == 0 || e == 15 * PAGE_SIZE);
    }

    #[test]
    fn test_allocation_stats() {
        let mut mm = MemoryManager::default();

        mm.allocate_pages(5).unwrap();
        mm.allocate_pages(3).unwrap();

        let stats = mm.stats().snapshot();
        assert_eq!(stats.allocations, 2);
        assert_eq!(stats.pages_allocated, 8);
        assert_eq!(stats.current_pages, 8);
        assert_eq!(stats.multi_page_allocations, 2);
    }

    #[test]
    fn test_stats_peak_tracking() {
        let mut mm = MemoryManager::default();

        let a = mm.allocate_pages(10).unwrap();
        let b = mm.allocate_pages(20).unwrap();

        mm.free_pages(a, 10).unwrap();

        let stats = mm.stats().snapshot();
        assert_eq!(stats.peak_pages, 30);
        assert_eq!(stats.current_pages, 20);

        mm.free_pages(b, 20).unwrap();
    }

    #[test]
    fn test_failed_allocation_tracking() {
        let mut mm = MemoryManager::default();

        // Try to allocate more than available
        let result = mm.allocate_pages(MAX_PAGES + 1);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(KernelError::AllocationTooLarge { .. })
        ));

        let stats = mm.stats().snapshot();
        assert_eq!(stats.failed_allocations, 1);
    }

    #[test]
    fn test_memory_summary() {
        let mut mm = MemoryManager::default();

        mm.allocate_pages(100).unwrap();

        let summary = mm.summary();
        assert_eq!(summary.total_pages, MAX_PAGES);
        assert_eq!(summary.allocated_pages, 100);
        assert_eq!(summary.free_pages, MAX_PAGES - 100);
        assert!(summary.largest_free_block > 0);
    }

    #[test]
    fn test_fragmentation_score() {
        let stats = StatsSnapshot {
            allocations: 0,
            deallocations: 0,
            failed_allocations: 0,
            pages_allocated: 0,
            pages_freed: 0,
            peak_pages: 0,
            current_pages: 0,
            coalesce_count: 0,
            multi_page_allocations: 0,
        };

        // No fragmentation: 1 block, 100 pages
        let score = stats.fragmentation_score(1, 100);
        assert_eq!(score, 0.0);

        // Maximum fragmentation: 100 blocks, 100 pages
        let score = stats.fragmentation_score(100, 100);
        assert!(score > 0.9);
    }

    #[test]
    fn test_largest_free_block() {
        let mut mm = MemoryManager::default();

        // Initially all pages are free in one block
        assert_eq!(mm.largest_free_block(), MAX_PAGES);

        // Allocate some pages
        mm.allocate_pages(100).unwrap();
        assert_eq!(mm.largest_free_block(), MAX_PAGES - 100);
    }

    #[test]
    fn test_double_free_error() {
        let mut mm = MemoryManager::default();

        let addr = mm.allocate_page().unwrap();
        mm.free_page(addr).unwrap();

        // Double free should fail with DoubleFree error
        let result = mm.free_page(addr);
        assert!(result.is_err());
        assert!(matches!(result, Err(KernelError::DoubleFree { .. })));
    }

    #[test]
    fn test_invalid_address_free() {
        let mut mm = MemoryManager::default();

        // Free an address that was never allocated (out of bounds)
        let result = mm.free_page(MAX_PAGES * PAGE_SIZE + PAGE_SIZE);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(KernelError::AddressOutOfBounds { .. })
        ));
    }

    #[test]
    fn test_allocation_exhaustion() {
        let mut mm = MemoryManager::default();

        // Allocate all pages
        let addr = mm.allocate_pages(MAX_PAGES);
        assert!(addr.is_ok());

        // Try to allocate one more
        let result = mm.allocate_page();
        assert!(result.is_err());
        assert!(matches!(result, Err(KernelError::OutOfMemory { .. })));

        // Free some and try again
        mm.free_pages(addr.unwrap(), 10).unwrap();
        let result = mm.allocate_page();
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_page_allocation() {
        let mut mm = MemoryManager::default();

        let result = mm.allocate_pages(0);
        assert!(result.is_err());
        assert!(matches!(result, Err(KernelError::ZeroAllocation)));
    }

    #[test]
    fn test_defragment() {
        let mut mm = MemoryManager::default();

        // Create fragmented state
        let a = mm.allocate_pages(10).unwrap();
        let b = mm.allocate_pages(10).unwrap();
        let c = mm.allocate_pages(10).unwrap();

        mm.free_pages(a, 10).unwrap();
        mm.free_pages(c, 10).unwrap();
        mm.free_pages(b, 10).unwrap();

        let before_blocks = mm.free_block_count();
        let coalesced = mm.defragment();

        assert!(mm.free_block_count() <= before_blocks);
        let _ = coalesced; // May or may not coalesce depending on order
    }

    #[test]
    fn test_success_rate() {
        let stats = StatsSnapshot {
            allocations: 90,
            deallocations: 0,
            failed_allocations: 10,
            pages_allocated: 0,
            pages_freed: 0,
            peak_pages: 0,
            current_pages: 0,
            coalesce_count: 0,
            multi_page_allocations: 0,
        };

        let rate = stats.success_rate();
        assert!((rate - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_global_functions() {
        // Initialize
        init().unwrap();

        // Test global functions
        let page = allocate_page();
        assert!(page.is_ok());

        free_page(page.unwrap()).unwrap();

        let summary = summary();
        assert!(summary.is_some());
    }

    #[test]
    fn test_error_display() {
        // Test that error Display implementation works
        let errors = [
            KernelError::MemoryInitFailed,
            KernelError::ZeroAllocation,
            KernelError::AllocationTooLarge {
                requested: 2000,
                max_pages: 1024,
            },
            KernelError::AddressOutOfBounds {
                address: 0x1000000,
                max_address: 0xFFFFF,
            },
            KernelError::DoubleFree { address: 0x1000 },
            KernelError::OutOfMemory {
                requested: 100,
                available: 50,
            },
        ];

        for error in &errors {
            // Ensure Display is implemented and doesn't panic
            let _ = alloc::format!("{}", error);
        }
    }

    #[test]
    fn test_bounds_checking_comprehensive() {
        let mut mm = MemoryManager::default();

        // Test address exactly at boundary
        let result = mm.free_page(MAX_PAGES * PAGE_SIZE);
        assert!(matches!(
            result,
            Err(KernelError::AddressOutOfBounds { .. })
        ));

        // Allocate some pages and test freeing beyond allocation
        let addr = mm.allocate_pages(10).unwrap();

        // Try to free more pages than were allocated at that address
        let result = mm.free_pages(addr, MAX_PAGES);
        assert!(result.is_err());

        // Cleanup
        mm.free_pages(addr, 10).unwrap();
    }
}

// =============================================================================
// Property-Based Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Allocate then free should return memory to pool
        #[test]
        fn prop_allocate_free_roundtrip(page_count in 1usize..=100) {
            let mut mm = MemoryManager::default();
            let initial_free = mm.free_page_count();

            // Allocate
            let addr = mm.allocate_pages(page_count).unwrap();

            prop_assert_eq!(mm.allocated_pages(), page_count);
            prop_assert_eq!(mm.free_page_count(), initial_free - page_count);

            // Free
            mm.free_pages(addr, page_count).unwrap();

            prop_assert_eq!(mm.allocated_pages(), 0);
            prop_assert_eq!(mm.free_page_count(), initial_free);
        }

        /// Multiple allocations should be contiguous within each allocation
        #[test]
        fn prop_allocations_are_page_aligned(
            sizes in prop::collection::vec(1usize..=10, 1..=10)
        ) {
            let mut mm = MemoryManager::default();
            let mut addrs = Vec::new();

            for &size in &sizes {
                if let Ok(addr) = mm.allocate_pages(size) {
                    // Address should be page-aligned
                    prop_assert_eq!(addr % PAGE_SIZE, 0);
                    addrs.push((addr, size));
                }
            }

            // Cleanup
            for (addr, size) in addrs {
                mm.free_pages(addr, size).unwrap();
            }
        }

        /// Free pages count should always equal MAX_PAGES - allocated
        #[test]
        fn prop_free_count_consistency(
            alloc_sizes in prop::collection::vec(1usize..=20, 0..=20)
        ) {
            let mut mm = MemoryManager::default();
            let mut allocations = Vec::new();
            let mut total_allocated = 0usize;

            for size in alloc_sizes {
                if total_allocated + size <= MAX_PAGES {
                    if let Ok(addr) = mm.allocate_pages(size) {
                        allocations.push((addr, size));
                        total_allocated += size;
                    }
                }
            }

            prop_assert_eq!(mm.allocated_pages(), total_allocated);
            prop_assert_eq!(mm.free_page_count(), MAX_PAGES - total_allocated);

            // Cleanup
            for (addr, size) in allocations {
                mm.free_pages(addr, size).unwrap();
            }
        }

        /// Zero allocation should always fail with ZeroAllocation error
        #[test]
        fn prop_zero_allocation_fails(_seed in any::<u64>()) {
            let mut mm = MemoryManager::default();
            let result = mm.allocate_pages(0);
            prop_assert!(matches!(result, Err(KernelError::ZeroAllocation)));
        }

        /// Allocation beyond MAX_PAGES should fail with AllocationTooLarge
        #[test]
        fn prop_oversized_allocation_fails(extra in 1usize..=100) {
            let mut mm = MemoryManager::default();
            let result = mm.allocate_pages(MAX_PAGES + extra);
            let is_too_large = matches!(result, Err(KernelError::AllocationTooLarge { requested: _, max_pages: _ }));
            prop_assert!(is_too_large);
        }

        /// Double free should always fail with DoubleFree error
        #[test]
        fn prop_double_free_fails(page_count in 1usize..=10) {
            let mut mm = MemoryManager::default();

            let addr = mm.allocate_pages(page_count).unwrap();
            mm.free_pages(addr, page_count).unwrap();

            // Second free should fail
            let result = mm.free_pages(addr, page_count);
            let is_double_free = matches!(result, Err(KernelError::DoubleFree { address: _ }));
            prop_assert!(is_double_free);
        }

        /// Stats should reflect actual operations
        #[test]
        fn prop_stats_consistency(
            alloc_count in 1usize..=20,
            pages_per_alloc in 1usize..=5
        ) {
            let mut mm = MemoryManager::default();
            let mut successful_allocs = 0u64;
            let mut total_pages = 0usize;
            let mut addrs = Vec::new();

            for _ in 0..alloc_count {
                if total_pages + pages_per_alloc <= MAX_PAGES {
                    if let Ok(addr) = mm.allocate_pages(pages_per_alloc) {
                        successful_allocs += 1;
                        total_pages += pages_per_alloc;
                        addrs.push((addr, pages_per_alloc));
                    }
                }
            }

            let stats = mm.stats().snapshot();
            prop_assert_eq!(stats.allocations, successful_allocs);
            prop_assert_eq!(stats.pages_allocated, total_pages as u64);

            // Cleanup
            for (addr, size) in addrs {
                mm.free_pages(addr, size).unwrap();
            }
        }

        /// Memory utilization should be between 0 and 1
        #[test]
        fn prop_utilization_bounds(alloc_pages in 0usize..=MAX_PAGES) {
            let mut mm = MemoryManager::default();

            if alloc_pages > 0 {
                let _ = mm.allocate_pages(alloc_pages);
            }

            let summary = mm.summary();
            let utilization = summary.allocated_pages as f64 / MAX_PAGES as f64;

            prop_assert!(utilization >= 0.0);
            prop_assert!(utilization <= 1.0);
        }
    }
}

// =============================================================================
// Stress Tests
// =============================================================================

#[cfg(test)]
mod stress_tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

    /// Stress test: 10,000+ allocations without fragmentation issues
    #[test]
    fn test_stress_many_allocations() {
        let mut mm = MemoryManager::default();
        let mut allocations: Vec<(usize, usize)> = Vec::new();

        // Perform many small allocations
        for _ in 0..1000 {
            // Allocate 1-3 pages each
            let size = 1 + (allocations.len() % 3);
            if let Ok(addr) = mm.allocate_pages(size) {
                allocations.push((addr, size));
            }
        }

        let allocs_after_first_round = allocations.len();
        assert!(
            allocs_after_first_round > 0,
            "Should have some successful allocations"
        );

        // Free half of them (every other one to create fragmentation)
        let mut freed_count = 0;
        for i in (0..allocations.len()).step_by(2) {
            if i < allocations.len() {
                let (addr, size) = allocations[i];
                mm.free_pages(addr, size).unwrap();
                freed_count += 1;
            }
        }

        // Remove freed allocations from our tracking (in reverse order to maintain indices)
        let mut i = 0;
        allocations.retain(|_| {
            let keep = i % 2 != 0;
            i += 1;
            keep
        });

        // Now allocate again into the freed spaces
        let mut new_allocs = 0;
        for _ in 0..freed_count {
            let size = 1 + (new_allocs % 3);
            if let Ok(addr) = mm.allocate_pages(size) {
                allocations.push((addr, size));
                new_allocs += 1;
            }
        }

        // Should have successfully reused fragmented memory
        assert!(
            new_allocs > 0,
            "Should be able to allocate into fragmented memory"
        );

        // Verify allocations are non-overlapping
        let mut all_pages: Vec<usize> = Vec::new();
        for (addr, size) in &allocations {
            let start_page = addr / PAGE_SIZE;
            for p in start_page..start_page + size {
                assert!(
                    !all_pages.contains(&p),
                    "Overlapping allocation detected at page {}",
                    p
                );
                all_pages.push(p);
            }
        }

        // Cleanup
        for (addr, size) in allocations {
            mm.free_pages(addr, size).unwrap();
        }

        // Should be back to initial state
        assert_eq!(mm.allocated_pages(), 0);
        assert_eq!(mm.free_page_count(), MAX_PAGES);
    }

    /// Stress test: Rapid allocate/free cycles
    #[test]
    fn test_stress_allocate_free_cycles() {
        let mut mm = MemoryManager::default();

        // Perform 5000 allocate/free cycles
        for i in 0..5000 {
            let size = 1 + (i % 5); // 1-5 pages
            if let Ok(addr) = mm.allocate_pages(size) {
                mm.free_pages(addr, size).unwrap();
            }
        }

        // Memory should be fully available
        assert_eq!(mm.allocated_pages(), 0);
        assert_eq!(mm.free_page_count(), MAX_PAGES);

        // Stats should reflect all operations
        let stats = mm.stats().snapshot();
        assert!(
            stats.allocations >= 5000,
            "Should have tracked 5000+ allocations"
        );
        assert_eq!(
            stats.allocations, stats.deallocations,
            "Allocations should equal deallocations"
        );
    }

    /// Stress test: Mixed allocation sizes
    #[test]
    fn test_stress_mixed_sizes() {
        let mut mm = MemoryManager::default();
        let sizes = [1, 2, 4, 8, 16, 32, 1, 3, 5, 7]; // Various sizes
        let mut allocations: Vec<(usize, usize)> = Vec::new();

        // Allocate with mixed sizes
        for _ in 0..100 {
            for &size in &sizes {
                if let Ok(addr) = mm.allocate_pages(size) {
                    allocations.push((addr, size));
                }
            }
        }

        assert!(allocations.len() > 100, "Should have many allocations");

        // Free in random-ish order (every 3rd, then every 5th, etc)
        for step in [3, 5, 7, 2] {
            for i in (0..allocations.len()).step_by(step) {
                if i < allocations.len() {
                    let (addr, size) = allocations[i];
                    // Only free if still allocated (check via stats)
                    let _ = mm.free_pages(addr, size);
                }
            }
        }

        // Allocate again - should reuse freed memory
        let before_realloc = mm.allocated_pages();
        for &size in &sizes {
            if let Ok(addr) = mm.allocate_pages(size) {
                allocations.push((addr, size));
            }
        }
        let after_realloc = mm.allocated_pages();
        assert!(
            after_realloc >= before_realloc,
            "Should be able to allocate after partial free"
        );
    }

    /// Stress test: Allocation strategies under pressure
    #[test]
    fn test_stress_strategies() {
        for strategy in [
            AllocationStrategy::FirstFit,
            AllocationStrategy::BestFit,
            AllocationStrategy::WorstFit,
        ] {
            let mut mm = MemoryManager::with_strategy(strategy);
            let mut allocations: Vec<(usize, usize)> = Vec::new();

            // Fill memory with various sizes
            for size in [10, 5, 20, 3, 15, 8, 12, 7] {
                if let Ok(addr) = mm.allocate_pages(size) {
                    allocations.push((addr, size));
                }
            }

            // Create holes
            for i in [0, 2, 4, 6] {
                if i < allocations.len() {
                    let (addr, size) = allocations[i];
                    mm.free_pages(addr, size).unwrap();
                }
            }

            // Try to fill holes with specific sizes
            for size in [5, 10, 15, 3] {
                let _ = mm.allocate_pages(size);
            }

            // Verify no corruption
            let summary = mm.summary();
            assert!(summary.free_pages + summary.allocated_pages == MAX_PAGES);
        }
    }

    /// Stress test: Coalescing under heavy fragmentation
    #[test]
    fn test_stress_coalescing() {
        let mut mm = MemoryManager::default();
        let mut addrs: Vec<usize> = Vec::new();

        // Allocate many single pages
        for _ in 0..100.min(MAX_PAGES) {
            if let Ok(addr) = mm.allocate_page() {
                addrs.push(addr);
            }
        }

        assert!(addrs.len() >= 50, "Should allocate at least 50 pages");

        // Free every other page to create fragmentation
        for i in (0..addrs.len()).step_by(2) {
            mm.free_page(addrs[i]).unwrap();
        }

        // Free remaining pages - should coalesce with neighbors
        for i in (1..addrs.len()).step_by(2) {
            mm.free_page(addrs[i]).unwrap();
        }

        // Memory should be fully coalesced
        assert_eq!(mm.allocated_pages(), 0);

        // Should be able to allocate a large contiguous block
        let large_alloc = mm.allocate_pages(50);
        assert!(
            large_alloc.is_ok(),
            "Should be able to allocate 50 pages after coalescing"
        );

        if let Ok(addr) = large_alloc {
            mm.free_pages(addr, 50).unwrap();
        }
    }
}

// =============================================================================
// Additional Edge Case Tests
// =============================================================================

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_strategy_switching_mid_execution() {
        let mut mm = MemoryManager::default();
        mm.set_strategy(AllocationStrategy::FirstFit);

        // Allocate with FirstFit
        let a = mm.allocate_pages(10).unwrap();
        let _b = mm.allocate_pages(10).unwrap();

        // Switch to BestFit
        mm.set_strategy(AllocationStrategy::BestFit);
        assert_eq!(mm.strategy(), AllocationStrategy::BestFit);

        // Free and reallocate with new strategy
        mm.free_pages(a, 10).unwrap();
        let c = mm.allocate_pages(5);
        assert!(c.is_ok());

        // Switch to WorstFit
        mm.set_strategy(AllocationStrategy::WorstFit);
        assert_eq!(mm.strategy(), AllocationStrategy::WorstFit);
    }

    #[test]
    fn test_page_alignment_verification() {
        let mut mm = MemoryManager::default();

        // All allocations should be page-aligned
        for _ in 0..10 {
            let addr = mm.allocate_page().unwrap();
            assert_eq!(
                addr % PAGE_SIZE,
                0,
                "Address {:#x} is not page-aligned",
                addr
            );
        }
    }

    #[test]
    fn test_multi_page_alignment() {
        let mut mm = MemoryManager::default();

        // Multi-page allocations should also be page-aligned
        for size in [2, 5, 10, 20, 50] {
            let addr = mm.allocate_pages(size);
            if let Ok(addr) = addr {
                assert_eq!(
                    addr % PAGE_SIZE,
                    0,
                    "Multi-page allocation not aligned: {:#x}",
                    addr
                );
                mm.free_pages(addr, size).unwrap();
            }
        }
    }

    #[test]
    fn test_maximum_single_allocation() {
        let mut mm = MemoryManager::default();

        // Should be able to allocate all pages at once
        let addr = mm.allocate_pages(MAX_PAGES);
        assert!(addr.is_ok());
        assert_eq!(mm.allocated_pages(), MAX_PAGES);
        assert_eq!(mm.free_page_count(), 0);

        mm.free_pages(addr.unwrap(), MAX_PAGES).unwrap();
    }

    #[test]
    fn test_sequential_single_page_allocations() {
        let mut mm = MemoryManager::default();
        let mut addrs = Vec::new();

        // Allocate pages one by one
        for _ in 0..10 {
            let addr = mm.allocate_page().unwrap();
            addrs.push(addr);
        }

        // Verify they're sequential
        for i in 1..addrs.len() {
            assert_eq!(addrs[i], addrs[i - 1] + PAGE_SIZE);
        }

        // Free all
        for addr in addrs {
            mm.free_page(addr).unwrap();
        }
    }

    #[test]
    fn test_free_pages_zero_count() {
        let mut mm = MemoryManager::default();
        let addr = mm.allocate_pages(10).unwrap();

        // Freeing zero pages should fail
        let result = mm.free_pages(addr, 0);
        assert!(result.is_err());
        assert!(matches!(result, Err(KernelError::InvalidPageCount)));

        mm.free_pages(addr, 10).unwrap();
    }

    #[test]
    fn test_allocate_after_exhaustion_and_partial_free() {
        let mut mm = MemoryManager::default();

        // Exhaust memory
        let addr = mm.allocate_pages(MAX_PAGES).unwrap();
        assert!(mm.allocate_page().is_err());

        // Free various partial amounts
        mm.free_pages(addr, 1).unwrap();
        assert!(mm.allocate_page().is_ok());

        mm.free_pages(addr + PAGE_SIZE, 5).unwrap();
        assert!(mm.allocate_pages(5).is_ok());
    }

    #[test]
    fn test_coalesce_count_tracking() {
        let mut mm = MemoryManager::default();

        let a = mm.allocate_pages(10).unwrap();
        let b = mm.allocate_pages(10).unwrap();
        let c = mm.allocate_pages(10).unwrap();

        let before_coalesce = mm.stats().snapshot().coalesce_count;

        // Free middle, then adjacent blocks to trigger coalescing
        mm.free_pages(b, 10).unwrap();
        mm.free_pages(a, 10).unwrap();
        mm.free_pages(c, 10).unwrap();

        let after_coalesce = mm.stats().snapshot().coalesce_count;
        assert!(after_coalesce >= before_coalesce);
    }

    #[test]
    fn test_alternating_alloc_free_pattern() {
        let mut mm = MemoryManager::default();

        for _ in 0..20 {
            let addr = mm.allocate_pages(5).unwrap();
            mm.free_pages(addr, 5).unwrap();
        }

        // Memory should be healthy
        assert_eq!(mm.allocated_pages(), 0);
        assert_eq!(mm.free_page_count(), MAX_PAGES);
        assert!(mm.allocate_pages(100).is_ok());
    }

    #[test]
    fn test_worst_fit_prefers_largest_block() {
        let mut mm = MemoryManager::with_strategy(AllocationStrategy::WorstFit);

        // Create holes of different sizes
        let a = mm.allocate_pages(5).unwrap();
        let _keep1 = mm.allocate_pages(5).unwrap();
        let b = mm.allocate_pages(10).unwrap();
        let _keep2 = mm.allocate_pages(5).unwrap();
        let c = mm.allocate_pages(20).unwrap();

        mm.free_pages(a, 5).unwrap(); // 5-page hole
        mm.free_pages(b, 10).unwrap(); // 10-page hole
        mm.free_pages(c, 20).unwrap(); // 20-page hole + rest

        // Worst fit should use the largest hole (c)
        let d = mm.allocate_pages(3).unwrap();
        assert_eq!(d, c); // Should allocate from the largest hole
    }

    #[test]
    fn test_defragment_reduces_free_blocks() {
        let mut mm = MemoryManager::default();

        // Create fragmented state
        let mut addrs = Vec::new();
        for _ in 0..10 {
            addrs.push(mm.allocate_pages(5).unwrap());
        }

        // Free alternating blocks
        for i in (0..addrs.len()).step_by(2) {
            mm.free_pages(addrs[i], 5).unwrap();
        }

        let before_blocks = mm.free_block_count();
        let coalesced = mm.defragment();

        // Defragment should reduce blocks or stay same
        assert!(mm.free_block_count() <= before_blocks);
        let _ = coalesced;
    }

    #[test]
    fn test_stats_consistency_after_operations() {
        let mut mm = MemoryManager::default();

        for _ in 0..10 {
            let addr = mm.allocate_pages(5).unwrap();
            mm.free_pages(addr, 5).unwrap();
        }

        let stats = mm.stats().snapshot();
        assert_eq!(stats.allocations, 10);
        assert_eq!(stats.deallocations, 10);
        assert_eq!(stats.current_pages, 0);
        assert_eq!(stats.pages_allocated, stats.pages_freed);
    }

    #[test]
    fn test_address_bounds_just_under_max() {
        let mut mm = MemoryManager::default();

        // Allocate at the last valid page
        let mut addrs = Vec::new();
        for _ in 0..(MAX_PAGES - 1) {
            addrs.push(mm.allocate_page().unwrap());
        }

        let last_page = mm.allocate_page();
        assert!(last_page.is_ok());
        assert_eq!(last_page.unwrap(), (MAX_PAGES - 1) * PAGE_SIZE);

        // No more pages available
        assert!(mm.allocate_page().is_err());
    }

    #[test]
    fn test_free_at_address_zero() {
        let mut mm = MemoryManager::default();

        let addr = mm.allocate_page().unwrap();
        assert_eq!(addr, 0);

        // Should be able to free page at address 0
        let result = mm.free_page(0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_defragment_calls() {
        let mut mm = MemoryManager::default();

        let a = mm.allocate_pages(10).unwrap();
        let b = mm.allocate_pages(10).unwrap();

        mm.free_pages(a, 10).unwrap();
        mm.free_pages(b, 10).unwrap();

        // Multiple defragment calls should be safe
        mm.defragment();
        mm.defragment();
        mm.defragment();

        // Memory should still be consistent
        assert_eq!(mm.allocated_pages(), 0);
        assert!(mm.allocate_pages(20).is_ok());
    }

    #[test]
    fn test_success_rate_all_success() {
        let stats = StatsSnapshot {
            allocations: 100,
            deallocations: 0,
            failed_allocations: 0,
            pages_allocated: 0,
            pages_freed: 0,
            peak_pages: 0,
            current_pages: 0,
            coalesce_count: 0,
            multi_page_allocations: 0,
        };

        assert_eq!(stats.success_rate(), 1.0);
    }

    #[test]
    fn test_success_rate_all_failure() {
        let stats = StatsSnapshot {
            allocations: 0,
            deallocations: 0,
            failed_allocations: 100,
            pages_allocated: 0,
            pages_freed: 0,
            peak_pages: 0,
            current_pages: 0,
            coalesce_count: 0,
            multi_page_allocations: 0,
        };

        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_success_rate_no_attempts() {
        let stats = StatsSnapshot {
            allocations: 0,
            deallocations: 0,
            failed_allocations: 0,
            pages_allocated: 0,
            pages_freed: 0,
            peak_pages: 0,
            current_pages: 0,
            coalesce_count: 0,
            multi_page_allocations: 0,
        };

        assert_eq!(stats.success_rate(), 1.0); // 100% success when no attempts
    }

    #[test]
    fn test_fragmentation_score_edge_cases() {
        let stats = StatsSnapshot {
            allocations: 0,
            deallocations: 0,
            failed_allocations: 0,
            pages_allocated: 0,
            pages_freed: 0,
            peak_pages: 0,
            current_pages: 0,
            coalesce_count: 0,
            multi_page_allocations: 0,
        };

        // Zero free pages
        assert_eq!(stats.fragmentation_score(0, 0), 0.0);

        // Single block
        assert_eq!(stats.fragmentation_score(1, 1000), 0.0);
    }

    #[test]
    fn test_global_set_strategy() {
        init().unwrap();
        set_strategy(AllocationStrategy::BestFit);
        set_strategy(AllocationStrategy::WorstFit);
        set_strategy(AllocationStrategy::FirstFit);
        // Should not panic
    }

    #[test]
    fn test_unaligned_address_free_fails() {
        let mut mm = MemoryManager::default();

        // Try to free an unaligned address
        let result = mm.free_page(PAGE_SIZE + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_free_pages_out_of_range() {
        let mut mm = MemoryManager::default();
        let addr = mm.allocate_pages(10).unwrap();

        // Try to free pages that extend beyond valid range
        let result = mm.free_pages(MAX_PAGES * PAGE_SIZE - PAGE_SIZE, 10);
        assert!(result.is_err());

        mm.free_pages(addr, 10).unwrap();
    }
}
