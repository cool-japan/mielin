//! # BatteryHealth - Trait Implementations
//!
//! This module contains trait implementations for `BatteryHealth`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::{BatteryChemistry, BatteryHealth};

impl Default for BatteryHealth {
    fn default() -> Self {
        Self::new(BatteryChemistry::LithiumIon)
    }
}
