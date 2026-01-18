//! # MemoryQuota - Trait Implementations
//!
//! This module contains trait implementations for `MemoryQuota`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::MemoryQuota;

impl Default for MemoryQuota {
    fn default() -> Self {
        Self {
            max_heap_bytes: 256 * 1024 * 1024,
            max_stack_bytes: 8 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            soft_limit_percent: 80,
        }
    }
}
