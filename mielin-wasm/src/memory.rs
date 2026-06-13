//! WASM Memory Management
//!
//! Provides memory limits enforcement, growth tracking, and memory isolation
//! for WASM module execution.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Memory limits configuration for WASM execution
#[derive(Debug, Clone)]
pub struct MemoryLimits {
    /// Initial memory pages (1 page = 64KB)
    pub initial_pages: u32,
    /// Maximum memory pages (None = unlimited up to WASM spec max)
    pub max_pages: Option<u32>,
    /// Maximum total memory in bytes (for additional enforcement)
    pub max_bytes: Option<usize>,
    /// Whether to allow memory growth beyond initial pages
    pub allow_growth: bool,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            initial_pages: 16,    // 1 MB initial
            max_pages: Some(256), // 16 MB max
            max_bytes: None,
            allow_growth: true,
        }
    }
}

impl MemoryLimits {
    /// Create limits for embedded/constrained environments
    pub fn embedded() -> Self {
        Self {
            initial_pages: 4,    // 256 KB
            max_pages: Some(16), // 1 MB max
            max_bytes: Some(1024 * 1024),
            allow_growth: true,
        }
    }

    /// Create limits for standard agents
    pub fn standard() -> Self {
        Self::default()
    }

    /// Create limits for compute-intensive agents
    pub fn compute() -> Self {
        Self {
            initial_pages: 64,     // 4 MB initial
            max_pages: Some(1024), // 64 MB max
            max_bytes: Some(64 * 1024 * 1024),
            allow_growth: true,
        }
    }

    /// Create unlimited limits (use with caution)
    pub fn unlimited() -> Self {
        Self {
            initial_pages: 16,
            max_pages: None,
            max_bytes: None,
            allow_growth: true,
        }
    }

    /// Calculate maximum bytes from page limit
    pub fn max_bytes_from_pages(&self) -> Option<usize> {
        self.max_pages.map(|p| p as usize * 65536)
    }

    /// Check if a requested size would exceed limits
    pub fn would_exceed(&self, current_bytes: usize, additional_bytes: usize) -> bool {
        if let Some(max) = self.max_bytes {
            current_bytes.saturating_add(additional_bytes) > max
        } else if let Some(max_pages) = self.max_pages {
            let max_bytes = max_pages as usize * 65536;
            current_bytes.saturating_add(additional_bytes) > max_bytes
        } else {
            false
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Default)]
pub struct MemoryStats {
    /// Current allocated memory in bytes
    current_bytes: AtomicU64,
    /// Peak memory usage in bytes
    peak_bytes: AtomicU64,
    /// Number of memory growth operations
    growth_count: AtomicU64,
    /// Total bytes allocated over lifetime
    total_allocated: AtomicU64,
    /// Number of allocation failures due to limits
    allocation_failures: AtomicU64,
}

impl MemoryStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a memory allocation
    pub fn record_allocation(&self, bytes: u64) {
        let old = self.current_bytes.fetch_add(bytes, Ordering::Relaxed);
        let new = old + bytes;

        // Update peak if necessary
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

        self.total_allocated.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record a memory deallocation
    pub fn record_deallocation(&self, bytes: u64) {
        self.current_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    /// Record a memory growth operation
    pub fn record_growth(&self, added_pages: u32) {
        self.growth_count.fetch_add(1, Ordering::Relaxed);
        self.record_allocation(added_pages as u64 * 65536);
    }

    /// Record an allocation failure
    pub fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current memory usage
    pub fn current_bytes(&self) -> u64 {
        self.current_bytes.load(Ordering::Relaxed)
    }

    /// Get peak memory usage
    pub fn peak_bytes(&self) -> u64 {
        self.peak_bytes.load(Ordering::Relaxed)
    }

    /// Get number of growth operations
    pub fn growth_count(&self) -> u64 {
        self.growth_count.load(Ordering::Relaxed)
    }

    /// Get total bytes allocated over lifetime
    pub fn total_allocated(&self) -> u64 {
        self.total_allocated.load(Ordering::Relaxed)
    }

    /// Get number of allocation failures
    pub fn allocation_failures(&self) -> u64 {
        self.allocation_failures.load(Ordering::Relaxed)
    }

    /// Reset statistics
    pub fn reset(&self) {
        self.current_bytes.store(0, Ordering::Relaxed);
        self.peak_bytes.store(0, Ordering::Relaxed);
        self.growth_count.store(0, Ordering::Relaxed);
        self.total_allocated.store(0, Ordering::Relaxed);
        self.allocation_failures.store(0, Ordering::Relaxed);
    }
}

impl Clone for MemoryStats {
    fn clone(&self) -> Self {
        Self {
            current_bytes: AtomicU64::new(self.current_bytes.load(Ordering::Relaxed)),
            peak_bytes: AtomicU64::new(self.peak_bytes.load(Ordering::Relaxed)),
            growth_count: AtomicU64::new(self.growth_count.load(Ordering::Relaxed)),
            total_allocated: AtomicU64::new(self.total_allocated.load(Ordering::Relaxed)),
            allocation_failures: AtomicU64::new(self.allocation_failures.load(Ordering::Relaxed)),
        }
    }
}

/// Memory growth callback type
pub type GrowthCallback = Arc<dyn Fn(u32, u32) -> bool + Send + Sync>;

/// Memory manager for a WASM instance
pub struct MemoryManager {
    limits: MemoryLimits,
    stats: Arc<MemoryStats>,
    growth_callback: Option<GrowthCallback>,
}

impl MemoryManager {
    pub fn new(limits: MemoryLimits) -> Self {
        Self {
            limits,
            stats: Arc::new(MemoryStats::new()),
            growth_callback: None,
        }
    }

    /// Create with default limits
    pub fn with_defaults() -> Self {
        Self::new(MemoryLimits::default())
    }

    /// Set a callback for memory growth events
    /// Returns true to allow growth, false to deny
    pub fn on_growth(&mut self, callback: impl Fn(u32, u32) -> bool + Send + Sync + 'static) {
        self.growth_callback = Some(Arc::new(callback));
    }

    /// Check if memory growth is allowed
    pub fn can_grow(&self, current_pages: u32, requested_pages: u32) -> bool {
        if !self.limits.allow_growth {
            return false;
        }

        let new_pages = current_pages + requested_pages;

        // Check against page limit
        if let Some(max_pages) = self.limits.max_pages {
            if new_pages > max_pages {
                self.stats.record_failure();
                return false;
            }
        }

        // Check against byte limit
        let new_bytes = new_pages as usize * 65536;
        if let Some(max_bytes) = self.limits.max_bytes {
            if new_bytes > max_bytes {
                self.stats.record_failure();
                return false;
            }
        }

        // Invoke callback if set
        if let Some(ref callback) = self.growth_callback {
            if !callback(current_pages, requested_pages) {
                self.stats.record_failure();
                return false;
            }
        }

        true
    }

    /// Record successful memory growth
    pub fn record_growth(&self, pages: u32) {
        self.stats.record_growth(pages);
    }

    /// Get current memory limits
    pub fn limits(&self) -> &MemoryLimits {
        &self.limits
    }

    /// Get memory statistics
    pub fn stats(&self) -> &MemoryStats {
        &self.stats
    }

    /// Get shared stats reference
    pub fn stats_arc(&self) -> Arc<MemoryStats> {
        Arc::clone(&self.stats)
    }

    /// Initialize with starting memory size
    pub fn initialize(&self, initial_bytes: u64) {
        self.stats.record_allocation(initial_bytes);
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl Clone for MemoryManager {
    fn clone(&self) -> Self {
        Self {
            limits: self.limits.clone(),
            stats: Arc::new((*self.stats).clone()),
            growth_callback: self.growth_callback.clone(),
        }
    }
}

/// Memory snapshot for migration
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Raw memory data
    pub data: Vec<u8>,
    /// Number of pages at time of snapshot
    pub pages: u32,
    /// Checksum for integrity verification
    pub checksum: u64,
}

impl MemorySnapshot {
    /// Create a snapshot from raw memory data
    pub fn new(data: Vec<u8>, pages: u32) -> Self {
        let checksum = Self::compute_checksum(&data);
        Self {
            data,
            pages,
            checksum,
        }
    }

    /// Compute a simple checksum (FNV-1a hash)
    pub(crate) fn compute_checksum(data: &[u8]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET;
        for &byte in data {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// Verify snapshot integrity
    pub fn verify(&self) -> bool {
        Self::compute_checksum(&self.data) == self.checksum
    }

    /// Get size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Create a compressed snapshot using LZ4.
    ///
    /// Format (13-byte header):
    /// - Bytes 0..4  : pages (u32 LE)
    /// - Bytes 4..12 : checksum (u64 LE)
    /// - Byte 12     : format tag (1 = LZ4, 0 = raw fallback)
    /// - Bytes 13..  : compressed or raw payload
    pub fn compress(&self) -> Vec<u8> {
        let (tag, payload) = match oxiarc_lz4::compress(&self.data) {
            Ok(compressed) => (1u8, compressed),
            Err(_) => (0u8, self.data.clone()),
        };
        let mut result = Vec::with_capacity(13 + payload.len());
        result.extend_from_slice(&self.pages.to_le_bytes());
        result.extend_from_slice(&self.checksum.to_le_bytes());
        result.push(tag);
        result.extend_from_slice(&payload);
        result
    }

    /// Decompress a snapshot produced by [`Self::compress`].
    pub fn decompress(data: &[u8]) -> Option<Self> {
        if data.len() < 13 {
            return None;
        }

        let pages = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let checksum = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);
        let tag = data[12];
        let payload = &data[13..];

        let memory_data = match tag {
            1 => oxiarc_lz4::decompress(payload, usize::MAX).ok()?,
            0 => payload.to_vec(),
            _ => return None,
        };

        let snapshot = Self {
            data: memory_data,
            pages,
            checksum,
        };

        if snapshot.verify() {
            Some(snapshot)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_limits_default() {
        let limits = MemoryLimits::default();
        assert_eq!(limits.initial_pages, 16);
        assert_eq!(limits.max_pages, Some(256));
        assert!(limits.allow_growth);
    }

    #[test]
    fn test_memory_limits_embedded() {
        let limits = MemoryLimits::embedded();
        assert_eq!(limits.initial_pages, 4);
        assert_eq!(limits.max_pages, Some(16));
    }

    #[test]
    fn test_memory_limits_compute() {
        let limits = MemoryLimits::compute();
        assert_eq!(limits.initial_pages, 64);
        assert_eq!(limits.max_pages, Some(1024));
    }

    #[test]
    fn test_memory_limits_would_exceed() {
        let limits = MemoryLimits {
            max_bytes: Some(1024 * 1024), // 1 MB
            ..Default::default()
        };

        assert!(!limits.would_exceed(512 * 1024, 256 * 1024));
        assert!(limits.would_exceed(900 * 1024, 200 * 1024));
    }

    #[test]
    fn test_memory_stats() {
        let stats = MemoryStats::new();

        stats.record_allocation(1000);
        assert_eq!(stats.current_bytes(), 1000);
        assert_eq!(stats.peak_bytes(), 1000);
        assert_eq!(stats.total_allocated(), 1000);

        stats.record_allocation(500);
        assert_eq!(stats.current_bytes(), 1500);
        assert_eq!(stats.peak_bytes(), 1500);

        stats.record_deallocation(800);
        assert_eq!(stats.current_bytes(), 700);
        assert_eq!(stats.peak_bytes(), 1500); // Peak unchanged
    }

    #[test]
    fn test_memory_stats_growth() {
        let stats = MemoryStats::new();

        stats.record_growth(2); // 2 pages = 128KB
        assert_eq!(stats.growth_count(), 1);
        assert_eq!(stats.current_bytes(), 2 * 65536);
    }

    #[test]
    fn test_memory_manager_can_grow() {
        let manager = MemoryManager::new(MemoryLimits {
            max_pages: Some(10),
            allow_growth: true,
            ..Default::default()
        });

        assert!(manager.can_grow(5, 3)); // 5 + 3 = 8 <= 10
        assert!(!manager.can_grow(5, 6)); // 5 + 6 = 11 > 10
        assert_eq!(manager.stats().allocation_failures(), 1);
    }

    #[test]
    fn test_memory_manager_growth_disabled() {
        let manager = MemoryManager::new(MemoryLimits {
            allow_growth: false,
            ..Default::default()
        });

        assert!(!manager.can_grow(1, 1));
    }

    #[test]
    fn test_memory_manager_growth_callback() {
        let mut manager = MemoryManager::new(MemoryLimits::default());

        // Only allow growth if adding less than 5 pages
        manager.on_growth(|_, requested| requested < 5);

        assert!(manager.can_grow(10, 3));
        assert!(!manager.can_grow(10, 5));
    }

    #[test]
    fn test_memory_snapshot_integrity() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let snapshot = MemorySnapshot::new(data.clone(), 1);

        assert!(snapshot.verify());
        assert_eq!(snapshot.size(), 10);
    }

    #[test]
    fn test_memory_snapshot_compress_decompress() {
        let data = vec![1, 2, 3, 4, 5];
        let snapshot = MemorySnapshot::new(data.clone(), 1);

        let compressed = snapshot.compress();
        let decompressed = MemorySnapshot::decompress(&compressed).unwrap();

        assert_eq!(decompressed.data, data);
        assert_eq!(decompressed.pages, 1);
        assert!(decompressed.verify());
    }

    #[test]
    fn test_memory_snapshot_invalid_decompress() {
        let result = MemorySnapshot::decompress(&[0, 1, 2]); // Too short
        assert!(result.is_none());

        // Invalid checksum
        let mut bad_data = vec![0u8; 20];
        bad_data[11] = 0xFF; // Corrupt checksum
        let result = MemorySnapshot::decompress(&bad_data);
        assert!(result.is_none());
    }

    #[test]
    fn test_memory_stats_reset() {
        let stats = MemoryStats::new();
        stats.record_allocation(1000);
        stats.record_growth(1);

        stats.reset();

        assert_eq!(stats.current_bytes(), 0);
        assert_eq!(stats.peak_bytes(), 0);
        assert_eq!(stats.growth_count(), 0);
    }

    #[test]
    fn test_memory_limits_max_bytes_from_pages() {
        let limits = MemoryLimits {
            max_pages: Some(10),
            ..Default::default()
        };

        assert_eq!(limits.max_bytes_from_pages(), Some(10 * 65536));

        let unlimited = MemoryLimits::unlimited();
        assert_eq!(unlimited.max_bytes_from_pages(), None);
    }

    #[test]
    fn test_memory_manager_initialize() {
        let manager = MemoryManager::with_defaults();
        manager.initialize(65536); // 1 page

        assert_eq!(manager.stats().current_bytes(), 65536);
    }

    #[test]
    fn test_snapshot_compress_decompress_roundtrip() {
        // Use highly compressible data (repeated bytes)
        let data = vec![0xABu8; 4096];
        let checksum = MemorySnapshot::compute_checksum(&data);
        let snapshot = MemorySnapshot {
            data: data.clone(),
            pages: 1,
            checksum,
        };
        let compressed = snapshot.compress();
        // Should be smaller than raw for highly repetitive data
        assert!(
            compressed.len() < data.len() + 13,
            "compressed ({}) should be smaller than raw+header ({})",
            compressed.len(),
            data.len() + 13
        );
        let recovered =
            MemorySnapshot::decompress(&compressed).expect("decompression should succeed");
        assert_eq!(recovered.data, data);
        assert_eq!(recovered.pages, 1);
        assert!(recovered.verify(), "checksum should pass");
    }

    #[test]
    fn test_snapshot_compress_too_short_returns_none() {
        assert!(MemorySnapshot::decompress(&[0u8; 5]).is_none());
        assert!(MemorySnapshot::decompress(&[]).is_none());
    }
}
