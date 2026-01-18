//! JIT (Just-In-Time) Optimization Module
//!
//! Provides tiered compilation, profile-guided optimization, hot function detection,
//! and inline caching for WebAssembly execution in MielinOS.
//!
//! # Architecture
//!
//! The JIT system uses a three-tier compilation strategy:
//! 1. **Interpreter Tier**: Quick startup, collects profiling data
//! 2. **Baseline Tier**: Fast compilation with basic optimizations
//! 3. **Optimized Tier**: Aggressive optimizations for hot functions
//!
//! # Features
//!
//! - **Profile-Guided Optimization**: Collects runtime statistics to guide optimization
//! - **Hot Function Detection**: Identifies frequently called functions for optimization
//! - **Inline Caching**: Caches type information for faster polymorphic calls
//! - **Tiered Compilation**: Progressive optimization based on execution frequency

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wasmtime::{Config, OptLevel, Strategy};

/// Compilation tier for WebAssembly modules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilationTier {
    /// Interpreter tier - fastest startup, no compilation
    Interpreter,
    /// Baseline tier - fast compilation with minimal optimizations
    Baseline,
    /// Optimized tier - aggressive optimizations for hot code
    Optimized,
}

impl CompilationTier {
    /// Check if this tier is higher than another
    pub fn is_higher_than(&self, other: &Self) -> bool {
        (*self as u8) > (*other as u8)
    }
}

/// Profile data for a WebAssembly function
#[derive(Debug, Clone)]
pub struct FunctionProfile {
    /// Function index in the module
    pub function_index: u32,
    /// Number of times this function has been called
    pub call_count: u64,
    /// Total execution time in nanoseconds
    pub total_execution_ns: u64,
    /// Average execution time in nanoseconds
    pub avg_execution_ns: u64,
    /// Peak execution time in nanoseconds
    pub peak_execution_ns: u64,
    /// Current compilation tier
    pub tier: CompilationTier,
    /// Timestamp of last optimization
    pub last_optimized: Option<Instant>,
}

impl FunctionProfile {
    /// Create a new function profile
    pub fn new(function_index: u32) -> Self {
        Self {
            function_index,
            call_count: 0,
            total_execution_ns: 0,
            avg_execution_ns: 0,
            peak_execution_ns: 0,
            tier: CompilationTier::Interpreter,
            last_optimized: None,
        }
    }

    /// Record a function call
    pub fn record_call(&mut self, duration_ns: u64) {
        self.call_count += 1;
        self.total_execution_ns += duration_ns;
        self.avg_execution_ns = self.total_execution_ns / self.call_count;
        if duration_ns > self.peak_execution_ns {
            self.peak_execution_ns = duration_ns;
        }
    }

    /// Check if this function is hot and should be optimized
    pub fn is_hot(&self, threshold: &HotFunctionThreshold) -> bool {
        self.call_count >= threshold.min_call_count
            && self.total_execution_ns >= threshold.min_total_time_ns
    }

    /// Update compilation tier
    pub fn set_tier(&mut self, tier: CompilationTier) {
        self.tier = tier;
        self.last_optimized = Some(Instant::now());
    }
}

/// Thresholds for hot function detection
#[derive(Debug, Clone)]
pub struct HotFunctionThreshold {
    /// Minimum number of calls to be considered hot
    pub min_call_count: u64,
    /// Minimum total execution time (ns) to be considered hot
    pub min_total_time_ns: u64,
    /// Minimum time since last optimization (prevents thrashing)
    pub min_reopt_interval: Duration,
}

impl Default for HotFunctionThreshold {
    fn default() -> Self {
        Self {
            min_call_count: 100,
            min_total_time_ns: 1_000_000, // 1ms
            min_reopt_interval: Duration::from_millis(100),
        }
    }
}

impl HotFunctionThreshold {
    /// Preset for interactive workloads (lower thresholds)
    pub fn interactive() -> Self {
        Self {
            min_call_count: 50,
            min_total_time_ns: 500_000, // 500µs
            min_reopt_interval: Duration::from_millis(50),
        }
    }

    /// Preset for batch workloads (higher thresholds)
    pub fn batch() -> Self {
        Self {
            min_call_count: 500,
            min_total_time_ns: 5_000_000, // 5ms
            min_reopt_interval: Duration::from_millis(500),
        }
    }

    /// Preset for embedded systems (conservative)
    pub fn embedded() -> Self {
        Self {
            min_call_count: 200,
            min_total_time_ns: 2_000_000, // 2ms
            min_reopt_interval: Duration::from_millis(200),
        }
    }
}

/// Inline cache entry for polymorphic call sites
#[derive(Debug, Clone)]
pub struct InlineCacheEntry {
    /// Type signature hash
    pub type_hash: u64,
    /// Cached function index
    pub function_index: u32,
    /// Hit count for this cache entry
    pub hit_count: u64,
    /// Miss count for this cache entry
    pub miss_count: u64,
}

impl InlineCacheEntry {
    /// Create a new inline cache entry
    pub fn new(type_hash: u64, function_index: u32) -> Self {
        Self {
            type_hash,
            function_index,
            hit_count: 0,
            miss_count: 0,
        }
    }

    /// Record a cache hit
    pub fn hit(&mut self) {
        self.hit_count += 1;
    }

    /// Record a cache miss
    pub fn miss(&mut self) {
        self.miss_count += 1;
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hit_count + self.miss_count;
        if total == 0 {
            0.0
        } else {
            self.hit_count as f64 / total as f64
        }
    }
}

/// Inline cache for polymorphic call sites
#[derive(Debug, Clone)]
pub struct InlineCache {
    /// Cache entries indexed by call site ID
    entries: HashMap<u32, Vec<InlineCacheEntry>>,
    /// Maximum number of entries per call site
    max_entries_per_site: usize,
}

impl InlineCache {
    /// Create a new inline cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries_per_site: 4,
        }
    }

    /// Look up a call site in the cache
    pub fn lookup(&mut self, call_site_id: u32, type_hash: u64) -> Option<u32> {
        if let Some(entries) = self.entries.get_mut(&call_site_id) {
            for entry in entries.iter_mut() {
                if entry.type_hash == type_hash {
                    entry.hit();
                    return Some(entry.function_index);
                }
            }
            // Cache miss
            for entry in entries.iter_mut() {
                entry.miss();
            }
        }
        None
    }

    /// Insert an entry into the cache
    pub fn insert(&mut self, call_site_id: u32, type_hash: u64, function_index: u32) {
        let entries = self.entries.entry(call_site_id).or_default();

        // Check if entry already exists
        for entry in entries.iter_mut() {
            if entry.type_hash == type_hash {
                entry.function_index = function_index;
                return;
            }
        }

        // Add new entry if under limit
        if entries.len() < self.max_entries_per_site {
            entries.push(InlineCacheEntry::new(type_hash, function_index));
        } else {
            // Evict entry with lowest hit rate
            if let Some((idx, _)) = entries.iter().enumerate().min_by(|(_, a), (_, b)| {
                a.hit_rate()
                    .partial_cmp(&b.hit_rate())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                entries[idx] = InlineCacheEntry::new(type_hash, function_index);
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> InlineCacheStats {
        let mut total_hits = 0;
        let mut total_misses = 0;
        let mut total_entries = 0;

        for entries in self.entries.values() {
            for entry in entries {
                total_hits += entry.hit_count;
                total_misses += entry.miss_count;
                total_entries += 1;
            }
        }

        InlineCacheStats {
            total_call_sites: self.entries.len(),
            total_entries,
            total_hits,
            total_misses,
        }
    }
}

impl Default for InlineCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for inline cache
#[derive(Debug, Clone, Copy)]
pub struct InlineCacheStats {
    /// Total number of call sites
    pub total_call_sites: usize,
    /// Total number of cache entries
    pub total_entries: usize,
    /// Total cache hits
    pub total_hits: u64,
    /// Total cache misses
    pub total_misses: u64,
}

impl InlineCacheStats {
    /// Get overall hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_hits + self.total_misses;
        if total == 0 {
            0.0
        } else {
            self.total_hits as f64 / total as f64
        }
    }
}

/// Profile-guided optimization manager
#[derive(Clone)]
pub struct ProfileGuidedOptimizer {
    /// Function profiles indexed by function index
    profiles: Arc<Mutex<HashMap<u32, FunctionProfile>>>,
    /// Hot function threshold
    threshold: HotFunctionThreshold,
    /// Inline cache
    inline_cache: Arc<Mutex<InlineCache>>,
    /// Enable profiling
    profiling_enabled: bool,
}

impl ProfileGuidedOptimizer {
    /// Create a new profile-guided optimizer
    pub fn new(threshold: HotFunctionThreshold) -> Self {
        Self {
            profiles: Arc::new(Mutex::new(HashMap::new())),
            threshold,
            inline_cache: Arc::new(Mutex::new(InlineCache::new())),
            profiling_enabled: true,
        }
    }

    /// Enable or disable profiling
    pub fn set_profiling_enabled(&mut self, enabled: bool) {
        self.profiling_enabled = enabled;
    }

    /// Record a function call
    pub fn record_call(&self, function_index: u32, duration_ns: u64) {
        if !self.profiling_enabled {
            return;
        }

        let mut profiles = self.profiles.lock().expect("JIT profiles lock poisoned");
        let profile = profiles
            .entry(function_index)
            .or_insert_with(|| FunctionProfile::new(function_index));
        profile.record_call(duration_ns);
    }

    /// Get hot functions that should be optimized
    pub fn get_hot_functions(&self) -> Vec<u32> {
        let profiles = self.profiles.lock().expect("JIT profiles lock poisoned");
        profiles
            .values()
            .filter(|p| p.is_hot(&self.threshold))
            .map(|p| p.function_index)
            .collect()
    }

    /// Get all function profiles
    pub fn get_profiles(&self) -> HashMap<u32, FunctionProfile> {
        self.profiles
            .lock()
            .expect("JIT profiles lock poisoned")
            .clone()
    }

    /// Update tier for a function
    pub fn set_tier(&self, function_index: u32, tier: CompilationTier) {
        let mut profiles = self.profiles.lock().expect("JIT profiles lock poisoned");
        if let Some(profile) = profiles.get_mut(&function_index) {
            profile.set_tier(tier);
        }
    }

    /// Get inline cache
    pub fn inline_cache(&self) -> Arc<Mutex<InlineCache>> {
        Arc::clone(&self.inline_cache)
    }

    /// Get inline cache statistics
    pub fn inline_cache_stats(&self) -> InlineCacheStats {
        self.inline_cache
            .lock()
            .expect("Inline cache lock poisoned")
            .stats()
    }

    /// Reset all profiles (for testing)
    pub fn reset(&self) {
        self.profiles
            .lock()
            .expect("JIT profiles lock poisoned")
            .clear();
        *self
            .inline_cache
            .lock()
            .expect("Inline cache lock poisoned") = InlineCache::new();
    }
}

impl Default for ProfileGuidedOptimizer {
    fn default() -> Self {
        Self::new(HotFunctionThreshold::default())
    }
}

/// JIT configuration builder
pub struct JitConfig {
    /// Enable tiered compilation
    pub tiered_compilation: bool,
    /// Enable profile-guided optimization
    pub profile_guided_optimization: bool,
    /// Enable inline caching
    pub inline_caching: bool,
    /// Hot function threshold
    pub hot_threshold: HotFunctionThreshold,
    /// Initial compilation tier
    pub initial_tier: CompilationTier,
}

impl JitConfig {
    /// Create a new JIT configuration
    pub fn new() -> Self {
        Self {
            tiered_compilation: true,
            profile_guided_optimization: true,
            inline_caching: true,
            hot_threshold: HotFunctionThreshold::default(),
            initial_tier: CompilationTier::Baseline,
        }
    }

    /// Preset for interactive workloads
    pub fn interactive() -> Self {
        Self {
            tiered_compilation: true,
            profile_guided_optimization: true,
            inline_caching: true,
            hot_threshold: HotFunctionThreshold::interactive(),
            initial_tier: CompilationTier::Baseline,
        }
    }

    /// Preset for batch workloads
    pub fn batch() -> Self {
        Self {
            tiered_compilation: true,
            profile_guided_optimization: true,
            inline_caching: true,
            hot_threshold: HotFunctionThreshold::batch(),
            initial_tier: CompilationTier::Optimized,
        }
    }

    /// Preset for embedded systems
    pub fn embedded() -> Self {
        Self {
            tiered_compilation: false,
            profile_guided_optimization: false,
            inline_caching: false,
            hot_threshold: HotFunctionThreshold::embedded(),
            initial_tier: CompilationTier::Baseline,
        }
    }

    /// Disable all optimizations
    pub fn no_optimization() -> Self {
        Self {
            tiered_compilation: false,
            profile_guided_optimization: false,
            inline_caching: false,
            hot_threshold: HotFunctionThreshold::default(),
            initial_tier: CompilationTier::Interpreter,
        }
    }

    /// Apply this configuration to a Wasmtime Config
    pub fn apply_to_config(&self, config: &mut Config) {
        match self.initial_tier {
            CompilationTier::Interpreter => {
                config.strategy(Strategy::Auto);
                config.cranelift_opt_level(OptLevel::None);
            }
            CompilationTier::Baseline => {
                config.strategy(Strategy::Cranelift);
                config.cranelift_opt_level(OptLevel::Speed);
            }
            CompilationTier::Optimized => {
                config.strategy(Strategy::Cranelift);
                config.cranelift_opt_level(OptLevel::SpeedAndSize);
            }
        }
    }
}

impl Default for JitConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// JIT optimizer that manages tiered compilation
pub struct JitOptimizer {
    /// JIT configuration
    config: JitConfig,
    /// Profile-guided optimizer
    pgo: ProfileGuidedOptimizer,
}

impl JitOptimizer {
    /// Create a new JIT optimizer
    pub fn new(config: JitConfig) -> Self {
        let pgo = ProfileGuidedOptimizer::new(config.hot_threshold.clone());
        Self { config, pgo }
    }

    /// Create optimizer with default configuration
    pub fn with_default() -> Self {
        Self::new(JitConfig::default())
    }

    /// Create optimizer for interactive workloads
    pub fn for_interactive() -> Self {
        Self::new(JitConfig::interactive())
    }

    /// Create optimizer for batch workloads
    pub fn for_batch() -> Self {
        Self::new(JitConfig::batch())
    }

    /// Create optimizer for embedded systems
    pub fn for_embedded() -> Self {
        Self::new(JitConfig::embedded())
    }

    /// Get configuration
    pub fn config(&self) -> &JitConfig {
        &self.config
    }

    /// Get profile-guided optimizer
    pub fn pgo(&self) -> &ProfileGuidedOptimizer {
        &self.pgo
    }

    /// Record function execution
    pub fn record_execution(&self, function_index: u32, duration: Duration) {
        if self.config.profile_guided_optimization {
            self.pgo
                .record_call(function_index, duration.as_nanos() as u64);
        }
    }

    /// Get functions that should be recompiled at higher tier
    pub fn get_recompilation_candidates(&self) -> Vec<(u32, CompilationTier)> {
        if !self.config.tiered_compilation {
            return Vec::new();
        }

        let hot_functions = self.pgo.get_hot_functions();
        let profiles = self.pgo.get_profiles();

        hot_functions
            .into_iter()
            .filter_map(|idx| {
                let profile = profiles.get(&idx)?;
                let target_tier = match profile.tier {
                    CompilationTier::Interpreter => CompilationTier::Baseline,
                    CompilationTier::Baseline => CompilationTier::Optimized,
                    CompilationTier::Optimized => return None,
                };
                Some((idx, target_tier))
            })
            .collect()
    }

    /// Apply JIT configuration to Wasmtime config
    pub fn configure_engine(&self, config: &mut Config) {
        self.config.apply_to_config(config);
    }
}

impl Default for JitOptimizer {
    fn default() -> Self {
        Self::with_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compilation_tier_ordering() {
        assert!(CompilationTier::Baseline.is_higher_than(&CompilationTier::Interpreter));
        assert!(CompilationTier::Optimized.is_higher_than(&CompilationTier::Baseline));
        assert!(!CompilationTier::Interpreter.is_higher_than(&CompilationTier::Baseline));
    }

    #[test]
    fn test_function_profile_recording() {
        let mut profile = FunctionProfile::new(0);
        assert_eq!(profile.call_count, 0);
        assert_eq!(profile.total_execution_ns, 0);

        profile.record_call(1000);
        assert_eq!(profile.call_count, 1);
        assert_eq!(profile.total_execution_ns, 1000);
        assert_eq!(profile.avg_execution_ns, 1000);

        profile.record_call(2000);
        assert_eq!(profile.call_count, 2);
        assert_eq!(profile.total_execution_ns, 3000);
        assert_eq!(profile.avg_execution_ns, 1500);
        assert_eq!(profile.peak_execution_ns, 2000);
    }

    #[test]
    fn test_hot_function_detection() {
        let threshold = HotFunctionThreshold::default();
        let mut profile = FunctionProfile::new(0);

        assert!(!profile.is_hot(&threshold));

        // Record enough calls to exceed threshold
        for _ in 0..100 {
            profile.record_call(10_000);
        }

        assert!(profile.is_hot(&threshold));
    }

    #[test]
    fn test_inline_cache_basic() {
        let mut cache = InlineCache::new();

        // Initially no entry
        assert_eq!(cache.lookup(0, 123), None);

        // Insert entry
        cache.insert(0, 123, 456);

        // Should find it now
        assert_eq!(cache.lookup(0, 123), Some(456));

        // Different type hash should miss
        assert_eq!(cache.lookup(0, 999), None);
    }

    #[test]
    fn test_inline_cache_eviction() {
        let mut cache = InlineCache::new();
        cache.max_entries_per_site = 2;

        // Insert two entries
        cache.insert(0, 1, 10);
        cache.insert(0, 2, 20);

        // Both should be present
        assert_eq!(cache.lookup(0, 1), Some(10));
        assert_eq!(cache.lookup(0, 2), Some(20));

        // Insert third entry - should evict one with lower hit rate
        cache.insert(0, 3, 30);

        // The cache should still have 2 entries
        let entries = &cache.entries[&0];
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_inline_cache_stats() {
        let mut cache = InlineCache::new();
        cache.insert(0, 1, 10);

        // Hit
        cache.lookup(0, 1);

        // Miss
        cache.lookup(0, 2);

        let stats = cache.stats();
        assert_eq!(stats.total_hits, 1);
        assert!(stats.total_misses > 0);
    }

    #[test]
    fn test_pgo_basic() {
        let pgo = ProfileGuidedOptimizer::default();

        pgo.record_call(0, 1000);
        pgo.record_call(0, 2000);

        let profiles = pgo.get_profiles();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[&0].call_count, 2);
    }

    #[test]
    fn test_pgo_hot_functions() {
        let threshold = HotFunctionThreshold {
            min_call_count: 5,
            min_total_time_ns: 10_000,
            min_reopt_interval: Duration::from_millis(0),
        };
        let pgo = ProfileGuidedOptimizer::new(threshold);

        // Function 0: hot
        for _ in 0..10 {
            pgo.record_call(0, 2000);
        }

        // Function 1: not hot (too few calls)
        pgo.record_call(1, 10_000);

        // Function 2: not hot (too little time)
        for _ in 0..10 {
            pgo.record_call(2, 100);
        }

        let hot = pgo.get_hot_functions();
        assert_eq!(hot.len(), 1);
        assert!(hot.contains(&0));
    }

    #[test]
    fn test_jit_config_presets() {
        let interactive = JitConfig::interactive();
        assert!(interactive.tiered_compilation);
        assert!(interactive.profile_guided_optimization);

        let batch = JitConfig::batch();
        assert_eq!(batch.initial_tier, CompilationTier::Optimized);

        let embedded = JitConfig::embedded();
        assert!(!embedded.tiered_compilation);

        let no_opt = JitConfig::no_optimization();
        assert!(!no_opt.tiered_compilation);
        assert_eq!(no_opt.initial_tier, CompilationTier::Interpreter);
    }

    #[test]
    fn test_jit_optimizer_recompilation_candidates() {
        let config = JitConfig {
            tiered_compilation: true,
            profile_guided_optimization: true,
            inline_caching: false,
            hot_threshold: HotFunctionThreshold {
                min_call_count: 5,
                min_total_time_ns: 10_000,
                min_reopt_interval: Duration::from_millis(0),
            },
            initial_tier: CompilationTier::Interpreter,
        };
        let optimizer = JitOptimizer::new(config);

        // Record hot function (starting at Interpreter tier)
        for _ in 0..10 {
            optimizer.record_execution(0, Duration::from_micros(2));
        }

        let candidates = optimizer.get_recompilation_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].0, 0);
        assert_eq!(candidates[0].1, CompilationTier::Baseline);
    }

    #[test]
    fn test_jit_optimizer_with_tiered_compilation_disabled() {
        let config = JitConfig {
            tiered_compilation: false,
            profile_guided_optimization: true,
            inline_caching: false,
            hot_threshold: HotFunctionThreshold::default(),
            initial_tier: CompilationTier::Baseline,
        };
        let optimizer = JitOptimizer::new(config);

        // Record hot function
        for _ in 0..1000 {
            optimizer.record_execution(0, Duration::from_micros(10));
        }

        // Should have no candidates since tiered compilation is disabled
        let candidates = optimizer.get_recompilation_candidates();
        assert_eq!(candidates.len(), 0);
    }
}
