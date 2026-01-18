//! # HistoryConfig - Trait Implementations
//!
//! This module contains trait implementations for `HistoryConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::HistoryConfig;

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_snapshots: 1000,
            min_interval_us: 1_000_000,
        }
    }
}
