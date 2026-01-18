//! # PoolError - Trait Implementations
//!
//! This module contains trait implementations for `PoolError`.
//!
//! ## Implemented Traits
//!
//! - `Display`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::PoolError;

impl core::fmt::Display for PoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PoolError::SizeTooLarge => write!(f, "Size too large for any pool"),
            PoolError::PoolExhausted => write!(f, "Pool exhausted"),
            PoolError::InvalidPool => write!(f, "Invalid pool index"),
            PoolError::DoubleFree => write!(f, "Double free detected"),
            PoolError::NotInitialized => write!(f, "Pool not initialized"),
            PoolError::InvalidBlockIndex => write!(f, "Invalid block index"),
            PoolError::InvalidAlignment => write!(f, "Alignment requirements not met"),
            PoolError::ZeroSizeAllocation => write!(f, "Zero-size allocation requested"),
            PoolError::MemoryCorruption => write!(f, "Memory corruption detected"),
            PoolError::InvalidAllocation => {
                write!(f, "Invalid allocation (not from this allocator)")
            }
        }
    }
}
