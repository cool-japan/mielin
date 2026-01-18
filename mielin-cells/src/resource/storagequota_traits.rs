//! # StorageQuota - Trait Implementations
//!
//! This module contains trait implementations for `StorageQuota`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::StorageQuota;

impl Default for StorageQuota {
    fn default() -> Self {
        Self {
            max_persistent_bytes: 1024 * 1024 * 1024,
            max_temp_bytes: 256 * 1024 * 1024,
            max_files: 10000,
            max_file_size: 100 * 1024 * 1024,
        }
    }
}
