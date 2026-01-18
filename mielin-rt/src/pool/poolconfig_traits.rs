//! # PoolConfig - Trait Implementations
//!
//! This module contains trait implementations for `PoolConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::functions::DEFAULT_BLOCKS_PER_POOL;
use super::types::PoolConfig;

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            blocks_per_pool: [
                DEFAULT_BLOCKS_PER_POOL,
                DEFAULT_BLOCKS_PER_POOL,
                DEFAULT_BLOCKS_PER_POOL,
                DEFAULT_BLOCKS_PER_POOL / 2,
                DEFAULT_BLOCKS_PER_POOL / 4,
                DEFAULT_BLOCKS_PER_POOL / 8,
                DEFAULT_BLOCKS_PER_POOL / 16,
            ],
            track_statistics: true,
            track_fragmentation: true,
            fragmentation_threshold: 50,
        }
    }
}
