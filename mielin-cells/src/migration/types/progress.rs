//! Migration progress tracking types

use crate::migration::functions::MigrationProgressCallback;
use serde::{Deserialize, Serialize};

/// Migration phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Capturing state
    Capturing,
    /// Transferring data
    Transferring,
    /// Validating state
    Validating,
    /// Restoring state
    Restoring,
    /// Completed
    Completed,
    /// Failed
    Failed,
}

/// Migration progress event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationProgressEvent {
    /// Migration started
    Started {
        /// Agent ID being migrated
        agent_id: [u8; 16],
        /// Total size to transfer in bytes
        total_bytes: usize,
    },
    /// State capture in progress
    CapturingState {
        /// Agent ID
        agent_id: [u8; 16],
        /// Percentage complete (0-100)
        percent: u8,
    },
    /// Data transfer in progress
    TransferProgress {
        /// Agent ID
        agent_id: [u8; 16],
        /// Bytes transferred so far
        transferred_bytes: usize,
        /// Total bytes to transfer
        total_bytes: usize,
        /// Percentage complete (0-100)
        percent: u8,
    },
    /// Validation in progress
    Validating {
        /// Agent ID
        agent_id: [u8; 16],
        /// Validation step name
        step: String,
    },
    /// Restoration in progress
    Restoring {
        /// Agent ID
        agent_id: [u8; 16],
        /// Percentage complete (0-100)
        percent: u8,
    },
    /// Migration completed successfully
    Completed {
        /// Agent ID
        agent_id: [u8; 16],
        /// Total time taken in milliseconds
        duration_ms: u64,
        /// Total bytes transferred
        bytes_transferred: usize,
    },
    /// Migration failed
    Failed {
        /// Agent ID
        agent_id: [u8; 16],
        /// Error message
        error: String,
    },
}

impl MigrationProgressEvent {
    /// Get the agent ID for this event
    pub fn agent_id(&self) -> [u8; 16] {
        match self {
            Self::Started { agent_id, .. }
            | Self::CapturingState { agent_id, .. }
            | Self::TransferProgress { agent_id, .. }
            | Self::Validating { agent_id, .. }
            | Self::Restoring { agent_id, .. }
            | Self::Completed { agent_id, .. }
            | Self::Failed { agent_id, .. } => *agent_id,
        }
    }

    /// Get the progress percentage if applicable
    pub fn progress_percent(&self) -> Option<u8> {
        match self {
            Self::CapturingState { percent, .. }
            | Self::TransferProgress { percent, .. }
            | Self::Restoring { percent, .. } => Some(*percent),
            _ => None,
        }
    }

    /// Check if this is a terminal event (Completed or Failed)
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. })
    }
}

/// Progress tracker for a migration operation
pub struct MigrationProgressTracker {
    /// Agent ID being tracked
    agent_id: [u8; 16],
    /// Total bytes to transfer
    total_bytes: usize,
    /// Bytes transferred so far
    transferred_bytes: usize,
    /// Start time
    start_time: std::time::Instant,
    /// Current phase
    phase: MigrationPhase,
    /// Callback
    callback: Option<Box<dyn MigrationProgressCallback>>,
}

impl MigrationProgressTracker {
    /// Create a new progress tracker
    pub fn new(agent_id: [u8; 16], total_bytes: usize) -> Self {
        Self {
            agent_id,
            total_bytes,
            transferred_bytes: 0,
            start_time: std::time::Instant::now(),
            phase: MigrationPhase::Capturing,
            callback: None,
        }
    }

    /// Set the progress callback
    pub fn with_callback<C: MigrationProgressCallback + 'static>(mut self, callback: C) -> Self {
        self.callback = Some(Box::new(callback));
        self
    }

    /// Report that migration has started
    pub fn report_started(&mut self) {
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::Started {
                agent_id: self.agent_id,
                total_bytes: self.total_bytes,
            });
        }
    }

    /// Update capturing progress
    pub fn update_capturing_progress(&mut self, percent: u8) {
        self.phase = MigrationPhase::Capturing;
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::CapturingState {
                agent_id: self.agent_id,
                percent,
            });
        }
    }

    /// Update transfer progress
    pub fn update_transfer_progress(&mut self, transferred_bytes: usize) {
        self.phase = MigrationPhase::Transferring;
        self.transferred_bytes = transferred_bytes;
        let percent = if self.total_bytes > 0 {
            ((transferred_bytes as f64 / self.total_bytes as f64) * 100.0) as u8
        } else {
            100
        };
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::TransferProgress {
                agent_id: self.agent_id,
                transferred_bytes,
                total_bytes: self.total_bytes,
                percent,
            });
        }
    }

    /// Report validation step
    pub fn report_validating(&mut self, step: String) {
        self.phase = MigrationPhase::Validating;
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::Validating {
                agent_id: self.agent_id,
                step,
            });
        }
    }

    /// Update restoration progress
    pub fn update_restoring_progress(&mut self, percent: u8) {
        self.phase = MigrationPhase::Restoring;
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::Restoring {
                agent_id: self.agent_id,
                percent,
            });
        }
    }

    /// Report completion
    pub fn report_completed(&mut self) {
        self.phase = MigrationPhase::Completed;
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::Completed {
                agent_id: self.agent_id,
                duration_ms,
                bytes_transferred: self.transferred_bytes,
            });
        }
    }

    /// Report failure
    pub fn report_failed(&mut self, error: String) {
        self.phase = MigrationPhase::Failed;
        if let Some(ref mut callback) = self.callback {
            callback.on_progress(MigrationProgressEvent::Failed {
                agent_id: self.agent_id,
                error,
            });
        }
    }

    /// Get the current phase
    pub fn phase(&self) -> MigrationPhase {
        self.phase
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Get current progress percentage
    pub fn progress_percent(&self) -> u8 {
        if self.total_bytes == 0 {
            return 100;
        }
        ((self.transferred_bytes as f64 / self.total_bytes as f64) * 100.0) as u8
    }
}
