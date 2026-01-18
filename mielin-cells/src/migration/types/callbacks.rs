//! Migration callback types

use super::progress::MigrationProgressEvent;
use crate::migration::functions::MigrationProgressCallback;

/// Simple callback that collects all events
#[derive(Debug, Clone, Default)]
pub struct CollectingCallback {
    pub(crate) events: Vec<MigrationProgressEvent>,
}

impl CollectingCallback {
    /// Create a new collecting callback
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Get all collected events
    pub fn events(&self) -> &[MigrationProgressEvent] {
        &self.events
    }

    /// Get the number of events collected
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Clear all collected events
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Get the latest event
    pub fn latest_event(&self) -> Option<&MigrationProgressEvent> {
        self.events.last()
    }
}

/// Callback that logs progress to stdout
#[derive(Debug, Clone)]
pub struct LoggingCallback {
    pub(crate) verbose: bool,
}

impl LoggingCallback {
    /// Create a new logging callback
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }
}

/// Multi-callback dispatcher
#[derive(Default)]
pub struct MultiCallback {
    pub(crate) callbacks: Vec<Box<dyn MigrationProgressCallback>>,
}

impl MultiCallback {
    /// Create a new multi-callback dispatcher
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    /// Add a callback
    pub fn add<C: MigrationProgressCallback + 'static>(&mut self, callback: C) {
        self.callbacks.push(Box::new(callback));
    }

    /// Get the number of registered callbacks
    pub fn callback_count(&self) -> usize {
        self.callbacks.len()
    }
}
