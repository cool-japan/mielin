//! # PoolConfigError - Trait Implementations
//!
//! This module contains trait implementations for `PoolConfigError`.
//!
//! ## Implemented Traits
//!
//! - `Display`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::PoolConfigError;

impl core::fmt::Display for PoolConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PoolConfigError::NoPoolsConfigured => {
                write!(f, "At least one pool must have blocks configured")
            }
            PoolConfigError::InvalidThreshold => {
                write!(f, "Fragmentation threshold must be between 0 and 100")
            }
        }
    }
}
