//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use core::sync::atomic::{AtomicUsize, Ordering};

use super::functions::POOL_SIZES;

/// Statistics for a single pool
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolStats {
    /// Total blocks in this pool
    pub total_blocks: usize,
    /// Currently allocated blocks
    pub allocated_blocks: usize,
    /// Peak allocated blocks
    pub peak_allocated: usize,
    /// Total allocations performed
    pub allocation_count: u64,
    /// Total deallocations performed
    pub deallocation_count: u64,
    /// Allocation failures (pool exhausted)
    pub allocation_failures: u64,
}
impl PoolStats {
    /// Get free blocks count
    pub fn free_blocks(&self) -> usize {
        self.total_blocks.saturating_sub(self.allocated_blocks)
    }
    /// Get utilization percentage
    pub fn utilization_percent(&self) -> u8 {
        if self.total_blocks == 0 {
            return 0;
        }
        ((self.allocated_blocks * 100) / self.total_blocks).min(100) as u8
    }
    /// Get peak utilization percentage
    pub fn peak_utilization_percent(&self) -> u8 {
        if self.total_blocks == 0 {
            return 0;
        }
        ((self.peak_allocated * 100) / self.total_blocks).min(100) as u8
    }
}
/// Atomic statistics for thread-safe access
#[allow(dead_code)]
struct PoolStatsAtomic {
    allocated_blocks: AtomicUsize,
    peak_allocated: AtomicUsize,
    allocation_count: AtomicUsize,
    deallocation_count: AtomicUsize,
    allocation_failures: AtomicUsize,
}
#[allow(dead_code)]
impl PoolStatsAtomic {
    fn new() -> Self {
        Self {
            allocated_blocks: AtomicUsize::new(0),
            peak_allocated: AtomicUsize::new(0),
            allocation_count: AtomicUsize::new(0),
            deallocation_count: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
        }
    }
    fn record_allocation(&self) {
        let alloc = self.allocated_blocks.fetch_add(1, Ordering::Relaxed) + 1;
        self.allocation_count.fetch_add(1, Ordering::Relaxed);
        let _ = self.peak_allocated.fetch_max(alloc, Ordering::Relaxed);
    }
    fn record_deallocation(&self) {
        self.allocated_blocks.fetch_sub(1, Ordering::Relaxed);
        self.deallocation_count.fetch_add(1, Ordering::Relaxed);
    }
    fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }
    fn to_stats(&self, total_blocks: usize) -> PoolStats {
        PoolStats {
            total_blocks,
            allocated_blocks: self.allocated_blocks.load(Ordering::Relaxed),
            peak_allocated: self.peak_allocated.load(Ordering::Relaxed),
            allocation_count: self.allocation_count.load(Ordering::Relaxed) as u64,
            deallocation_count: self.deallocation_count.load(Ordering::Relaxed) as u64,
            allocation_failures: self.allocation_failures.load(Ordering::Relaxed) as u64,
        }
    }
}
/// Block state in the free list
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockState {
    Free,
    Allocated,
}
/// Pool configuration
#[derive(Debug, Clone, Copy)]
pub struct PoolConfig {
    /// Number of blocks per pool for each size
    pub blocks_per_pool: [usize; 7],
    /// Enable statistics tracking
    pub track_statistics: bool,
    /// Enable fragmentation monitoring
    pub track_fragmentation: bool,
    /// High fragmentation threshold (percentage)
    pub fragmentation_threshold: u8,
}
impl PoolConfig {
    /// Configuration for minimal embedded systems
    pub const fn minimal() -> Self {
        Self {
            blocks_per_pool: [16, 16, 8, 4, 2, 1, 1],
            track_statistics: false,
            track_fragmentation: false,
            fragmentation_threshold: 75,
        }
    }
    /// Configuration for standard embedded systems
    pub const fn standard() -> Self {
        Self {
            blocks_per_pool: [32, 32, 32, 16, 8, 4, 2],
            track_statistics: true,
            track_fragmentation: true,
            fragmentation_threshold: 50,
        }
    }
    /// Configuration for resource-rich systems
    pub const fn generous() -> Self {
        Self {
            blocks_per_pool: [128, 128, 64, 32, 16, 8, 4],
            track_statistics: true,
            track_fragmentation: true,
            fragmentation_threshold: 30,
        }
    }
    /// Configuration optimized for small MCUs (< 32KB RAM)
    pub const fn tiny() -> Self {
        Self {
            blocks_per_pool: [8, 8, 4, 2, 1, 0, 0],
            track_statistics: false,
            track_fragmentation: false,
            fragmentation_threshold: 80,
        }
    }
    /// Configuration for ultra-low power systems
    pub const fn ultra_low_power() -> Self {
        Self {
            blocks_per_pool: [12, 12, 6, 3, 2, 1, 0],
            track_statistics: false,
            track_fragmentation: false,
            fragmentation_threshold: 75,
        }
    }
    /// Configuration with custom block counts per pool size
    ///
    /// # Arguments
    /// * `counts` - Array of block counts for each pool size [16B, 32B, 64B, 128B, 256B, 512B, 1KB]
    pub const fn custom(counts: [usize; 7]) -> Self {
        Self {
            blocks_per_pool: counts,
            track_statistics: true,
            track_fragmentation: true,
            fragmentation_threshold: 50,
        }
    }
    /// Set the number of blocks for a specific pool size
    ///
    /// # Arguments
    /// * `pool_index` - Index of the pool (0-6)
    /// * `count` - Number of blocks
    pub fn with_pool_blocks(mut self, pool_index: usize, count: usize) -> Self {
        if pool_index < POOL_SIZES.len() {
            self.blocks_per_pool[pool_index] = count;
        }
        self
    }
    /// Set the number of blocks for the 16B pool
    pub const fn with_16b_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[0] = count;
        self
    }
    /// Set the number of blocks for the 32B pool
    pub const fn with_32b_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[1] = count;
        self
    }
    /// Set the number of blocks for the 64B pool
    pub const fn with_64b_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[2] = count;
        self
    }
    /// Set the number of blocks for the 128B pool
    pub const fn with_128b_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[3] = count;
        self
    }
    /// Set the number of blocks for the 256B pool
    pub const fn with_256b_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[4] = count;
        self
    }
    /// Set the number of blocks for the 512B pool
    pub const fn with_512b_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[5] = count;
        self
    }
    /// Set the number of blocks for the 1KB pool
    pub const fn with_1kb_blocks(mut self, count: usize) -> Self {
        self.blocks_per_pool[6] = count;
        self
    }
    /// Enable or disable statistics tracking
    pub const fn with_statistics(mut self, enabled: bool) -> Self {
        self.track_statistics = enabled;
        self
    }
    /// Enable or disable fragmentation tracking
    pub const fn with_fragmentation_tracking(mut self, enabled: bool) -> Self {
        self.track_fragmentation = enabled;
        self
    }
    /// Set the fragmentation threshold percentage
    pub const fn with_fragmentation_threshold(mut self, threshold: u8) -> Self {
        self.fragmentation_threshold = threshold;
        self
    }
    /// Scale all pool sizes by a factor
    ///
    /// Useful for adjusting memory budget while maintaining proportions
    pub fn scale_by(mut self, factor: f32) -> Self {
        for count in &mut self.blocks_per_pool {
            *count = (*count as f32 * factor + 0.5) as usize;
        }
        self
    }
    /// Limit total memory usage to approximately the given bytes
    ///
    /// Scales down pool sizes proportionally to fit within memory budget
    pub fn limit_to_bytes(mut self, max_bytes: usize) -> Self {
        let current_total = self.total_memory();
        if current_total > max_bytes {
            let scale_factor = max_bytes as f32 / current_total as f32;
            self = self.scale_by(scale_factor);
        }
        self
    }
    /// Configure for a specific total RAM budget
    ///
    /// Creates a balanced configuration optimized for the given RAM size
    pub fn for_ram_size(ram_bytes: usize) -> Self {
        let pool_budget = ram_bytes / 2;
        if pool_budget < 2048 {
            Self::tiny()
        } else if pool_budget < 8192 {
            Self::minimal().limit_to_bytes(pool_budget)
        } else if pool_budget < 32768 {
            Self::standard().limit_to_bytes(pool_budget)
        } else {
            Self::generous().limit_to_bytes(pool_budget)
        }
    }
    /// Calculate total memory required for all pools
    pub fn total_memory(&self) -> usize {
        let mut total = 0;
        for (i, &count) in self.blocks_per_pool.iter().enumerate() {
            total += POOL_SIZES[i] * count;
        }
        total
    }
    /// Get memory usage breakdown by pool size
    pub fn memory_breakdown(&self) -> [(usize, usize, usize); 7] {
        let mut breakdown = [(0, 0, 0); 7];
        for (i, &count) in self.blocks_per_pool.iter().enumerate() {
            breakdown[i] = (POOL_SIZES[i], count, POOL_SIZES[i] * count);
        }
        breakdown
    }
    /// Validate configuration
    pub fn validate(&self) -> Result<(), PoolConfigError> {
        if self.blocks_per_pool.iter().all(|&count| count == 0) {
            return Err(PoolConfigError::NoPoolsConfigured);
        }
        if self.fragmentation_threshold > 100 {
            return Err(PoolConfigError::InvalidThreshold);
        }
        Ok(())
    }
}
/// Multi-size pool allocator
pub struct PoolAllocator {
    config: PoolConfig,
    /// Statistics per pool
    pool_stats: [PoolStats; 7],
    /// Allocated counts (atomic for thread safety)
    allocated: [AtomicUsize; 7],
    /// Peak allocated
    peak: [AtomicUsize; 7],
    /// Allocation counts
    alloc_counts: [AtomicUsize; 7],
    /// Deallocation counts
    dealloc_counts: [AtomicUsize; 7],
    /// Failure counts
    failure_counts: [AtomicUsize; 7],
    /// Simulated free list heads (index into backing storage)
    free_heads: [AtomicUsize; 7],
    /// Track fragmentation state per pool
    fragmentation: [AtomicUsize; 7],
}
impl PoolAllocator {
    /// Create a new pool allocator with the given configuration
    pub fn new(config: PoolConfig) -> Self {
        let empty = || AtomicUsize::new(0);
        let max_head = || AtomicUsize::new(usize::MAX);
        Self {
            config,
            pool_stats: [PoolStats::default(); 7],
            allocated: [
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
            ],
            peak: [
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
            ],
            alloc_counts: [
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
            ],
            dealloc_counts: [
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
            ],
            failure_counts: [
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
            ],
            free_heads: [
                max_head(),
                max_head(),
                max_head(),
                max_head(),
                max_head(),
                max_head(),
                max_head(),
            ],
            fragmentation: [
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
                empty(),
            ],
        }
    }
    /// Initialize the allocator (must be called before allocations)
    pub fn init(&mut self) {
        for (i, &count) in self.config.blocks_per_pool.iter().enumerate() {
            self.pool_stats[i].total_blocks = count;
            if count > 0 {
                self.free_heads[i].store(0, Ordering::Release);
            }
        }
    }
    /// Find the appropriate pool index for a given size
    fn pool_index_for_size(&self, size: usize) -> Option<usize> {
        for (i, &pool_size) in POOL_SIZES.iter().enumerate() {
            if size <= pool_size {
                return Some(i);
            }
        }
        None
    }
    /// Validate allocation request
    fn validate_allocation_request(&self, size: usize) -> Result<(), PoolError> {
        if size == 0 {
            return Err(PoolError::ZeroSizeAllocation);
        }
        if size > POOL_SIZES[POOL_SIZES.len() - 1] {
            return Err(PoolError::SizeTooLarge);
        }
        Ok(())
    }
    /// Validate allocation structure
    pub(crate) fn validate_allocation(&self, alloc: &Allocation) -> Result<(), PoolError> {
        if alloc.pool_index >= POOL_SIZES.len() {
            return Err(PoolError::InvalidPool);
        }
        if alloc.block_index >= self.config.blocks_per_pool[alloc.pool_index] {
            return Err(PoolError::InvalidBlockIndex);
        }
        if alloc.size != POOL_SIZES[alloc.pool_index] {
            return Err(PoolError::InvalidAllocation);
        }
        Ok(())
    }
    /// Check pool integrity
    pub fn check_integrity(&self) -> Result<(), PoolError> {
        for (i, &total) in self.config.blocks_per_pool.iter().enumerate() {
            let allocated = self.allocated[i].load(Ordering::Relaxed);
            if allocated > total {
                return Err(PoolError::MemoryCorruption);
            }
            let allocs = self.alloc_counts[i].load(Ordering::Relaxed);
            let deallocs = self.dealloc_counts[i].load(Ordering::Relaxed);
            if deallocs > allocs {
                return Err(PoolError::MemoryCorruption);
            }
        }
        Ok(())
    }
    /// Allocate memory of the given size with safety checks
    ///
    /// Returns an allocation result with pool index and block index.
    /// In a real implementation, this would return a pointer.
    pub fn allocate(&self, size: usize) -> Result<Allocation, PoolError> {
        self.validate_allocation_request(size)?;
        let pool_idx = self
            .pool_index_for_size(size)
            .ok_or(PoolError::SizeTooLarge)?;
        let total = self.config.blocks_per_pool[pool_idx];
        let allocated = self.allocated[pool_idx].load(Ordering::Acquire);
        if allocated >= total {
            self.failure_counts[pool_idx].fetch_add(1, Ordering::Relaxed);
            return Err(PoolError::PoolExhausted);
        }
        let block_idx = self.allocated[pool_idx].fetch_add(1, Ordering::AcqRel);
        if block_idx >= total {
            self.allocated[pool_idx].fetch_sub(1, Ordering::AcqRel);
            self.failure_counts[pool_idx].fetch_add(1, Ordering::Relaxed);
            return Err(PoolError::PoolExhausted);
        }
        self.alloc_counts[pool_idx].fetch_add(1, Ordering::Relaxed);
        let _ = self.peak[pool_idx].fetch_max(block_idx + 1, Ordering::Relaxed);
        self.update_fragmentation(pool_idx);
        Ok(Allocation {
            pool_index: pool_idx,
            block_index: block_idx,
            size: POOL_SIZES[pool_idx],
        })
    }
    /// Deallocate memory with comprehensive safety checks
    pub fn deallocate(&self, alloc: &Allocation) -> Result<(), PoolError> {
        self.validate_allocation(alloc)?;
        let pool_idx = alloc.pool_index;
        let allocated = self.allocated[pool_idx].load(Ordering::Acquire);
        if allocated == 0 {
            return Err(PoolError::DoubleFree);
        }
        let alloc_count = self.alloc_counts[pool_idx].load(Ordering::Relaxed);
        let dealloc_count = self.dealloc_counts[pool_idx].load(Ordering::Relaxed);
        if dealloc_count >= alloc_count {
            return Err(PoolError::DoubleFree);
        }
        self.allocated[pool_idx].fetch_sub(1, Ordering::AcqRel);
        self.dealloc_counts[pool_idx].fetch_add(1, Ordering::Relaxed);
        self.update_fragmentation(pool_idx);
        Ok(())
    }
    /// Safe deallocate - validates and deallocates, returning detailed error
    pub fn safe_deallocate(&self, alloc: &Allocation) -> Result<(), PoolError> {
        self.check_integrity()?;
        let result = self.deallocate(alloc);
        if result.is_ok() {
            self.check_integrity()?;
        }
        result
    }
    /// Update fragmentation score for a pool
    fn update_fragmentation(&self, pool_idx: usize) {
        let total = self.config.blocks_per_pool[pool_idx];
        let allocated = self.allocated[pool_idx].load(Ordering::Relaxed);
        if total == 0 || allocated == 0 {
            self.fragmentation[pool_idx].store(0, Ordering::Relaxed);
            return;
        }
        let utilization = (allocated * 100) / total;
        let score = if utilization > 0 && utilization < 100 {
            let diff_from_half = utilization.abs_diff(50);
            50 - diff_from_half.min(50)
        } else {
            0
        };
        self.fragmentation[pool_idx].store(score, Ordering::Relaxed);
    }
    /// Get statistics for a specific pool
    pub fn pool_stats(&self, pool_idx: usize) -> Option<PoolStats> {
        if pool_idx >= POOL_SIZES.len() {
            return None;
        }
        Some(PoolStats {
            total_blocks: self.config.blocks_per_pool[pool_idx],
            allocated_blocks: self.allocated[pool_idx].load(Ordering::Relaxed),
            peak_allocated: self.peak[pool_idx].load(Ordering::Relaxed),
            allocation_count: self.alloc_counts[pool_idx].load(Ordering::Relaxed) as u64,
            deallocation_count: self.dealloc_counts[pool_idx].load(Ordering::Relaxed) as u64,
            allocation_failures: self.failure_counts[pool_idx].load(Ordering::Relaxed) as u64,
        })
    }
    /// Get statistics for all pools
    pub fn all_stats(&self) -> [PoolStats; 7] {
        let mut stats = [PoolStats::default(); 7];
        for (i, stat) in stats.iter_mut().enumerate() {
            *stat = self.pool_stats(i).unwrap_or_default();
        }
        stats
    }
    /// Get aggregate statistics across all pools
    pub fn aggregate_stats(&self) -> AggregatePoolStats {
        let mut total_blocks = 0usize;
        let mut allocated_blocks = 0usize;
        let mut total_bytes = 0usize;
        let mut allocated_bytes = 0usize;
        let mut allocation_count = 0u64;
        let mut deallocation_count = 0u64;
        let mut failure_count = 0u64;
        for (i, &pool_size) in POOL_SIZES.iter().enumerate() {
            let stats = self.pool_stats(i).unwrap_or_default();
            total_blocks += stats.total_blocks;
            allocated_blocks += stats.allocated_blocks;
            total_bytes += stats.total_blocks * pool_size;
            allocated_bytes += stats.allocated_blocks * pool_size;
            allocation_count += stats.allocation_count;
            deallocation_count += stats.deallocation_count;
            failure_count += stats.allocation_failures;
        }
        AggregatePoolStats {
            total_blocks,
            allocated_blocks,
            total_bytes,
            allocated_bytes,
            allocation_count,
            deallocation_count,
            failure_count,
        }
    }
    /// Get fragmentation info for a pool
    pub fn fragmentation_info(&self, pool_idx: usize) -> Option<FragmentationInfo> {
        if pool_idx >= POOL_SIZES.len() {
            return None;
        }
        let total = self.config.blocks_per_pool[pool_idx];
        let allocated = self.allocated[pool_idx].load(Ordering::Relaxed);
        let free = total.saturating_sub(allocated);
        let score = self.fragmentation[pool_idx].load(Ordering::Relaxed);
        Some(FragmentationInfo {
            free_blocks: free,
            free_regions: if free > 0 { 1 } else { 0 },
            largest_free_region: free,
            fragmentation_score: score.min(100) as u8,
        })
    }
    /// Check if any pool has high fragmentation
    pub fn has_high_fragmentation(&self) -> bool {
        for i in 0..7 {
            let score = self.fragmentation[i].load(Ordering::Relaxed);
            if score >= self.config.fragmentation_threshold as usize {
                return true;
            }
        }
        false
    }
    /// Get total available memory across all pools
    pub fn available_memory(&self) -> usize {
        let mut available = 0;
        for (i, &pool_size) in POOL_SIZES.iter().enumerate() {
            let total = self.config.blocks_per_pool[i];
            let allocated = self.allocated[i].load(Ordering::Relaxed);
            let free = total.saturating_sub(allocated);
            available += free * pool_size;
        }
        available
    }
    /// Get the pool size for a given pool index
    pub fn pool_size(&self, pool_idx: usize) -> Option<usize> {
        POOL_SIZES.get(pool_idx).copied()
    }
    /// Reset all statistics
    pub fn reset_stats(&self) {
        for i in 0..7 {
            self.peak[i].store(self.allocated[i].load(Ordering::Relaxed), Ordering::Relaxed);
            self.alloc_counts[i].store(0, Ordering::Relaxed);
            self.dealloc_counts[i].store(0, Ordering::Relaxed);
            self.failure_counts[i].store(0, Ordering::Relaxed);
        }
    }
}
/// Allocation result
#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    /// Pool index (0-6)
    pub pool_index: usize,
    /// Block index within pool
    pub block_index: usize,
    /// Actual block size allocated
    pub size: usize,
}
impl Allocation {
    /// Get the internal fragmentation (wasted bytes)
    pub fn internal_fragmentation(&self, requested_size: usize) -> usize {
        self.size.saturating_sub(requested_size)
    }
}
/// Aggregate statistics across all pools
#[derive(Debug, Clone, Copy, Default)]
pub struct AggregatePoolStats {
    /// Total blocks across all pools
    pub total_blocks: usize,
    /// Total allocated blocks
    pub allocated_blocks: usize,
    /// Total bytes of memory managed
    pub total_bytes: usize,
    /// Total bytes currently allocated
    pub allocated_bytes: usize,
    /// Total allocations performed
    pub allocation_count: u64,
    /// Total deallocations performed
    pub deallocation_count: u64,
    /// Total allocation failures
    pub failure_count: u64,
}
impl AggregatePoolStats {
    /// Get block utilization percentage
    pub fn block_utilization(&self) -> u8 {
        if self.total_blocks == 0 {
            return 0;
        }
        ((self.allocated_blocks * 100) / self.total_blocks).min(100) as u8
    }
    /// Get byte utilization percentage
    pub fn byte_utilization(&self) -> u8 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.allocated_bytes * 100) / self.total_bytes).min(100) as u8
    }
    /// Get allocation success rate
    pub fn success_rate(&self) -> f32 {
        let total_attempts = self.allocation_count + self.failure_count;
        if total_attempts == 0 {
            return 100.0;
        }
        (self.allocation_count as f32 / total_attempts as f32) * 100.0
    }
}
/// Fragmentation analysis result
#[derive(Debug, Clone, Copy)]
pub struct FragmentationInfo {
    /// Number of free blocks
    pub free_blocks: usize,
    /// Number of contiguous free regions
    pub free_regions: usize,
    /// Largest contiguous free region (in blocks)
    pub largest_free_region: usize,
    /// Fragmentation score (0-100, higher = more fragmented)
    pub fragmentation_score: u8,
}
impl FragmentationInfo {
    /// Check if fragmentation is high
    pub fn is_high(&self, threshold: u8) -> bool {
        self.fragmentation_score >= threshold
    }
}
/// Pool size selector - helps choose optimal pool
pub struct PoolSizeSelector;
impl PoolSizeSelector {
    /// Get the smallest pool size that can fit the requested size
    pub fn select(size: usize) -> Option<usize> {
        POOL_SIZES
            .iter()
            .find(|&&pool_size| size <= pool_size)
            .copied()
    }
    /// Get the pool index for a size
    pub fn index_for(size: usize) -> Option<usize> {
        for (i, &pool_size) in POOL_SIZES.iter().enumerate() {
            if size <= pool_size {
                return Some(i);
            }
        }
        None
    }
    /// Calculate internal fragmentation for a size
    pub fn internal_fragmentation(size: usize) -> Option<usize> {
        Self::select(size).map(|pool_size| pool_size - size)
    }
    /// Find the best-fit pool (smallest that fits)
    pub fn best_fit(size: usize) -> Option<(usize, usize)> {
        Self::index_for(size).map(|idx| (idx, POOL_SIZES[idx]))
    }
}
/// A single memory pool for a specific block size
#[allow(dead_code)]
pub struct Pool {
    /// Block size in bytes
    block_size: usize,
    /// Pool index
    pool_index: usize,
    /// Total number of blocks
    total_blocks: usize,
    /// Free list head index (usize::MAX means empty)
    free_head: AtomicUsize,
    /// Block states (for fragmentation tracking)
    block_states: &'static mut [BlockState],
    /// Next pointers for free list (stored in block data or separately)
    next_pointers: &'static mut [usize],
    /// Statistics
    stats: PoolStatsAtomic,
}
/// Pool configuration errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolConfigError {
    /// No pools have any blocks configured
    NoPoolsConfigured,
    /// Fragmentation threshold is invalid (must be 0-100)
    InvalidThreshold,
}
/// Pool allocation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolError {
    /// Requested size is too large for any pool
    SizeTooLarge,
    /// Pool is exhausted (no free blocks)
    PoolExhausted,
    /// Invalid pool index
    InvalidPool,
    /// Double free detected
    DoubleFree,
    /// Pool not initialized
    NotInitialized,
    /// Invalid block index
    InvalidBlockIndex,
    /// Alignment requirements not met
    InvalidAlignment,
    /// Zero-size allocation requested
    ZeroSizeAllocation,
    /// Memory corruption detected
    MemoryCorruption,
    /// Invalid allocation (not from this allocator)
    InvalidAllocation,
}
