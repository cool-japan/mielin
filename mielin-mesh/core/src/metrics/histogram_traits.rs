//! # Histogram - Trait Implementations
//!
//! This module contains trait implementations for `Histogram`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::Histogram;

impl Default for Histogram {
    fn default() -> Self {
        Self::new_latency()
    }
}
