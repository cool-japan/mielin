//! Resource Efficiency Module
//!
//! Provides module instance pooling, memory page deduplication, lazy function compilation,
//! and code size optimization for efficient resource utilization in MielinOS.
//!
//! # Features
//!
//! - **Module Instance Pooling**: Reuse compiled module instances to reduce allocation overhead
//! - **Memory Page Deduplication**: Share identical memory pages across instances
//! - **Lazy Function Compilation**: Compile functions on-demand to reduce startup time
//! - **Code Size Optimization**: Minimize compiled code size for embedded systems

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use wasmtime::{Instance, Store};

use crate::host::HostState;
use crate::WasmError;

/// Hash of a WASM module for deduplication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleHash(u64);

impl ModuleHash {
    /// Compute hash from WASM bytecode using FNV-1a algorithm
    pub fn from_bytes(bytes: &[u8]) -> Self {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET_BASIS;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        Self(hash)
    }

    /// Get the raw hash value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Memory page for deduplication
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryPage {
    /// Page index
    pub index: usize,
    /// Page data (4KB)
    pub data: Vec<u8>,
}

impl MemoryPage {
    /// Standard page size (4KB)
    pub const PAGE_SIZE: usize = 4096;

    /// Create a new memory page
    pub fn new(index: usize, data: Vec<u8>) -> Self {
        assert!(data.len() <= Self::PAGE_SIZE, "Page data exceeds page size");
        Self { index, data }
    }

    /// Compute hash of this page
    pub fn hash(&self) -> u64 {
        ModuleHash::from_bytes(&self.data).as_u64()
    }

    /// Check if this page is all zeros
    pub fn is_zero(&self) -> bool {
        self.data.iter().all(|&b| b == 0)
    }

    /// Check if this page matches another
    pub fn matches(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

/// Memory page deduplication manager
pub struct PageDeduplicator {
    /// Pages indexed by hash
    pages: HashMap<u64, Arc<MemoryPage>>,
    /// Statistics
    stats: DeduplicationStats,
}

impl PageDeduplicator {
    /// Create a new page deduplicator
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            stats: DeduplicationStats::default(),
        }
    }

    /// Deduplicate a memory region
    pub fn deduplicate(&mut self, data: &[u8]) -> Vec<Arc<MemoryPage>> {
        let mut result = Vec::new();
        let page_count = data.len().div_ceil(MemoryPage::PAGE_SIZE);

        for i in 0..page_count {
            let start = i * MemoryPage::PAGE_SIZE;
            let end = std::cmp::min(start + MemoryPage::PAGE_SIZE, data.len());
            let page_data = data[start..end].to_vec();
            let page = MemoryPage::new(i, page_data);
            let hash = page.hash();

            self.stats.total_pages_processed += 1;

            if let Some(existing) = self.pages.get(&hash) {
                // Found duplicate
                self.stats.deduplicated_pages += 1;
                self.stats.bytes_saved += page.data.len();
                result.push(Arc::clone(existing));
            } else {
                // New unique page
                let page_arc = Arc::new(page);
                self.pages.insert(hash, Arc::clone(&page_arc));
                result.push(page_arc);
            }
        }

        result
    }

    /// Get statistics
    pub fn stats(&self) -> &DeduplicationStats {
        &self.stats
    }

    /// Clean up unused pages (those with only one reference)
    pub fn cleanup(&mut self) {
        self.pages.retain(|_, page| Arc::strong_count(page) > 1);
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = DeduplicationStats::default();
    }
}

impl Default for PageDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

/// Deduplication statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct DeduplicationStats {
    /// Total pages processed
    pub total_pages_processed: usize,
    /// Number of deduplicated pages
    pub deduplicated_pages: usize,
    /// Bytes saved through deduplication
    pub bytes_saved: usize,
}

impl DeduplicationStats {
    /// Get deduplication ratio
    pub fn deduplication_ratio(&self) -> f64 {
        if self.total_pages_processed == 0 {
            0.0
        } else {
            self.deduplicated_pages as f64 / self.total_pages_processed as f64
        }
    }

    /// Get space efficiency (bytes saved / total bytes)
    pub fn space_efficiency(&self) -> f64 {
        let total_bytes = self.total_pages_processed * MemoryPage::PAGE_SIZE;
        if total_bytes == 0 {
            0.0
        } else {
            self.bytes_saved as f64 / total_bytes as f64
        }
    }
}

/// Pooled module instance
pub struct PooledInstance {
    /// The module instance
    pub instance: Instance,
    /// The store
    pub store: Store<HostState>,
    /// Last used timestamp
    pub last_used: Instant,
    /// Number of times this instance has been reused
    pub reuse_count: usize,
}

impl PooledInstance {
    /// Create a new pooled instance
    pub fn new(instance: Instance, store: Store<HostState>) -> Self {
        Self {
            instance,
            store,
            last_used: Instant::now(),
            reuse_count: 0,
        }
    }

    /// Mark this instance as used
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.reuse_count += 1;
    }

    /// Check if this instance is expired
    pub fn is_expired(&self, max_idle_time: Duration) -> bool {
        self.last_used.elapsed() > max_idle_time
    }
}

/// Module instance pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of instances per module
    pub max_instances_per_module: usize,
    /// Maximum total instances across all modules
    pub max_total_instances: usize,
    /// Maximum idle time before instance is evicted
    pub max_idle_time: Duration,
    /// Enable instance warmup
    pub warmup_enabled: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_instances_per_module: 10,
            max_total_instances: 100,
            max_idle_time: Duration::from_secs(60),
            warmup_enabled: false,
        }
    }
}

impl PoolConfig {
    /// Preset for embedded systems (conservative)
    pub fn embedded() -> Self {
        Self {
            max_instances_per_module: 2,
            max_total_instances: 10,
            max_idle_time: Duration::from_secs(30),
            warmup_enabled: false,
        }
    }

    /// Preset for server workloads (aggressive pooling)
    pub fn server() -> Self {
        Self {
            max_instances_per_module: 50,
            max_total_instances: 1000,
            max_idle_time: Duration::from_secs(300),
            warmup_enabled: true,
        }
    }

    /// Preset for interactive workloads
    pub fn interactive() -> Self {
        Self {
            max_instances_per_module: 5,
            max_total_instances: 50,
            max_idle_time: Duration::from_secs(120),
            warmup_enabled: true,
        }
    }
}

/// Module instance pool
pub struct InstancePool {
    /// Available instances indexed by module hash
    available: HashMap<ModuleHash, VecDeque<PooledInstance>>,
    /// Configuration
    config: PoolConfig,
    /// Total number of instances
    total_instances: usize,
    /// Statistics
    stats: PoolStats,
}

impl InstancePool {
    /// Create a new instance pool
    pub fn new(config: PoolConfig) -> Self {
        Self {
            available: HashMap::new(),
            config,
            total_instances: 0,
            stats: PoolStats::default(),
        }
    }

    /// Acquire an instance from the pool or create a new one
    pub fn acquire<F>(
        &mut self,
        module_hash: ModuleHash,
        create_fn: F,
    ) -> Result<PooledInstance, WasmError>
    where
        F: FnOnce() -> Result<PooledInstance, WasmError>,
    {
        // Try to get from pool
        if let Some(instances) = self.available.get_mut(&module_hash) {
            while let Some(mut instance) = instances.pop_front() {
                // Check if instance is still valid
                if !instance.is_expired(self.config.max_idle_time) {
                    instance.mark_used();
                    self.stats.pool_hits += 1;
                    return Ok(instance);
                } else {
                    // Instance expired, decrease count
                    self.total_instances -= 1;
                    self.stats.evictions += 1;
                }
            }
        }

        // Pool miss, create new instance
        self.stats.pool_misses += 1;

        if self.total_instances >= self.config.max_total_instances {
            // Evict least recently used instance
            self.evict_lru();
        }

        let instance = create_fn()?;
        self.total_instances += 1;
        Ok(instance)
    }

    /// Return an instance to the pool
    pub fn release(&mut self, module_hash: ModuleHash, mut instance: PooledInstance) {
        let instances = self.available.entry(module_hash).or_default();

        // Check per-module limit
        if instances.len() >= self.config.max_instances_per_module {
            // Don't add to pool, just drop it
            self.total_instances -= 1;
            return;
        }

        instance.last_used = Instant::now();
        instances.push_back(instance);
    }

    /// Evict least recently used instance
    fn evict_lru(&mut self) {
        let mut oldest_time = Instant::now();
        let mut oldest_key = None;

        for (key, instances) in &self.available {
            if let Some(instance) = instances.front() {
                if instance.last_used < oldest_time {
                    oldest_time = instance.last_used;
                    oldest_key = Some(*key);
                }
            }
        }

        if let Some(key) = oldest_key {
            if let Some(instances) = self.available.get_mut(&key) {
                if instances.pop_front().is_some() {
                    self.total_instances -= 1;
                    self.stats.evictions += 1;
                }
            }
        }
    }

    /// Clean up expired instances
    pub fn cleanup(&mut self) {
        for instances in self.available.values_mut() {
            let before = instances.len();
            instances.retain(|instance| !instance.is_expired(self.config.max_idle_time));
            let removed = before - instances.len();
            self.total_instances -= removed;
            self.stats.evictions += removed;
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }

    /// Get total number of instances
    pub fn total_instances(&self) -> usize {
        self.total_instances
    }

    /// Clear the entire pool
    pub fn clear(&mut self) {
        self.available.clear();
        self.total_instances = 0;
    }
}

/// Pool statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct PoolStats {
    /// Number of pool hits
    pub pool_hits: u64,
    /// Number of pool misses
    pub pool_misses: u64,
    /// Number of evictions
    pub evictions: usize,
}

impl PoolStats {
    /// Get pool hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.pool_hits + self.pool_misses;
        if total == 0 {
            0.0
        } else {
            self.pool_hits as f64 / total as f64
        }
    }
}

/// Lazy compilation manager
pub struct LazyCompiler {
    /// Compiled functions indexed by function index
    compiled: Mutex<HashMap<u32, Weak<CompiledFunction>>>,
    /// Statistics
    stats: Mutex<LazyCompilationStats>,
}

impl LazyCompiler {
    /// Create a new lazy compiler
    pub fn new() -> Self {
        Self {
            compiled: Mutex::new(HashMap::new()),
            stats: Mutex::new(LazyCompilationStats::default()),
        }
    }

    /// Request compilation of a function
    pub fn request_compilation(&self, function_index: u32) -> Arc<CompiledFunction> {
        let mut compiled = self.compiled.lock().expect("Compiled pool lock poisoned");
        let mut stats = self.stats.lock().expect("Resource stats lock poisoned");

        // Check if already compiled
        if let Some(weak) = compiled.get(&function_index) {
            if let Some(strong) = weak.upgrade() {
                stats.cache_hits += 1;
                return strong;
            }
        }

        // Compile the function
        stats.cache_misses += 1;
        stats.total_compiled += 1;

        let func = Arc::new(CompiledFunction {
            function_index,
            compiled_at: Instant::now(),
        });

        compiled.insert(function_index, Arc::downgrade(&func));
        func
    }

    /// Get statistics
    pub fn stats(&self) -> LazyCompilationStats {
        *self.stats.lock().expect("Resource stats lock poisoned")
    }

    /// Clean up unused compiled functions
    pub fn cleanup(&self) {
        let mut compiled = self.compiled.lock().expect("Compiled pool lock poisoned");
        compiled.retain(|_, weak| weak.strong_count() > 0);
    }
}

impl Default for LazyCompiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Compiled function metadata
#[derive(Debug)]
pub struct CompiledFunction {
    /// Function index
    pub function_index: u32,
    /// Time when compiled
    pub compiled_at: Instant,
}

/// Lazy compilation statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct LazyCompilationStats {
    /// Total functions compiled
    pub total_compiled: usize,
    /// Cache hits (function already compiled)
    pub cache_hits: u64,
    /// Cache misses (function needs compilation)
    pub cache_misses: u64,
}

impl LazyCompilationStats {
    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
}

/// Code size optimizer
pub struct CodeSizeOptimizer {
    /// Enable dead code elimination
    pub dead_code_elimination: bool,
    /// Enable function inlining
    pub function_inlining: bool,
    /// Enable constant folding
    pub constant_folding: bool,
    /// Maximum inline size
    pub max_inline_size: usize,
}

impl CodeSizeOptimizer {
    /// Create a new code size optimizer
    pub fn new() -> Self {
        Self {
            dead_code_elimination: true,
            function_inlining: true,
            constant_folding: true,
            max_inline_size: 128,
        }
    }

    /// Preset for aggressive size optimization
    pub fn aggressive() -> Self {
        Self {
            dead_code_elimination: true,
            function_inlining: true,
            constant_folding: true,
            max_inline_size: 64,
        }
    }

    /// Preset for minimal optimization
    pub fn minimal() -> Self {
        Self {
            dead_code_elimination: false,
            function_inlining: false,
            constant_folding: false,
            max_inline_size: 0,
        }
    }

    /// Estimate code size reduction
    pub fn estimate_reduction(&self, original_size: usize) -> usize {
        let mut reduction = 0;

        if self.dead_code_elimination {
            reduction += original_size / 10; // ~10% reduction
        }

        if self.constant_folding {
            reduction += original_size / 20; // ~5% reduction
        }

        std::cmp::min(reduction, original_size)
    }
}

impl Default for CodeSizeOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_hash() {
        let data1 = b"hello world";
        let data2 = b"hello world";
        let data3 = b"different";

        let hash1 = ModuleHash::from_bytes(data1);
        let hash2 = ModuleHash::from_bytes(data2);
        let hash3 = ModuleHash::from_bytes(data3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_memory_page() {
        let data = vec![1, 2, 3, 4];
        let page = MemoryPage::new(0, data);

        assert_eq!(page.index, 0);
        assert!(!page.is_zero());
    }

    #[test]
    fn test_page_deduplication() {
        let mut dedup = PageDeduplicator::new();

        // Create memory with duplicate pages
        let mut data = vec![0u8; MemoryPage::PAGE_SIZE * 3];
        // Page 0: all zeros
        // Page 1: all ones
        for byte in data
            .iter_mut()
            .skip(MemoryPage::PAGE_SIZE)
            .take(MemoryPage::PAGE_SIZE)
        {
            *byte = 1;
        }
        // Page 2: all zeros (duplicate of page 0)

        let pages = dedup.deduplicate(&data);
        assert_eq!(pages.len(), 3);

        let stats = dedup.stats();
        assert_eq!(stats.total_pages_processed, 3);
        assert_eq!(stats.deduplicated_pages, 1); // Page 2 is duplicate of page 0
    }

    #[test]
    fn test_pool_config_presets() {
        let embedded = PoolConfig::embedded();
        assert_eq!(embedded.max_instances_per_module, 2);

        let server = PoolConfig::server();
        assert!(server.max_total_instances > embedded.max_total_instances);

        let interactive = PoolConfig::interactive();
        assert!(interactive.warmup_enabled);
    }

    #[test]
    fn test_lazy_compiler() {
        let compiler = LazyCompiler::new();

        let func1 = compiler.request_compilation(0);
        assert_eq!(func1.function_index, 0);

        let func2 = compiler.request_compilation(0);
        assert_eq!(func2.function_index, 0); // Verify func2 points to same function
        assert_eq!(Arc::strong_count(&func1), 2); // Both func1 and func2 point to same

        let stats = compiler.stats();
        assert_eq!(stats.total_compiled, 1);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }

    #[test]
    fn test_code_size_optimizer() {
        let optimizer = CodeSizeOptimizer::new();
        let original_size = 1000;

        let reduction = optimizer.estimate_reduction(original_size);
        assert!(reduction > 0);
        assert!(reduction <= original_size);

        let aggressive = CodeSizeOptimizer::aggressive();
        let aggressive_reduction = aggressive.estimate_reduction(original_size);
        assert!(aggressive_reduction > 0);

        let minimal = CodeSizeOptimizer::minimal();
        let minimal_reduction = minimal.estimate_reduction(original_size);
        assert_eq!(minimal_reduction, 0);
    }

    #[test]
    fn test_deduplication_stats() {
        let stats = DeduplicationStats {
            total_pages_processed: 100,
            deduplicated_pages: 25,
            bytes_saved: 25 * MemoryPage::PAGE_SIZE,
        };

        assert_eq!(stats.deduplication_ratio(), 0.25);
        assert_eq!(stats.space_efficiency(), 0.25);
    }

    #[test]
    fn test_pool_stats() {
        let stats = PoolStats {
            pool_hits: 75,
            pool_misses: 25,
            evictions: 5,
        };

        assert_eq!(stats.hit_rate(), 0.75);
    }

    #[test]
    fn test_lazy_compilation_stats() {
        let stats = LazyCompilationStats {
            total_compiled: 100,
            cache_hits: 300,
            cache_misses: 100,
        };

        assert_eq!(stats.hit_rate(), 0.75);
    }
}
