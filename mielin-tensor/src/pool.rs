//! Tensor Memory Pool
//!
//! Provides aligned memory allocation and reuse for tensor buffers.
//! Optimized for cache efficiency and SIMD operations.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Alignment for SIMD operations (64 bytes for AVX-512)
pub const SIMD_ALIGNMENT: usize = 64;

/// Minimum block size for pooling (256 bytes)
pub const MIN_BLOCK_SIZE: usize = 256;

/// Maximum pooled block size (16 MB)
pub const MAX_POOLED_SIZE: usize = 16 * 1024 * 1024;

/// Memory allocation statistics
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Total allocations performed
    pub total_allocations: AtomicU64,
    /// Total deallocations performed
    pub total_deallocations: AtomicU64,
    /// Current bytes allocated
    pub current_bytes: AtomicUsize,
    /// Peak bytes allocated
    pub peak_bytes: AtomicUsize,
    /// Cache hits (reused blocks)
    pub cache_hits: AtomicU64,
    /// Cache misses (new allocations)
    pub cache_misses: AtomicU64,
    /// Total bytes saved by reuse
    pub bytes_saved: AtomicU64,
}

impl PoolStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an allocation
    pub fn record_allocation(&self, size: usize, cache_hit: bool) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
        let old = self.current_bytes.fetch_add(size, Ordering::Relaxed);
        let new = old + size;

        // Update peak
        let mut peak = self.peak_bytes.load(Ordering::Relaxed);
        while new > peak {
            match self.peak_bytes.compare_exchange_weak(
                peak,
                new,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }

        if cache_hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            self.bytes_saved.fetch_add(size as u64, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a deallocation
    pub fn record_deallocation(&self, size: usize) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
        self.current_bytes.fetch_sub(size, Ordering::Relaxed);
    }

    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get current memory usage
    pub fn current_usage(&self) -> usize {
        self.current_bytes.load(Ordering::Relaxed)
    }

    /// Get peak memory usage
    pub fn peak_usage(&self) -> usize {
        self.peak_bytes.load(Ordering::Relaxed)
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.total_allocations.store(0, Ordering::Relaxed);
        self.total_deallocations.store(0, Ordering::Relaxed);
        self.current_bytes.store(0, Ordering::Relaxed);
        self.peak_bytes.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.bytes_saved.store(0, Ordering::Relaxed);
    }
}

impl Clone for PoolStats {
    fn clone(&self) -> Self {
        Self {
            total_allocations: AtomicU64::new(self.total_allocations.load(Ordering::Relaxed)),
            total_deallocations: AtomicU64::new(self.total_deallocations.load(Ordering::Relaxed)),
            current_bytes: AtomicUsize::new(self.current_bytes.load(Ordering::Relaxed)),
            peak_bytes: AtomicUsize::new(self.peak_bytes.load(Ordering::Relaxed)),
            cache_hits: AtomicU64::new(self.cache_hits.load(Ordering::Relaxed)),
            cache_misses: AtomicU64::new(self.cache_misses.load(Ordering::Relaxed)),
            bytes_saved: AtomicU64::new(self.bytes_saved.load(Ordering::Relaxed)),
        }
    }
}

/// A pooled memory buffer
#[derive(Debug)]
pub struct PooledBuffer {
    /// Pointer to the allocated memory
    ptr: NonNull<u8>,
    /// Actual allocated size (may be larger than requested)
    capacity: usize,
    /// Requested size
    size: usize,
    /// Layout used for allocation
    layout: Layout,
    /// Flag to indicate if the buffer should be freed on drop
    owned: bool,
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if self.owned {
            unsafe {
                alloc::alloc::dealloc(self.ptr.as_ptr(), self.layout);
            }
        }
    }
}

impl PooledBuffer {
    /// Create a new pooled buffer
    ///
    /// # Safety
    /// The ptr must be valid and allocated with the given layout
    unsafe fn new(ptr: NonNull<u8>, capacity: usize, size: usize, layout: Layout) -> Self {
        Self {
            ptr,
            capacity,
            size,
            layout,
            owned: true,
        }
    }

    /// Mark the buffer as not owned (won't be freed on drop)
    fn release_ownership(&mut self) {
        self.owned = false;
    }

    /// Get the pointer to the buffer
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Get a mutable pointer to the buffer
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Get a slice view of the buffer
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.size) }
    }

    /// Get a mutable slice view of the buffer
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size) }
    }

    /// Get as f32 slice
    ///
    /// # Panics
    /// Panics if size is not aligned to f32
    pub fn as_f32_slice(&self) -> &[f32] {
        assert!(
            self.size.is_multiple_of(4),
            "Buffer size not aligned to f32"
        );
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr() as *const f32, self.size / 4) }
    }

    /// Get as mutable f32 slice
    ///
    /// # Panics
    /// Panics if size is not aligned to f32
    pub fn as_f32_mut_slice(&mut self) -> &mut [f32] {
        assert!(
            self.size.is_multiple_of(4),
            "Buffer size not aligned to f32"
        );
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr() as *mut f32, self.size / 4) }
    }

    /// Get the size of the buffer
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the capacity of the buffer
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the layout used for allocation
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Zero out the buffer
    pub fn zero(&mut self) {
        unsafe {
            core::ptr::write_bytes(self.ptr.as_ptr(), 0, self.size);
        }
    }

    /// Fill the buffer with a value
    pub fn fill(&mut self, value: u8) {
        unsafe {
            core::ptr::write_bytes(self.ptr.as_ptr(), value, self.size);
        }
    }
}

/// Size class for pooled buffers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SizeClass(usize);

impl SizeClass {
    /// Get the size class for a given size
    pub fn for_size(size: usize) -> Self {
        if size <= MIN_BLOCK_SIZE {
            SizeClass(MIN_BLOCK_SIZE)
        } else {
            // Round up to next power of 2
            let power = size.next_power_of_two();
            SizeClass(power.max(MIN_BLOCK_SIZE))
        }
    }

    /// Get the actual size of this class
    pub fn size(&self) -> usize {
        self.0
    }
}

/// Free block in the pool
struct FreeBlock {
    ptr: NonNull<u8>,
    layout: Layout,
}

/// Tensor memory pool
///
/// Provides efficient allocation and reuse of tensor buffers
pub struct TensorPool {
    /// Free blocks organized by size class
    free_lists: UnsafeCell<BTreeMap<SizeClass, Vec<FreeBlock>>>,
    /// Pool statistics
    stats: PoolStats,
    /// Maximum bytes to keep in pool
    max_cached_bytes: usize,
    /// Current cached bytes
    cached_bytes: AtomicUsize,
}

impl TensorPool {
    /// Create a new tensor pool with default settings
    pub fn new() -> Self {
        Self::with_max_cached(64 * 1024 * 1024) // 64 MB default
    }

    /// Create a new tensor pool with custom max cached bytes
    pub fn with_max_cached(max_cached_bytes: usize) -> Self {
        Self {
            free_lists: UnsafeCell::new(BTreeMap::new()),
            stats: PoolStats::new(),
            max_cached_bytes,
            cached_bytes: AtomicUsize::new(0),
        }
    }

    /// Allocate a buffer of the given size
    pub fn allocate(&self, size: usize) -> Option<PooledBuffer> {
        if size == 0 {
            return None;
        }

        let size_class = SizeClass::for_size(size);
        let actual_size = size_class.size();

        // Try to get a cached block
        let cached = unsafe {
            let free_lists = &mut *self.free_lists.get();
            free_lists.get_mut(&size_class).and_then(|list| list.pop())
        };

        if let Some(block) = cached {
            self.cached_bytes.fetch_sub(actual_size, Ordering::Relaxed);
            self.stats.record_allocation(size, true);

            return Some(unsafe { PooledBuffer::new(block.ptr, actual_size, size, block.layout) });
        }

        // Allocate new block
        let layout = Layout::from_size_align(actual_size, SIMD_ALIGNMENT).ok()?;
        let ptr = unsafe {
            let ptr = alloc::alloc::alloc(layout);
            if ptr.is_null() {
                return None;
            }
            NonNull::new_unchecked(ptr)
        };

        self.stats.record_allocation(size, false);

        Some(unsafe { PooledBuffer::new(ptr, actual_size, size, layout) })
    }

    /// Allocate and zero-initialize a buffer
    pub fn allocate_zeroed(&self, size: usize) -> Option<PooledBuffer> {
        let mut buffer = self.allocate(size)?;
        buffer.zero();
        Some(buffer)
    }

    /// Deallocate a buffer back to the pool
    ///
    /// # Safety
    /// The buffer must have been allocated from this pool
    pub unsafe fn deallocate(&self, mut buffer: PooledBuffer) {
        let size = buffer.capacity;
        self.stats.record_deallocation(buffer.size);

        // Check if we should cache this block
        let current_cached = self.cached_bytes.load(Ordering::Relaxed);
        if current_cached + size > self.max_cached_bytes || size > MAX_POOLED_SIZE {
            // Don't cache, just free
            alloc::alloc::dealloc(buffer.ptr.as_ptr(), buffer.layout);
            return;
        }

        // Cache the block
        let size_class = SizeClass(size);
        let block = FreeBlock {
            ptr: buffer.ptr,
            layout: buffer.layout,
        };

        let free_lists = &mut *self.free_lists.get();
        free_lists.entry(size_class).or_default().push(block);
        self.cached_bytes.fetch_add(size, Ordering::Relaxed);

        // Mark buffer as not owned so drop doesn't free it
        buffer.release_ownership();
    }

    /// Clear all cached blocks
    pub fn clear_cache(&self) {
        unsafe {
            let free_lists = &mut *self.free_lists.get();
            for (_, blocks) in free_lists.iter_mut() {
                for block in blocks.drain(..) {
                    alloc::alloc::dealloc(block.ptr.as_ptr(), block.layout);
                }
            }
            free_lists.clear();
        }
        self.cached_bytes.store(0, Ordering::Relaxed);
    }

    /// Get pool statistics
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Get current cached bytes
    pub fn cached_bytes(&self) -> usize {
        self.cached_bytes.load(Ordering::Relaxed)
    }

    /// Get maximum cached bytes
    pub fn max_cached_bytes(&self) -> usize {
        self.max_cached_bytes
    }

    /// Get number of cached blocks
    pub fn cached_blocks(&self) -> usize {
        unsafe {
            let free_lists = &*self.free_lists.get();
            free_lists.values().map(|v| v.len()).sum()
        }
    }

    /// Trim cache to a target size
    pub fn trim_to(&self, target_bytes: usize) {
        unsafe {
            let free_lists = &mut *self.free_lists.get();
            let mut current = self.cached_bytes.load(Ordering::Relaxed);

            // Remove from largest size classes first
            let size_classes: Vec<_> = free_lists.keys().copied().collect();
            for size_class in size_classes.into_iter().rev() {
                if current <= target_bytes {
                    break;
                }

                if let Some(blocks) = free_lists.get_mut(&size_class) {
                    while current > target_bytes && !blocks.is_empty() {
                        if let Some(block) = blocks.pop() {
                            alloc::alloc::dealloc(block.ptr.as_ptr(), block.layout);
                            current -= size_class.size();
                        }
                    }
                }
            }

            self.cached_bytes.store(current, Ordering::Relaxed);
        }
    }
}

impl Default for TensorPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TensorPool {
    fn drop(&mut self) {
        self.clear_cache();
    }
}

// Safety: TensorPool uses interior mutability safely
// The free_lists are only accessed through &self methods
// which provide proper synchronization for single-threaded use
unsafe impl Send for TensorPool {}

use core::sync::atomic::AtomicBool;

/// Global pool storage using lazy initialization
struct GlobalPool {
    pool: UnsafeCell<Option<TensorPool>>,
    initialized: AtomicBool,
}

// Safety: We only access pool through atomic-guarded methods
unsafe impl Sync for GlobalPool {}

impl GlobalPool {
    const fn new() -> Self {
        Self {
            pool: UnsafeCell::new(None),
            initialized: AtomicBool::new(false),
        }
    }

    fn get_or_init(&self) -> &TensorPool {
        if !self.initialized.load(Ordering::Acquire) {
            // Try to initialize
            if !self.initialized.swap(true, Ordering::AcqRel) {
                // We won the race to initialize
                unsafe {
                    *self.pool.get() = Some(TensorPool::new());
                }
            } else {
                // Another thread is initializing, spin until done
                while unsafe { (*self.pool.get()).is_none() } {
                    core::hint::spin_loop();
                }
            }
        }
        unsafe { (*self.pool.get()).as_ref().expect("pool was initialized above before this access") }
    }

    fn init_with_max(&self, max_cached_bytes: usize) {
        if !self.initialized.swap(true, Ordering::AcqRel) {
            unsafe {
                *self.pool.get() = Some(TensorPool::with_max_cached(max_cached_bytes));
            }
        }
    }
}

static GLOBAL_POOL: GlobalPool = GlobalPool::new();

/// Initialize the global tensor pool
pub fn init_global_pool() {
    let _ = GLOBAL_POOL.get_or_init();
}

/// Initialize the global pool with custom max cached bytes
pub fn init_global_pool_with_max(max_cached_bytes: usize) {
    GLOBAL_POOL.init_with_max(max_cached_bytes);
}

/// Get the global pool
pub fn global_pool() -> &'static TensorPool {
    GLOBAL_POOL.get_or_init()
}

/// Allocate from the global pool
pub fn allocate(size: usize) -> Option<PooledBuffer> {
    global_pool().allocate(size)
}

/// Allocate and zero from the global pool
pub fn allocate_zeroed(size: usize) -> Option<PooledBuffer> {
    global_pool().allocate_zeroed(size)
}

/// Deallocate to the global pool
///
/// # Safety
/// The buffer must have been allocated from the global pool
pub unsafe fn deallocate(buffer: PooledBuffer) {
    global_pool().deallocate(buffer);
}

/// Get global pool statistics
pub fn stats() -> &'static PoolStats {
    global_pool().stats()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_class() {
        assert_eq!(SizeClass::for_size(1).size(), MIN_BLOCK_SIZE);
        assert_eq!(SizeClass::for_size(256).size(), 256);
        assert_eq!(SizeClass::for_size(257).size(), 512);
        assert_eq!(SizeClass::for_size(1000).size(), 1024);
        assert_eq!(SizeClass::for_size(1024).size(), 1024);
        assert_eq!(SizeClass::for_size(1025).size(), 2048);
    }

    #[test]
    fn test_pool_allocation() {
        let pool = TensorPool::new();

        let buffer = pool.allocate(100).unwrap();
        assert!(buffer.capacity() >= 100);
        assert_eq!(buffer.size(), 100);

        unsafe {
            pool.deallocate(buffer);
        }
    }

    #[test]
    fn test_pool_reuse() {
        let pool = TensorPool::new();

        // Allocate and deallocate
        let buffer1 = pool.allocate(100).unwrap();
        let ptr1 = buffer1.as_ptr();
        unsafe {
            pool.deallocate(buffer1);
        }

        // Second allocation should reuse the block
        let buffer2 = pool.allocate(100).unwrap();
        let ptr2 = buffer2.as_ptr();

        assert_eq!(ptr1, ptr2, "Should reuse the same block");

        let stats = pool.stats();
        assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 1);

        unsafe {
            pool.deallocate(buffer2);
        }
    }

    #[test]
    fn test_pool_zeroed() {
        let pool = TensorPool::new();

        let buffer = pool.allocate_zeroed(100).unwrap();
        for &b in buffer.as_slice() {
            assert_eq!(b, 0);
        }

        unsafe {
            pool.deallocate(buffer);
        }
    }

    #[test]
    fn test_pool_fill() {
        let pool = TensorPool::new();

        let mut buffer = pool.allocate(100).unwrap();
        buffer.fill(0xAB);

        for &b in buffer.as_slice() {
            assert_eq!(b, 0xAB);
        }

        unsafe {
            pool.deallocate(buffer);
        }
    }

    #[test]
    fn test_pool_f32_slice() {
        let pool = TensorPool::new();

        let mut buffer = pool.allocate(16).unwrap(); // 4 floats
        let slice = buffer.as_f32_mut_slice();
        slice[0] = 1.0;
        slice[1] = 2.0;
        slice[2] = 3.0;
        slice[3] = 4.0;

        let read_slice = buffer.as_f32_slice();
        assert_eq!(read_slice, &[1.0, 2.0, 3.0, 4.0]);

        unsafe {
            pool.deallocate(buffer);
        }
    }

    #[test]
    fn test_pool_stats() {
        let pool = TensorPool::new();

        let buffer1 = pool.allocate(100).unwrap();
        assert_eq!(pool.stats().total_allocations.load(Ordering::Relaxed), 1);
        assert_eq!(pool.stats().cache_misses.load(Ordering::Relaxed), 1);

        unsafe {
            pool.deallocate(buffer1);
        }
        assert_eq!(pool.stats().total_deallocations.load(Ordering::Relaxed), 1);

        let _buffer2 = pool.allocate(100).unwrap();
        assert_eq!(pool.stats().cache_hits.load(Ordering::Relaxed), 1);
        assert!(pool.stats().hit_rate() > 0.0);
    }

    #[test]
    fn test_pool_clear_cache() {
        let pool = TensorPool::new();

        let buffer = pool.allocate(1024).unwrap();
        unsafe {
            pool.deallocate(buffer);
        }

        assert!(pool.cached_bytes() > 0);

        pool.clear_cache();
        assert_eq!(pool.cached_bytes(), 0);
        assert_eq!(pool.cached_blocks(), 0);
    }

    #[test]
    fn test_pool_trim() {
        let pool = TensorPool::with_max_cached(1024 * 1024);

        // Allocate and deallocate buffers of different sizes
        let sizes = [256, 512, 1024, 2048, 4096, 8192, 16384, 32768];
        for &size in &sizes {
            let buffer = pool.allocate(size).unwrap();
            unsafe {
                pool.deallocate(buffer);
            }
        }

        let before = pool.cached_bytes();
        assert!(before > 4096, "Should have cached more than 4096 bytes");

        pool.trim_to(4096);
        let after = pool.cached_bytes();

        assert!(after <= 4096);
        assert!(after < before);
    }

    #[test]
    fn test_pool_different_sizes() {
        let pool = TensorPool::new();

        let sizes = [64, 256, 512, 1024, 4096, 16384];
        let mut buffers = Vec::new();

        for &size in &sizes {
            let buffer = pool.allocate(size).unwrap();
            assert!(buffer.capacity() >= size);
            buffers.push(buffer);
        }

        // Deallocate all
        for buffer in buffers {
            unsafe {
                pool.deallocate(buffer);
            }
        }

        // Should have multiple size classes cached
        assert!(pool.cached_blocks() >= sizes.len());
    }

    #[test]
    fn test_pool_stats_hit_rate() {
        let stats = PoolStats::new();
        assert_eq!(stats.hit_rate(), 0.0);

        stats.record_allocation(100, true);
        stats.record_allocation(100, false);
        assert_eq!(stats.hit_rate(), 0.5);

        stats.record_allocation(100, true);
        assert!(stats.hit_rate() > 0.6);
    }

    #[test]
    fn test_pool_stats_peak() {
        let stats = PoolStats::new();

        stats.record_allocation(1000, false);
        assert_eq!(stats.peak_usage(), 1000);

        stats.record_allocation(500, false);
        assert_eq!(stats.peak_usage(), 1500);

        stats.record_deallocation(500);
        assert_eq!(stats.current_usage(), 1000);
        assert_eq!(stats.peak_usage(), 1500); // Peak unchanged
    }

    #[test]
    fn test_pool_stats_reset() {
        let stats = PoolStats::new();

        stats.record_allocation(100, true);
        stats.record_deallocation(100);

        stats.reset();

        assert_eq!(stats.total_allocations.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_deallocations.load(Ordering::Relaxed), 0);
        assert_eq!(stats.cache_hits.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_buffer_capacity() {
        let pool = TensorPool::new();

        // Request 100 bytes, should get at least MIN_BLOCK_SIZE
        let buffer = pool.allocate(100).unwrap();
        assert!(buffer.capacity() >= MIN_BLOCK_SIZE);
        assert_eq!(buffer.size(), 100);

        unsafe {
            pool.deallocate(buffer);
        }
    }

    #[test]
    fn test_zero_size_allocation() {
        let pool = TensorPool::new();
        assert!(pool.allocate(0).is_none());
    }

    #[test]
    fn test_global_pool() {
        init_global_pool();

        let buffer = allocate(256).unwrap();
        assert!(buffer.capacity() >= 256);

        unsafe {
            deallocate(buffer);
        }

        let s = stats();
        assert!(s.total_allocations.load(Ordering::Relaxed) > 0);
    }
}
