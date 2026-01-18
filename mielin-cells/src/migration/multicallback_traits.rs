//! # MultiCallback - Trait Implementations
//!
//! This module contains trait implementations for `MultiCallback`.
//!
//! ## Implemented Traits
//!
//! - `MigrationProgressCallback`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::functions::MigrationProgressCallback;
use super::types::{MigrationProgressEvent, MultiCallback};

impl MigrationProgressCallback for MultiCallback {
    fn on_progress(&mut self, event: MigrationProgressEvent) {
        for callback in &mut self.callbacks {
            callback.on_progress(event.clone());
        }
    }
}
