//! # ResourceQuota - Trait Implementations
//!
//! This module contains trait implementations for `ResourceQuota`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::{CpuQuota, MemoryQuota, NetworkQuota, ResourceQuota, StorageQuota};

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            memory: MemoryQuota::default(),
            cpu: CpuQuota::default(),
            network: NetworkQuota::default(),
            storage: StorageQuota::default(),
            enforced: true,
        }
    }
}
