//! # BatteryConfig - Trait Implementations
//!
//! This module contains trait implementations for `BatteryConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::BatteryConfig;

impl Default for BatteryConfig {
    fn default() -> Self {
        Self::single_cell_lipo(1000)
    }
}
