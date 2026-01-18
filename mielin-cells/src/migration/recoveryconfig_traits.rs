//! # RecoveryConfig - Trait Implementations
//!
//! This module contains trait implementations for `RecoveryConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::RecoveryConfig;

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay_secs: 5,
            max_retry_delay_secs: 60,
            use_exponential_backoff: true,
            recovery_timeout_secs: 300,
            auto_rollback: true,
        }
    }
}
