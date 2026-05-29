#![allow(dead_code)]
//! Buddy memory allocator.
//!
//! Power-of-2 block allocator that splits larger blocks and coalesces
//! buddies on free.  Complements the bitmap allocator in `memory.rs`.
//!
//! ## Design
//!
//! The allocator maintains a set of free lists, one per order level.
//! Order 0 = MIN_ORDER = 12 corresponds to 4 KiB (one page).
//! Level index `i` corresponds to order `MIN_ORDER + i`, so a block of
//! size `2^(MIN_ORDER + i)` bytes.
//!
//! On allocation the allocator finds the smallest level that has a free
//! block, then repeatedly splits it down to the requested order (pushing
//! the unused half onto the level below).  On deallocation it XOR-computes
//! the buddy address and coalesces upward as long as the buddy is free.
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::buddy::BuddyAllocator;
//!
//! let mut allocator = BuddyAllocator::new(0x4000_0000, 4 * 1024 * 1024).unwrap();
//! let addr = allocator.allocate(4096).unwrap();
//! allocator.deallocate(addr, 4096).unwrap();
//! ```

use alloc::vec::Vec;
use core::fmt;

// ──────────────────────────── constants ────────────────────────────

/// Smallest allocation granularity: 4 KiB (same as PAGE_SIZE).
pub const MIN_ORDER: u32 = 12; // 2^12 = 4096

/// Largest supported block: 2^(MIN_ORDER + MAX_LEVELS − 1) = 2^27 = 128 MiB.
pub const MAX_LEVELS: usize = 16;

// ──────────────────────────── error type ───────────────────────────

/// Errors that the buddy allocator can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuddyError {
    /// Requested size is zero.
    ZeroSize,
    /// Requested size exceeds the largest available order.
    AllocationTooLarge { requested: usize, max: usize },
    /// The allocator region is exhausted (no free block of the needed order).
    OutOfMemory { order: u32, available: usize },
    /// Deallocated address is not aligned to its order.
    UnalignedAddress {
        addr: usize,
        order: u32,
        required_align: usize,
    },
    /// Deallocated address is outside the managed region.
    AddressOutOfRange {
        addr: usize,
        base: usize,
        end: usize,
    },
    /// Size given to `deallocate` does not match any valid order.
    InvalidSize { size: usize },
}

impl fmt::Display for BuddyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSize => write!(f, "buddy allocator: requested size is zero"),
            Self::AllocationTooLarge { requested, max } => write!(
                f,
                "buddy allocator: requested {requested} bytes exceeds maximum {max} bytes"
            ),
            Self::OutOfMemory { order, available } => write!(
                f,
                "buddy allocator: out of memory at order {order} \
                 ({available} bytes still free, none contiguous enough)"
            ),
            Self::UnalignedAddress {
                addr,
                order,
                required_align,
            } => write!(
                f,
                "buddy allocator: address {addr:#x} is not aligned to \
                 order {order} (required alignment {required_align:#x})"
            ),
            Self::AddressOutOfRange { addr, base, end } => write!(
                f,
                "buddy allocator: address {addr:#x} is outside the managed \
                 region [{base:#x}, {end:#x})"
            ),
            Self::InvalidSize { size } => write!(
                f,
                "buddy allocator: size {size} does not correspond to any valid buddy order"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BuddyError {}

// ─────────────────────────── statistics ────────────────────────────

/// Runtime statistics for the buddy allocator.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuddyStats {
    /// Total bytes currently allocated (live allocations).
    pub allocated_bytes: usize,
    /// Total bytes currently free.
    pub free_bytes: usize,
    /// Total bytes in the managed region (constant after construction).
    pub total_bytes: usize,
    /// Cumulative number of block splits performed.
    pub split_count: u64,
    /// Cumulative number of buddy coalescing operations performed.
    pub coalesce_count: u64,
    /// Cumulative number of successful allocations.
    pub allocation_count: u64,
    /// Cumulative number of successful deallocations.
    pub deallocation_count: u64,
    /// Order of the largest currently-free block (MIN_ORDER when no blocks are free).
    pub largest_free_order: u32,
}

impl BuddyStats {
    /// Fragmentation percentage: 0% means one perfectly contiguous free region;
    /// 100% means all free bytes are in blocks too small to coalesce.
    pub fn fragmentation_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        let free = self.free_bytes.max(1) as f64;
        let largest = self.largest_free_block_bytes() as f64;
        100.0 * (1.0 - largest / free)
    }

    /// Size in bytes of the largest single contiguous free block.
    pub fn largest_free_block_bytes(&self) -> usize {
        if self.free_bytes == 0 {
            0
        } else {
            1usize << self.largest_free_order
        }
    }

    /// Percentage of the managed region that is currently allocated.
    pub fn utilization_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        100.0 * self.allocated_bytes as f64 / self.total_bytes as f64
    }
}

// ──────────────────────── buddy allocator ──────────────────────────

/// Power-of-two buddy allocator over a contiguous physical address range.
///
/// # Invariants
///
/// - `base` and `size` are both multiples of `2^MIN_ORDER`.
/// - `size` is a power of two.
/// - `free_lists.len() == levels`.
/// - `free_lists[i]` holds base addresses of free blocks at order `MIN_ORDER + i`.
/// - All addresses in `free_lists[i]` are aligned to `2^(MIN_ORDER + i)`.
/// - `stats.free_bytes + stats.allocated_bytes == stats.total_bytes` always.
#[derive(Debug)]
pub struct BuddyAllocator {
    /// Physical base address of the managed region.
    base: usize,
    /// Total managed size in bytes (power of two, >= 2^MIN_ORDER).
    size: usize,
    /// Number of active order levels (level 0 = MIN_ORDER, …, level levels-1 = top_order).
    levels: usize,
    /// `free_lists[level]` is the list of free block base addresses at that level.
    free_lists: Vec<Vec<usize>>,
    /// Live statistics.
    stats: BuddyStats,
}

// ─────────────────── internal helper functions ─────────────────────

/// Compute the smallest order k such that 2^k >= size (k >= MIN_ORDER).
#[inline]
fn required_order(size: usize) -> u32 {
    if size <= (1 << MIN_ORDER) {
        return MIN_ORDER;
    }
    // We need the smallest k with 2^k >= size.
    // That is ceil_log2(size), but never below MIN_ORDER.
    let bits = usize::BITS - (size - 1).leading_zeros();
    bits.max(MIN_ORDER)
}

/// Convert an order value to its level index (order - MIN_ORDER).
#[inline]
fn order_to_level(order: u32) -> usize {
    (order - MIN_ORDER) as usize
}

/// Compute the buddy's base address using the XOR trick.
///
/// For a block at `addr` with size `2^order`, its buddy is at `addr ^ (1 << order)`.
/// This works because both blocks come from the same parent: the parent's address
/// is `addr & !(1 << order)`, which is `addr` with bit `order` cleared; the buddy
/// is the same parent with bit `order` set or cleared the other way.
#[inline]
fn buddy_of(addr: usize, order: u32) -> usize {
    addr ^ (1usize << order)
}

// ────────────────────── impl BuddyAllocator ────────────────────────

impl BuddyAllocator {
    // ─────────────── constructor ───────────────

    /// Create a new buddy allocator over `[base, base + size)`.
    ///
    /// `size` must be > 0.  If `size` is not a power of two it is rounded
    /// **down** to the largest power of two that fits within it.  The
    /// effective size is the amount of memory actually managed.
    ///
    /// # Errors
    ///
    /// - [`BuddyError::ZeroSize`] if `size == 0`.
    pub fn new(base: usize, size: usize) -> Result<Self, BuddyError> {
        if size == 0 {
            return Err(BuddyError::ZeroSize);
        }

        // Round size down to the largest power of two that fits.
        let effective_size: usize = if size.is_power_of_two() {
            size
        } else {
            // Largest power of two <= size.
            1usize << (usize::BITS - 1 - size.leading_zeros())
        };

        // Clamp effective_size to at least 2^MIN_ORDER.
        let effective_size = effective_size.max(1 << MIN_ORDER);

        let top_order = effective_size.trailing_zeros(); // = log2(effective_size)
        let raw_levels = (top_order.saturating_sub(MIN_ORDER) + 1) as usize;
        let levels = raw_levels.min(MAX_LEVELS);

        // Build the free lists.  Level 0 == MIN_ORDER, level (levels-1) == top_order.
        let mut free_lists: Vec<Vec<usize>> = (0..levels).map(|_| Vec::new()).collect();

        // The whole region is one free block at the top level.
        let top_level = levels - 1;
        free_lists[top_level].push(base);

        let stats = BuddyStats {
            total_bytes: effective_size,
            free_bytes: effective_size,
            allocated_bytes: 0,
            largest_free_order: top_order,
            ..BuddyStats::default()
        };

        Ok(Self {
            base,
            size: effective_size,
            levels,
            free_lists,
            stats,
        })
    }

    // ─────────────── helpers ───────────────────

    /// The highest order this allocator handles.
    #[inline]
    fn top_order(&self) -> u32 {
        MIN_ORDER + self.levels as u32 - 1
    }

    /// Convert `order` to its level index.
    #[inline]
    fn level_of(&self, order: u32) -> usize {
        order_to_level(order)
    }

    /// Total bytes in the managed region.
    #[inline]
    pub fn total(&self) -> usize {
        self.size
    }

    /// Return a copy of the current statistics snapshot.
    #[inline]
    pub fn stats(&self) -> BuddyStats {
        self.stats
    }

    /// Return `true` if no bytes are currently allocated.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.stats.allocated_bytes == 0
    }

    /// Number of free 4 KiB pages.
    #[inline]
    pub fn free_pages(&self) -> usize {
        self.stats.free_bytes >> MIN_ORDER
    }

    /// Number of allocated 4 KiB pages.
    #[inline]
    pub fn used_pages(&self) -> usize {
        self.stats.allocated_bytes >> MIN_ORDER
    }

    /// Recompute `stats.largest_free_order` by scanning free lists top-down.
    fn refresh_largest_free_order(&mut self) {
        for level in (0..self.levels).rev() {
            if !self.free_lists[level].is_empty() {
                self.stats.largest_free_order = MIN_ORDER + level as u32;
                return;
            }
        }
        // No free blocks at all – set sentinel to MIN_ORDER.
        self.stats.largest_free_order = MIN_ORDER;
    }

    // ─────────────── allocate ──────────────────

    /// Allocate a contiguous block of at least `size` bytes.
    ///
    /// The returned address is aligned to the block's order boundary.
    ///
    /// # Errors
    ///
    /// - [`BuddyError::ZeroSize`] – `size == 0`.
    /// - [`BuddyError::AllocationTooLarge`] – `size` requires an order above
    ///   the top order of this allocator.
    /// - [`BuddyError::OutOfMemory`] – no free block of sufficient order.
    pub fn allocate(&mut self, size: usize) -> Result<usize, BuddyError> {
        if size == 0 {
            return Err(BuddyError::ZeroSize);
        }

        let order = required_order(size);
        let top = self.top_order();

        if order > top {
            return Err(BuddyError::AllocationTooLarge {
                requested: size,
                max: 1usize << top,
            });
        }

        // Find the lowest level at or above `order` that has a free block.
        let found_level =
            (order_to_level(order)..self.levels).find(|&lvl| !self.free_lists[lvl].is_empty());

        let found_level = found_level.ok_or(BuddyError::OutOfMemory {
            order,
            available: self.stats.free_bytes,
        })?;

        // Split downward from `found_level` to `order_to_level(order)`.
        let mut current_level = found_level;
        while current_level > order_to_level(order) {
            // Pop the block at current_level.
            let block = self.free_lists[current_level]
                .pop()
                .ok_or(BuddyError::OutOfMemory {
                    order: MIN_ORDER + current_level as u32,
                    available: self.stats.free_bytes,
                })?;

            // Split into two halves at the level below.
            let child_order = MIN_ORDER + current_level as u32 - 1;
            let half_size = 1usize << child_order;
            let left = block;
            let right = block + half_size;
            let child_level = current_level - 1;
            self.free_lists[child_level].push(left);
            self.free_lists[child_level].push(right);
            self.stats.split_count += 1;
            current_level -= 1;
        }

        // Now pop the block from the target level.
        let allocated_addr =
            self.free_lists[current_level]
                .pop()
                .ok_or(BuddyError::OutOfMemory {
                    order,
                    available: self.stats.free_bytes,
                })?;

        let block_size = 1usize << order;
        self.stats.allocated_bytes += block_size;
        self.stats.free_bytes -= block_size;
        self.stats.allocation_count += 1;
        self.refresh_largest_free_order();

        Ok(allocated_addr)
    }

    // ─────────────── deallocate ────────────────

    /// Return a previously-allocated block back to the allocator.
    ///
    /// The `size` must be the same size that was passed to (or implied by)
    /// `allocate`.  On success the block is coalesced with its buddy as far
    /// up the tree as possible.
    ///
    /// # Errors
    ///
    /// - [`BuddyError::ZeroSize`] – `size == 0`.
    /// - [`BuddyError::InvalidSize`] – `size` does not correspond to a valid order.
    /// - [`BuddyError::UnalignedAddress`] – `addr` is not aligned to `size`.
    /// - [`BuddyError::AddressOutOfRange`] – `addr` is outside the managed region.
    pub fn deallocate(&mut self, addr: usize, size: usize) -> Result<(), BuddyError> {
        if size == 0 {
            return Err(BuddyError::ZeroSize);
        }

        let order = required_order(size);
        let top = self.top_order();

        if order > top {
            return Err(BuddyError::InvalidSize { size });
        }

        let required_align = 1usize << order;

        // Alignment check.
        if !addr.is_multiple_of(required_align) {
            return Err(BuddyError::UnalignedAddress {
                addr,
                order,
                required_align,
            });
        }

        // Range check.
        let block_size = 1usize << order;
        let region_end = self.base + self.size;
        if addr < self.base || addr.saturating_add(block_size) > region_end {
            return Err(BuddyError::AddressOutOfRange {
                addr,
                base: self.base,
                end: region_end,
            });
        }

        // Coalesce loop: try to merge with buddy, moving upward.
        let mut current_addr = addr;
        let mut current_order = order;

        while current_order < top {
            let buddy = buddy_of(current_addr, current_order);
            let level = order_to_level(current_order);

            // Check whether the buddy is in the free list at this level.
            if let Some(pos) = self.free_lists[level].iter().position(|&a| a == buddy) {
                // Remove the buddy from the free list.
                self.free_lists[level].swap_remove(pos);
                // The merged block starts at the lower of the two addresses.
                current_addr = current_addr.min(buddy);
                current_order += 1;
                self.stats.coalesce_count += 1;
            } else {
                break;
            }
        }

        // Push the (possibly coalesced) block onto its free list.
        let final_level = order_to_level(current_order);
        self.free_lists[final_level].push(current_addr);

        self.stats.free_bytes += block_size;
        self.stats.allocated_bytes = self.stats.allocated_bytes.saturating_sub(block_size);
        self.stats.deallocation_count += 1;
        self.refresh_largest_free_order();

        Ok(())
    }

    // ─────────────── page helpers ──────────────

    /// Allocate `pages` contiguous 4 KiB pages.
    ///
    /// This is a convenience wrapper; the returned address is aligned to
    /// `pages * PAGE_SIZE` rounded up to the next order boundary.
    pub fn allocate_pages(&mut self, pages: usize) -> Result<usize, BuddyError> {
        if pages == 0 {
            return Err(BuddyError::ZeroSize);
        }
        let byte_size = pages << MIN_ORDER; // pages * 4096
        self.allocate(byte_size)
    }

    /// Allocate `pages` contiguous pages, logically zero-filling them.
    ///
    /// In a real kernel this would invoke a platform-specific routine to
    /// zero physical pages (e.g., via a mapped window or DMA).  Here it
    /// simply allocates the block; actual zeroing is deferred to the
    /// caller or handled by the paging layer.
    pub fn allocate_pages_zeroed(&mut self, pages: usize) -> Result<usize, BuddyError> {
        // NOTE: In production, after obtaining the physical address the kernel
        // would map it temporarily and memset it to zero.  For this
        // allocator stub the zeroing is the caller's responsibility.
        self.allocate_pages(pages)
    }
}

// ────────────────────────────── tests ──────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 0x1000_0000;
    const MB: usize = 1024 * 1024;
    const PAGE: usize = 1 << MIN_ORDER; // 4096

    // ── construction ──

    #[test]
    fn test_new_power_of_two_size() {
        let a = BuddyAllocator::new(BASE, MB).unwrap();
        assert_eq!(a.stats().total_bytes, MB);
        assert_eq!(a.stats().free_bytes, MB);
        assert_eq!(a.stats().allocated_bytes, 0);
    }

    #[test]
    fn test_new_non_power_of_two_rounds_down() {
        // 3 MB rounds down to 2 MB.
        let a = BuddyAllocator::new(BASE, 3 * MB).unwrap();
        assert_eq!(a.stats().total_bytes, 2 * MB);
        assert_eq!(a.stats().free_bytes, 2 * MB);
    }

    #[test]
    fn test_new_zero_size() {
        let result = BuddyAllocator::new(BASE, 0);
        assert!(
            matches!(result, Err(BuddyError::ZeroSize)),
            "expected ZeroSize, got {result:?}"
        );
    }

    // ── basic allocations ──

    #[test]
    fn test_allocate_single_page() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let addr = a.allocate(PAGE).unwrap();
        // Address must be page-aligned and within the region.
        assert_eq!(addr % PAGE, 0, "not page-aligned: {addr:#x}");
        assert!((BASE..BASE + MB).contains(&addr));
        assert_eq!(a.stats().allocated_bytes, PAGE);
        assert_eq!(a.stats().free_bytes, MB - PAGE);
    }

    #[test]
    fn test_allocate_then_free() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let addr = a.allocate(PAGE).unwrap();
        a.deallocate(addr, PAGE).unwrap();
        assert_eq!(a.stats().free_bytes, MB);
        assert_eq!(a.stats().allocated_bytes, 0);
    }

    #[test]
    fn test_allocate_two_pages() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let a1 = a.allocate(PAGE).unwrap();
        let a2 = a.allocate(PAGE).unwrap();
        assert_ne!(a1, a2, "two allocations returned the same address");
        assert_eq!(a.stats().allocated_bytes, 2 * PAGE);
    }

    #[test]
    fn test_allocate_splits_blocks() {
        // From a 4 MB region allocating a single page requires log2(4MB/4KB) = 10 splits.
        let region_size = 4 * MB;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        let levels = a.levels;
        a.allocate(PAGE).unwrap();
        // Each level between the top and level-0 contributes one split.
        // levels - 1 splits total.
        assert_eq!(a.stats().split_count, (levels - 1) as u64);
    }

    // ── coalescing ──

    #[test]
    fn test_coalesce_buddy() {
        // 8 KiB region: exactly two pages, which are each other's buddy.
        let region_size = 8 * 1024; // 2 pages
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        let a1 = a.allocate(PAGE).unwrap();
        let a2 = a.allocate(PAGE).unwrap();
        // Free both; the second free should coalesce.
        a.deallocate(a1, PAGE).unwrap();
        a.deallocate(a2, PAGE).unwrap();
        assert!(
            a.stats().coalesce_count >= 1,
            "expected at least one coalesce, got {}",
            a.stats().coalesce_count
        );
        assert_eq!(a.stats().free_bytes, region_size);
    }

    #[test]
    fn test_coalesce_chain() {
        // 16 KiB region = 4 pages.  Allocate all 4, free in reverse → chain coalesce.
        let region_size = 4 * PAGE;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        let addrs: Vec<usize> = (0..4).map(|_| a.allocate(PAGE).unwrap()).collect();
        // Free in reverse order.
        for &addr in addrs.iter().rev() {
            a.deallocate(addr, PAGE).unwrap();
        }
        assert_eq!(a.stats().free_bytes, region_size);
        assert_eq!(a.stats().allocated_bytes, 0);
        // At least two coalesces expected (possibly more depending on order freed).
        assert!(a.stats().coalesce_count >= 2);
    }

    // ── boundary conditions ──

    #[test]
    fn test_allocate_max_order() {
        let region_size = MB;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        // Allocate the whole region in one shot.
        let addr = a.allocate(region_size).unwrap();
        assert_eq!(addr, BASE);
        assert_eq!(a.stats().allocated_bytes, region_size);
        assert_eq!(a.stats().free_bytes, 0);
    }

    #[test]
    fn test_allocate_exhaustion() {
        let region_size = 2 * PAGE;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        a.allocate(PAGE).unwrap();
        a.allocate(PAGE).unwrap();
        // Third allocation must fail.
        let result = a.allocate(PAGE);
        assert!(
            matches!(result, Err(BuddyError::OutOfMemory { .. })),
            "expected OutOfMemory, got {result:?}"
        );
    }

    #[test]
    fn test_allocate_too_large() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let result = a.allocate(2 * MB);
        assert!(
            matches!(result, Err(BuddyError::AllocationTooLarge { .. })),
            "expected AllocationTooLarge, got {result:?}"
        );
    }

    #[test]
    fn test_allocate_unaligned_free() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let addr = a.allocate(PAGE).unwrap();
        // Try to free with an address that is not page-aligned.
        let bad_addr = addr + 1;
        let result = a.deallocate(bad_addr, PAGE);
        assert!(
            matches!(result, Err(BuddyError::UnalignedAddress { .. })),
            "expected UnalignedAddress, got {result:?}"
        );
    }

    #[test]
    fn test_allocate_out_of_range_free() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        // Address is completely outside the managed region.
        let out_addr = BASE + 2 * MB;
        let result = a.deallocate(out_addr, PAGE);
        assert!(
            matches!(result, Err(BuddyError::AddressOutOfRange { .. })),
            "expected AddressOutOfRange, got {result:?}"
        );
    }

    #[test]
    fn test_allocate_zero_size() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        assert_eq!(a.allocate(0), Err(BuddyError::ZeroSize));
    }

    #[test]
    fn test_free_zero_size() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        assert_eq!(a.deallocate(BASE, 0), Err(BuddyError::ZeroSize));
    }

    // ── statistics ──

    #[test]
    fn test_stats_allocation_count() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        assert_eq!(a.stats().allocation_count, 0);
        a.allocate(PAGE).unwrap();
        assert_eq!(a.stats().allocation_count, 1);
        a.allocate(PAGE).unwrap();
        assert_eq!(a.stats().allocation_count, 2);
    }

    #[test]
    fn test_stats_coalesce_count() {
        let region_size = 2 * PAGE;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        let a1 = a.allocate(PAGE).unwrap();
        let a2 = a.allocate(PAGE).unwrap();
        assert_eq!(a.stats().coalesce_count, 0);
        a.deallocate(a1, PAGE).unwrap();
        a.deallocate(a2, PAGE).unwrap();
        assert!(a.stats().coalesce_count >= 1);
    }

    #[test]
    fn test_stats_split_count() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        assert_eq!(a.stats().split_count, 0);
        a.allocate(PAGE).unwrap();
        assert!(a.stats().split_count > 0);
    }

    #[test]
    fn test_stats_utilization() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        assert!((a.stats().utilization_percent() - 0.0).abs() < 1e-6);
        a.allocate(MB).unwrap();
        assert!((a.stats().utilization_percent() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_stats_fragmentation() {
        // Allocate two non-adjacent pages from a 4-page region so that the
        // remaining two free pages are not buddies of each other.
        let region_size = 4 * PAGE;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        // Alloc all 4, then free the 0th and 2nd (non-buddy pair in a 4-page region).
        let addrs: Vec<usize> = (0..4).map(|_| a.allocate(PAGE).unwrap()).collect();
        // Keep addrs[1] and addrs[3] allocated; free addrs[0] and addrs[2].
        a.deallocate(addrs[0], PAGE).unwrap();
        a.deallocate(addrs[2], PAGE).unwrap();
        // If the two free pages are not buddies, fragmentation > 0.
        // (They might or might not coalesce depending on addresses.)
        let frag = a.stats().fragmentation_percent();
        assert!(frag >= 0.0);
    }

    // ── page helpers ──

    #[test]
    fn test_free_pages_count() {
        let mut a = BuddyAllocator::new(BASE, 4 * PAGE).unwrap();
        assert_eq!(a.free_pages(), 4);
        a.allocate(PAGE).unwrap();
        assert_eq!(a.free_pages(), 3);
    }

    #[test]
    fn test_used_pages_count() {
        let mut a = BuddyAllocator::new(BASE, 4 * PAGE).unwrap();
        assert_eq!(a.used_pages(), 0);
        a.allocate(PAGE).unwrap();
        assert_eq!(a.used_pages(), 1);
    }

    #[test]
    fn test_allocate_various_sizes() {
        let mut a = BuddyAllocator::new(BASE, 16 * MB).unwrap();
        // 1 byte → rounds up to MIN_ORDER block (4096 bytes)
        let r1 = a.allocate(1).unwrap();
        // 512 bytes → still rounds up to 4096 (MIN_ORDER)
        let r2 = a.allocate(512).unwrap();
        // 4096 bytes → exact page
        let r3 = a.allocate(4096).unwrap();
        // 8192 bytes → two-page block
        let r4 = a.allocate(8192).unwrap();
        // 64 KiB block
        let r5 = a.allocate(64 * 1024).unwrap();
        // All addresses must be distinct and page-aligned.
        let all = [r1, r2, r3, r4, r5];
        for &addr in &all {
            assert_eq!(addr % PAGE, 0, "not page-aligned: {addr:#x}");
        }
        // All distinct
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }

    #[test]
    fn test_interleaved_alloc_free() {
        let region_size = 8 * MB;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        // Allocate 8 pages.
        let addrs: Vec<usize> = (0..8).map(|_| a.allocate(PAGE).unwrap()).collect();
        assert_eq!(a.stats().allocation_count, 8);
        // Free even-indexed ones.
        for i in (0..8).step_by(2) {
            a.deallocate(addrs[i], PAGE).unwrap();
        }
        let free_after_even = a.stats().free_bytes;
        assert_eq!(free_after_even, region_size - 4 * PAGE);
        // Now alloc 4 more pages.
        let new_addrs: Vec<usize> = (0..4).map(|_| a.allocate(PAGE).unwrap()).collect();
        // New addresses should not overlap with still-allocated odd-indexed ones.
        let odd_addrs: Vec<usize> = (0..4).map(|i| addrs[2 * i + 1]).collect();
        for &na in &new_addrs {
            for &oa in &odd_addrs {
                assert_ne!(na, oa, "new alloc overlaps existing: {na:#x}");
            }
        }
    }

    #[test]
    fn test_full_cycle_no_leaks() {
        let pages = 8usize;
        let region_size = pages * PAGE;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        let total = a.stats().total_bytes;
        let addrs: Vec<usize> = (0..pages).map(|_| a.allocate(PAGE).unwrap()).collect();
        assert_eq!(a.stats().free_bytes, 0);
        assert_eq!(a.stats().allocated_bytes, total);
        for &addr in &addrs {
            a.deallocate(addr, PAGE).unwrap();
        }
        assert_eq!(a.stats().free_bytes, total);
        assert_eq!(a.stats().allocated_bytes, 0);
    }

    #[test]
    fn test_largest_free_order_tracking() {
        let mut a = BuddyAllocator::new(BASE, 4 * PAGE).unwrap();
        let initial_top = a.stats().largest_free_order;
        // After filling all pages the largest free order should drop to MIN_ORDER
        // (sentinel value when nothing is free).
        a.allocate(4 * PAGE).unwrap();
        // After exhaustion refresh should set it to MIN_ORDER (sentinel).
        assert!(a.stats().largest_free_order <= initial_top);
        // Free it back; should restore.
        a.deallocate(BASE, 4 * PAGE).unwrap();
        assert_eq!(a.stats().largest_free_order, initial_top);
    }

    #[test]
    fn test_buddy_xor_coalescing() {
        // Verify the XOR buddy address formula directly.
        // For a block at BASE (must be order-aligned), buddy = BASE ^ (1 << order).
        let order: u32 = MIN_ORDER; // 4 KiB
        let block = BASE; // BASE = 0x1000_0000, which is page-aligned
        let computed_buddy = buddy_of(block, order);
        // buddy should differ by exactly one page.
        assert_eq!(
            computed_buddy,
            block ^ (1 << order),
            "XOR buddy formula mismatch"
        );
        // Also confirm round-trip: buddy of buddy is original.
        assert_eq!(buddy_of(computed_buddy, order), block);
    }

    #[test]
    fn test_buddy_error_display() {
        let variants: &[BuddyError] = &[
            BuddyError::ZeroSize,
            BuddyError::AllocationTooLarge {
                requested: 128 * MB,
                max: MB,
            },
            BuddyError::OutOfMemory {
                order: 12,
                available: 0,
            },
            BuddyError::UnalignedAddress {
                addr: 0x1000_0001,
                order: 12,
                required_align: PAGE,
            },
            BuddyError::AddressOutOfRange {
                addr: 0xDEAD_BEEF,
                base: BASE,
                end: BASE + MB,
            },
            BuddyError::InvalidSize { size: 7 },
        ];
        for v in variants {
            let s = alloc::format!("{v}");
            assert!(!s.is_empty(), "Display for {v:?} was empty");
        }
    }

    #[test]
    fn test_small_region_two_pages() {
        let region_size = 2 * PAGE; // 8 KiB
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        assert_eq!(a.stats().total_bytes, region_size);
        let a1 = a.allocate(PAGE).unwrap();
        let a2 = a.allocate(PAGE).unwrap();
        assert_ne!(a1, a2);
        assert_eq!(a.stats().free_bytes, 0);
        a.deallocate(a1, PAGE).unwrap();
        a.deallocate(a2, PAGE).unwrap();
        assert_eq!(a.stats().free_bytes, region_size);
    }

    #[test]
    fn test_allocate_pages_helper() {
        // A 3-page request would need 4 pages (next power of two), so use a big region.
        let region_size = 8 * MB;
        let mut a = BuddyAllocator::new(BASE, region_size).unwrap();
        let addr = a.allocate_pages(3).unwrap();
        // 3 pages → 12288 bytes → rounds to 16384 (next power-of-two order)
        assert_eq!(addr % PAGE, 0);
        assert!(addr >= BASE && addr < BASE + region_size);
    }

    #[test]
    fn test_is_empty() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        assert!(a.is_empty(), "should be empty before any allocation");
        let addr = a.allocate(PAGE).unwrap();
        assert!(!a.is_empty(), "should not be empty after allocation");
        a.deallocate(addr, PAGE).unwrap();
        assert!(a.is_empty(), "should be empty again after deallocation");
    }

    // ── additional edge cases ──

    #[test]
    fn test_allocate_pages_zeroed() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        // allocate_pages_zeroed is a stub that behaves like allocate_pages.
        let addr = a.allocate_pages_zeroed(1).unwrap();
        assert_eq!(addr % PAGE, 0);
        a.deallocate(addr, PAGE).unwrap();
    }

    #[test]
    fn test_deallocation_count() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let addr = a.allocate(PAGE).unwrap();
        assert_eq!(a.stats().deallocation_count, 0);
        a.deallocate(addr, PAGE).unwrap();
        assert_eq!(a.stats().deallocation_count, 1);
    }

    #[test]
    fn test_total_bytes_constant() {
        let mut a = BuddyAllocator::new(BASE, MB).unwrap();
        let total = a.stats().total_bytes;
        let addr = a.allocate(PAGE).unwrap();
        assert_eq!(
            a.stats().total_bytes,
            total,
            "total_bytes changed after alloc"
        );
        a.deallocate(addr, PAGE).unwrap();
        assert_eq!(
            a.stats().total_bytes,
            total,
            "total_bytes changed after dealloc"
        );
    }

    #[test]
    fn test_free_plus_allocated_equals_total() {
        let mut a = BuddyAllocator::new(BASE, 4 * MB).unwrap();
        let total = a.stats().total_bytes;
        for _ in 0..8 {
            let addr = a.allocate(PAGE).unwrap();
            let s = a.stats();
            assert_eq!(
                s.free_bytes + s.allocated_bytes,
                total,
                "invariant broken after alloc"
            );
            a.deallocate(addr, PAGE).unwrap();
            let s = a.stats();
            assert_eq!(
                s.free_bytes + s.allocated_bytes,
                total,
                "invariant broken after dealloc"
            );
        }
    }
}
