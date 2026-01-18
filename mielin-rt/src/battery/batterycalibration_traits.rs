//! # BatteryCalibration - Trait Implementations
//!
//! This module contains trait implementations for `BatteryCalibration`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::{BatteryCalibration, BatteryChemistry};

impl Default for BatteryCalibration {
    fn default() -> Self {
        Self::new(BatteryChemistry::LithiumIon, 1)
    }
}
