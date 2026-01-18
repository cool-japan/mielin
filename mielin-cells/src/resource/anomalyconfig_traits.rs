//! # AnomalyConfig - Trait Implementations
//!
//! This module contains trait implementations for `AnomalyConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::AnomalyConfig;

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            memory_spike_threshold: 2.0,
            cpu_spike_threshold: 3.0,
            network_spike_threshold: 5.0,
            storage_spike_threshold: 2.0,
            leak_detection_samples: 10,
            leak_growth_threshold: 0.95,
        }
    }
}
