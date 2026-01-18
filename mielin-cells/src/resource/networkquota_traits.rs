//! # NetworkQuota - Trait Implementations
//!
//! This module contains trait implementations for `NetworkQuota`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::NetworkQuota;

impl Default for NetworkQuota {
    fn default() -> Self {
        Self {
            max_send_bytes_per_sec: 10 * 1024 * 1024,
            max_recv_bytes_per_sec: 10 * 1024 * 1024,
            max_total_bytes_per_sec: 20 * 1024 * 1024,
            max_connections: 100,
            max_messages_per_sec: 1000,
        }
    }
}
