//! # FuelGaugeReading - Trait Implementations
//!
//! This module contains trait implementations for `FuelGaugeReading`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::FuelGaugeReading;

impl Default for FuelGaugeReading {
    fn default() -> Self {
        Self {
            soc_percent: 50,
            remaining_mah: 0,
            full_capacity_mah: 0,
            design_capacity_mah: 0,
            voltage_mv: 3700,
            current_ma: 0,
            temperature_deci_c: 250,
            time_to_empty_min: 0,
            time_to_full_min: 0,
        }
    }
}
