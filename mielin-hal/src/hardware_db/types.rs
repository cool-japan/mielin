//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::capabilities::HardwareCapabilities;
use crate::Architecture;
use alloc::string::String;


/// Known processor configuration
#[derive(Debug, Clone)]
pub struct ProcessorSpec {
    /// Processor name
    pub name: String,
    /// Vendor
    pub vendor: String,
    /// Architecture
    pub architecture: Architecture,
    /// Expected capabilities
    pub capabilities: HardwareCapabilities,
    /// Core count (physical)
    pub physical_cores: usize,
    /// Thread count (logical cores)
    pub logical_cores: usize,
    /// Base frequency in MHz
    pub base_freq_mhz: u32,
    /// Max frequency in MHz
    pub max_freq_mhz: u32,
    /// L1 data cache per core
    pub l1d_per_core_kb: usize,
    /// L2 cache per core
    pub l2_per_core_kb: usize,
    /// L3 cache total
    pub l3_total_kb: usize,
    /// Cache line size
    pub cache_line_bytes: usize,
    /// TDP in watts
    pub tdp_watts: u32,
}
impl ProcessorSpec {
    /// Check if current hardware matches this specification
    pub fn matches_current(&self) -> bool {
        let profile = crate::capabilities::HardwareProfile::detect();
        profile.architecture == self.architecture
            && profile.core_count >= self.physical_cores
            && profile.capabilities.contains(self.capabilities)
    }
    /// Calculate cache hierarchy score (higher is better)
    pub fn cache_score(&self) -> f32 {
        let l1_score = self.l1d_per_core_kb as f32;
        let l2_score = self.l2_per_core_kb as f32 * 0.5;
        let l3_score = (self.l3_total_kb / self.physical_cores.max(1)) as f32 * 0.25;
        l1_score + l2_score + l3_score
    }
    /// Calculate performance score (arbitrary units for comparison)
    pub fn performance_score(&self) -> f32 {
        let freq_score = (self.max_freq_mhz as f32 / 1000.0) * self.logical_cores as f32;
        let cache_score = self.cache_score();
        let capability_score = self.capabilities.bits().count_ones() as f32 * 10.0;
        freq_score + cache_score * 0.1 + capability_score
    }
}
