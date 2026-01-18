//! # CollectingCallback - Trait Implementations
//!
//! This module contains trait implementations for `CollectingCallback`.
//!
//! ## Implemented Traits
//!
//! - `MigrationProgressCallback`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::functions::MigrationProgressCallback;
use super::types::{CollectingCallback, MigrationProgressEvent};

impl MigrationProgressCallback for CollectingCallback {
    fn on_progress(&mut self, event: MigrationProgressEvent) {
        self.events.push(event);
    }
}
