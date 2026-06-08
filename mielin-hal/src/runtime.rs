//! Runtime Capability Switching
//!
//! This module provides dynamic feature detection and optimal path selection
//! at runtime. It enables fallback mechanisms when optimal features are missing
//! and allows applications to adapt to hardware changes (hot-plug scenarios).
//!
//! ## Features
//!
//! - **Dynamic Detection**: Re-detect capabilities at runtime
//! - **Fallback Chains**: Define multiple implementation paths with priorities
//! - **Optimal Path Selection**: Automatically choose the best available implementation
//! - **Feature Verification**: Verify that selected features actually work
//! - **Hot-Reload Support**: Handle hardware changes at runtime
//!
//! ## Example
//!
//! ```no_run
//! use mielin_hal::runtime::{RuntimeSelector, FeatureRequirement, FallbackChain};
//! use mielin_hal::capabilities::HardwareCapabilities;
//!
//! // Create a fallback chain for SIMD operations
//! let mut chain = FallbackChain::new("vector_multiply");
//!
//! // Prefer AVX512 > AVX2 > SSE4.2 > scalar
//! chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]));
//! chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]));
//! chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::SSE4_2]));
//! chain.add_requirement(FeatureRequirement::none()); // Scalar fallback
//!
//! // Select optimal implementation
//! let selector = RuntimeSelector::new();
//! let selected = selector.select(&chain).expect("Failed to select implementation");
//! ```

use crate::capabilities::{HardwareCapabilities, HardwareProfile};
use crate::error::{Error, Result};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Feature requirement for a specific code path
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureRequirement {
    /// Required hardware capabilities
    pub required: HardwareCapabilities,
    /// Optional capabilities that improve performance
    pub preferred: HardwareCapabilities,
    /// Capabilities that must NOT be present (for workarounds)
    pub excluded: HardwareCapabilities,
    /// Minimum vector width required (0 = any)
    pub min_vector_width: usize,
    /// Minimum core count required
    pub min_cores: usize,
    /// Minimum cache size required (bytes, 0 = any)
    pub min_cache_size: usize,
    /// Priority score (higher = more preferred)
    pub priority: u32,
    /// Human-readable name for this requirement
    pub name: String,
}

impl FeatureRequirement {
    /// Create a requirement that needs all specified capabilities
    pub fn all_of(caps: &[HardwareCapabilities]) -> Self {
        let mut required = HardwareCapabilities::empty();
        for cap in caps {
            required |= *cap;
        }
        Self {
            required,
            preferred: HardwareCapabilities::empty(),
            excluded: HardwareCapabilities::empty(),
            min_vector_width: 0,
            min_cores: 1,
            min_cache_size: 0,
            priority: Self::calculate_priority(required),
            name: Self::generate_name(required),
        }
    }

    /// Create a requirement that needs any of the specified capabilities
    pub fn any_of(caps: &[HardwareCapabilities]) -> Self {
        let mut required = HardwareCapabilities::empty();
        for cap in caps {
            required |= *cap;
        }
        Self {
            required,
            preferred: HardwareCapabilities::empty(),
            excluded: HardwareCapabilities::empty(),
            min_vector_width: 0,
            min_cores: 1,
            min_cache_size: 0,
            priority: Self::calculate_priority(required) / 2, // Lower priority for "any"
            name: Self::generate_name(required),
        }
    }

    /// Create a requirement with no mandatory features (fallback)
    pub fn none() -> Self {
        Self {
            required: HardwareCapabilities::empty(),
            preferred: HardwareCapabilities::empty(),
            excluded: HardwareCapabilities::empty(),
            min_vector_width: 0,
            min_cores: 1,
            min_cache_size: 0,
            priority: 0,
            name: "scalar_fallback".to_string(),
        }
    }

    /// Set preferred (optional) capabilities
    pub fn with_preferred(mut self, caps: HardwareCapabilities) -> Self {
        self.preferred = caps;
        self
    }

    /// Set excluded capabilities
    pub fn with_excluded(mut self, caps: HardwareCapabilities) -> Self {
        self.excluded = caps;
        self
    }

    /// Set minimum vector width
    pub fn with_min_vector_width(mut self, width: usize) -> Self {
        self.min_vector_width = width;
        self
    }

    /// Set minimum core count
    pub fn with_min_cores(mut self, cores: usize) -> Self {
        self.min_cores = cores;
        self
    }

    /// Set minimum cache size
    pub fn with_min_cache_size(mut self, size: usize) -> Self {
        self.min_cache_size = size;
        self
    }

    /// Set custom priority
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Set custom name
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Check if this requirement is satisfied by the given profile
    pub fn is_satisfied_by(&self, profile: &HardwareProfile) -> bool {
        // Check required capabilities
        if !profile.capabilities.contains(self.required) {
            return false;
        }

        // Check excluded capabilities
        if profile.capabilities.intersects(self.excluded) {
            return false;
        }

        // Check vector width
        if self.min_vector_width > 0 && profile.max_vector_width() < self.min_vector_width {
            return false;
        }

        // Check core count
        if profile.core_count < self.min_cores {
            return false;
        }

        // Check cache size
        if self.min_cache_size > 0 {
            let cache = crate::cache::CacheTopology::detect();
            if cache.l1_total_size() < self.min_cache_size {
                return false;
            }
        }

        true
    }

    /// Calculate a satisfaction score (0-100) for this requirement
    pub fn satisfaction_score(&self, profile: &HardwareProfile) -> u32 {
        if !self.is_satisfied_by(profile) {
            return 0;
        }

        let mut score = self.priority;

        // Bonus for preferred capabilities
        let preferred_count = (profile.capabilities & self.preferred).bits().count_ones();
        score += preferred_count * 10;

        // Bonus for exceeding minimum requirements
        if self.min_vector_width > 0 {
            let width_ratio = profile.max_vector_width() / self.min_vector_width.max(1);
            score += width_ratio.min(4) as u32 * 5;
        }

        if self.min_cores > 0 {
            let core_ratio = profile.core_count / self.min_cores.max(1);
            score += core_ratio.min(8) as u32 * 3;
        }

        score
    }

    /// Calculate priority based on capabilities
    fn calculate_priority(caps: HardwareCapabilities) -> u32 {
        let mut priority = 10;

        // x86_64 SIMD hierarchy
        if caps.contains(HardwareCapabilities::AVX512) {
            priority += 50;
        } else if caps.contains(HardwareCapabilities::AVX2) {
            priority += 40;
        } else if caps.contains(HardwareCapabilities::AVX) {
            priority += 30;
        } else if caps.contains(HardwareCapabilities::SSE4_2) {
            priority += 20;
        }

        // AArch64 SIMD hierarchy
        if caps.contains(HardwareCapabilities::SME) {
            priority += 55;
        } else if caps.contains(HardwareCapabilities::SVE2) {
            priority += 45;
        } else if caps.contains(HardwareCapabilities::SVE) {
            priority += 35;
        } else if caps.contains(HardwareCapabilities::NEON) {
            priority += 25;
        }

        // Additional features
        if caps.contains(HardwareCapabilities::FMA) {
            priority += 10;
        }
        if caps.contains(HardwareCapabilities::AES_NI) {
            priority += 5;
        }

        priority
    }

    /// Generate a human-readable name from capabilities
    fn generate_name(caps: HardwareCapabilities) -> String {
        if caps.is_empty() {
            return "scalar".to_string();
        }

        let mut parts = Vec::new();

        // x86_64
        if caps.contains(HardwareCapabilities::AVX512) {
            parts.push("avx512");
        } else if caps.contains(HardwareCapabilities::AVX2) {
            parts.push("avx2");
        } else if caps.contains(HardwareCapabilities::AVX) {
            parts.push("avx");
        } else if caps.contains(HardwareCapabilities::SSE4_2) {
            parts.push("sse4.2");
        }

        // AArch64
        if caps.contains(HardwareCapabilities::SME) {
            parts.push("sme");
        } else if caps.contains(HardwareCapabilities::SVE2) {
            parts.push("sve2");
        } else if caps.contains(HardwareCapabilities::SVE) {
            parts.push("sve");
        } else if caps.contains(HardwareCapabilities::NEON) {
            parts.push("neon");
        }

        if parts.is_empty() {
            "generic".to_string()
        } else {
            parts.join("_")
        }
    }
}

/// A chain of fallback implementations with priorities
#[derive(Debug, Clone)]
pub struct FallbackChain {
    /// Name of this fallback chain
    pub name: String,
    /// Ordered list of requirements (highest priority first)
    pub requirements: Vec<FeatureRequirement>,
}

impl FallbackChain {
    /// Create a new fallback chain
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            requirements: Vec::new(),
        }
    }

    /// Add a requirement to the chain
    pub fn add_requirement(&mut self, req: FeatureRequirement) {
        self.requirements.push(req);
        // Keep sorted by priority (highest first)
        self.requirements
            .sort_by_key(|r| core::cmp::Reverse(r.priority));
    }

    /// Add multiple requirements
    pub fn add_requirements(&mut self, reqs: &[FeatureRequirement]) {
        for req in reqs {
            self.add_requirement(req.clone());
        }
    }

    /// Get the best matching requirement for a profile
    pub fn select_best(&self, profile: &HardwareProfile) -> Option<&FeatureRequirement> {
        self.requirements
            .iter()
            .filter(|req| req.is_satisfied_by(profile))
            .max_by_key(|req| req.satisfaction_score(profile))
    }

    /// Get all satisfied requirements, ordered by score
    pub fn all_satisfied(&self, profile: &HardwareProfile) -> Vec<&FeatureRequirement> {
        let mut satisfied: Vec<_> = self
            .requirements
            .iter()
            .filter(|req| req.is_satisfied_by(profile))
            .collect();
        satisfied.sort_by_key(|req| core::cmp::Reverse(req.satisfaction_score(profile)));
        satisfied
    }
}

/// Runtime selector for optimal code paths
pub struct RuntimeSelector {
    /// Cached hardware profile
    profile: HardwareProfile,
    /// Invalidation counter for cache invalidation
    version: AtomicU64,
    /// Whether profile has been verified
    verified: AtomicBool,
}

impl RuntimeSelector {
    /// Create a new runtime selector
    pub fn new() -> Self {
        Self {
            profile: HardwareProfile::detect(),
            version: AtomicU64::new(0),
            verified: AtomicBool::new(false),
        }
    }

    /// Create a selector with a specific profile (for testing)
    pub fn with_profile(profile: HardwareProfile) -> Self {
        Self {
            profile,
            version: AtomicU64::new(0),
            verified: AtomicBool::new(false),
        }
    }

    /// Refresh hardware profile (for hot-plug scenarios)
    pub fn refresh(&mut self) {
        self.profile = HardwareProfile::detect();
        self.version.fetch_add(1, Ordering::SeqCst);
        self.verified.store(false, Ordering::SeqCst);
    }

    /// Get the current hardware profile
    pub fn profile(&self) -> &HardwareProfile {
        &self.profile
    }

    /// Get the current version counter
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Select the best requirement from a fallback chain
    pub fn select<'a>(&self, chain: &'a FallbackChain) -> Result<&'a FeatureRequirement> {
        chain
            .select_best(&self.profile)
            .ok_or(Error::UnsupportedArchitecture {
                feature: chain.name.clone(),
                architecture: crate::detect_architecture(),
            })
    }

    /// Select with verification (test that features actually work)
    pub fn select_verified<'a>(
        &mut self,
        chain: &'a FallbackChain,
        verify_fn: impl Fn(&FeatureRequirement) -> bool,
    ) -> Result<&'a FeatureRequirement> {
        let candidates = chain.all_satisfied(&self.profile);

        for req in candidates {
            if verify_fn(req) {
                self.verified.store(true, Ordering::SeqCst);
                return Ok(req);
            }
        }

        Err(Error::UnsupportedArchitecture {
            feature: chain.name.clone(),
            architecture: crate::detect_architecture(),
        })
    }

    /// Check if a specific requirement is satisfied
    pub fn is_satisfied(&self, req: &FeatureRequirement) -> bool {
        req.is_satisfied_by(&self.profile)
    }

    /// Get satisfaction score for a requirement
    pub fn score(&self, req: &FeatureRequirement) -> u32 {
        req.satisfaction_score(&self.profile)
    }

    /// Check if the selector has been verified
    pub fn is_verified(&self) -> bool {
        self.verified.load(Ordering::SeqCst)
    }
}

impl Default for RuntimeSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Function pointer type for runtime dispatch
pub type DispatchFn<T> = fn() -> T;

/// Dispatch table for runtime function selection
pub struct DispatchTable<T> {
    /// Name of the dispatch table
    pub name: String,
    /// Fallback chain for selection
    pub chain: FallbackChain,
    /// Function implementations indexed by requirement
    pub functions: Vec<(FeatureRequirement, DispatchFn<T>)>,
    /// Currently selected function
    current: Option<DispatchFn<T>>,
}

impl<T> DispatchTable<T> {
    /// Create a new dispatch table
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            chain: FallbackChain::new(name),
            functions: Vec::new(),
            current: None,
        }
    }

    /// Add a function implementation
    pub fn add_impl(&mut self, req: FeatureRequirement, func: DispatchFn<T>) {
        self.chain.add_requirement(req.clone());
        self.functions.push((req, func));
    }

    /// Select and cache the optimal implementation
    pub fn select(&mut self, selector: &RuntimeSelector) -> Result<()> {
        let best = selector.select(&self.chain)?;

        // Find matching function
        for (req, func) in &self.functions {
            if req == best {
                self.current = Some(*func);
                return Ok(());
            }
        }

        Err(Error::UnsupportedArchitecture {
            feature: self.name.clone(),
            architecture: crate::detect_architecture(),
        })
    }

    /// Call the selected function
    pub fn call(&self) -> Result<T> {
        match self.current {
            Some(func) => Ok(func()),
            None => Err(Error::UnsupportedArchitecture {
                feature: self.name.clone(),
                architecture: crate::detect_architecture(),
            }),
        }
    }

    /// Get the currently selected function
    pub fn current_fn(&self) -> Option<DispatchFn<T>> {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_requirement_none() {
        let req = FeatureRequirement::none();
        assert_eq!(req.required, HardwareCapabilities::empty());
        assert_eq!(req.priority, 0);
        assert_eq!(req.name, "scalar_fallback");
    }

    #[test]
    fn test_feature_requirement_all_of() {
        let req =
            FeatureRequirement::all_of(&[HardwareCapabilities::AVX2, HardwareCapabilities::FMA]);
        assert!(req.required.contains(HardwareCapabilities::AVX2));
        assert!(req.required.contains(HardwareCapabilities::FMA));
        assert!(req.priority > 0);
    }

    #[test]
    fn test_feature_requirement_builders() {
        let req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])
            .with_min_vector_width(256)
            .with_min_cores(4)
            .with_name("avx2_optimized");

        assert_eq!(req.min_vector_width, 256);
        assert_eq!(req.min_cores, 4);
        assert_eq!(req.name, "avx2_optimized");
    }

    #[test]
    fn test_feature_requirement_satisfaction() {
        let req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])
            .with_min_vector_width(256)
            .with_min_cores(2);

        let profile = HardwareProfile::detect();

        // Satisfaction depends on actual hardware
        let satisfied = req.is_satisfied_by(&profile);
        let score = req.satisfaction_score(&profile);

        if satisfied {
            assert!(score > 0);
        } else {
            assert_eq!(score, 0);
        }
    }

    #[test]
    fn test_fallback_chain_creation() {
        let mut chain = FallbackChain::new("test_chain");
        assert_eq!(chain.name, "test_chain");
        assert_eq!(chain.requirements.len(), 0);

        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]));
        chain.add_requirement(FeatureRequirement::none());

        assert_eq!(chain.requirements.len(), 2);
    }

    #[test]
    fn test_fallback_chain_priority_sorting() {
        let mut chain = FallbackChain::new("sorted");

        // Add in reverse priority order
        chain.add_requirement(FeatureRequirement::none()); // Priority 0
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX512])); // High priority
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])); // Medium priority

        // Should be sorted by priority (highest first)
        assert!(chain.requirements[0].priority > chain.requirements[1].priority);
        assert!(chain.requirements[1].priority > chain.requirements[2].priority);
    }

    #[test]
    fn test_fallback_chain_selection() {
        let mut chain = FallbackChain::new("simd_ops");
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]));
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]));
        chain.add_requirement(FeatureRequirement::none());

        let profile = HardwareProfile::detect();
        let selected = chain.select_best(&profile);

        // Should always find at least the scalar fallback
        assert!(selected.is_some());
    }

    #[test]
    fn test_runtime_selector_creation() {
        let selector = RuntimeSelector::new();
        assert!(selector.version() == 0);
        assert!(!selector.is_verified());
    }

    #[test]
    fn test_runtime_selector_refresh() {
        let mut selector = RuntimeSelector::new();
        let v1 = selector.version();
        selector.refresh();
        let v2 = selector.version();
        assert_eq!(v2, v1 + 1);
    }

    #[test]
    fn test_runtime_selector_select() {
        let mut chain = FallbackChain::new("test");
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]));
        chain.add_requirement(FeatureRequirement::none());

        let selector = RuntimeSelector::new();
        let result = selector.select(&chain);

        // Should always succeed with fallback
        assert!(result.is_ok());
    }

    #[test]
    fn test_runtime_selector_is_satisfied() {
        let selector = RuntimeSelector::new();
        let scalar = FeatureRequirement::none();

        assert!(selector.is_satisfied(&scalar));
    }

    #[test]
    fn test_runtime_selector_score() {
        let selector = RuntimeSelector::new();
        let scalar = FeatureRequirement::none();
        let score = selector.score(&scalar);

        // Scalar should always have a defined score
        let _ = score; // Just verify it doesn't panic
    }

    #[test]
    fn test_dispatch_table_creation() {
        let table: DispatchTable<i32> = DispatchTable::new("add");
        assert_eq!(table.name, "add");
        assert_eq!(table.functions.len(), 0);
        assert!(table.current.is_none());
    }

    #[test]
    fn test_dispatch_table_add_impl() {
        let mut table: DispatchTable<i32> = DispatchTable::new("add");

        fn scalar_add() -> i32 {
            42
        }

        table.add_impl(FeatureRequirement::none(), scalar_add);
        assert_eq!(table.functions.len(), 1);
        assert_eq!(table.chain.requirements.len(), 1);
    }

    #[test]
    fn test_dispatch_table_select_and_call() {
        let mut table: DispatchTable<i32> = DispatchTable::new("compute");

        fn scalar_impl() -> i32 {
            100
        }

        table.add_impl(FeatureRequirement::none(), scalar_impl);

        let selector = RuntimeSelector::new();
        let select_result = table.select(&selector);
        assert!(select_result.is_ok());

        let call_result = table.call();
        assert!(call_result.is_ok());
        assert_eq!(call_result.expect("call should return Ok"), 100);
    }

    #[test]
    fn test_dispatch_table_multiple_impls() {
        let mut table: DispatchTable<&'static str> = DispatchTable::new("backend");

        fn scalar() -> &'static str {
            "scalar"
        }
        fn vector() -> &'static str {
            "vector"
        }

        table.add_impl(FeatureRequirement::none(), scalar);
        table.add_impl(
            FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]),
            vector,
        );

        let selector = RuntimeSelector::new();
        assert!(table.select(&selector).is_ok());

        let result = table.call().expect("call should return Ok");
        assert!(result == "scalar" || result == "vector");
    }

    #[test]
    fn test_feature_requirement_excluded_caps() {
        let req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])
            .with_excluded(HardwareCapabilities::AVX512);

        assert_eq!(req.excluded, HardwareCapabilities::AVX512);
    }

    #[test]
    fn test_priority_calculation() {
        let avx512_req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]);
        let avx2_req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]);
        let scalar_req = FeatureRequirement::none();

        assert!(avx512_req.priority > avx2_req.priority);
        assert!(avx2_req.priority > scalar_req.priority);
    }

    #[test]
    fn test_name_generation() {
        let req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]);
        assert!(req.name.contains("avx2"));

        let scalar = FeatureRequirement::none();
        assert_eq!(scalar.name, "scalar_fallback");
    }

    #[test]
    fn test_fallback_chain_all_satisfied() {
        let mut chain = FallbackChain::new("test");
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]));
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]));
        chain.add_requirement(FeatureRequirement::none());

        let profile = HardwareProfile::detect();
        let satisfied = chain.all_satisfied(&profile);

        // At least scalar should be satisfied
        assert!(!satisfied.is_empty());
    }

    #[test]
    fn test_selector_with_custom_profile() {
        let mut profile = HardwareProfile::detect();
        profile.core_count = 16;

        let selector = RuntimeSelector::with_profile(profile);
        assert_eq!(selector.profile().core_count, 16);
    }
}
