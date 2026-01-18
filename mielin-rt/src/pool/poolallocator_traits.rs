//! # PoolAllocator - Trait Implementations
//!
//! This module contains trait implementations for `PoolAllocator`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::{PoolAllocator, PoolConfig};

impl Default for PoolAllocator {
    fn default() -> Self {
        let mut allocator = Self::new(PoolConfig::default());
        allocator.init();
        allocator
    }
}
