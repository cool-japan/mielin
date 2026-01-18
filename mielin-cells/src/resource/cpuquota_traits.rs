//! # CpuQuota - Trait Implementations
//!
//! This module contains trait implementations for `CpuQuota`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::CpuQuota;

impl Default for CpuQuota {
    fn default() -> Self {
        Self {
            max_execution_time_us: 10_000_000,
            max_time_per_second_us: 500_000,
            max_instructions: u64::MAX,
            priority: 50,
        }
    }
}
