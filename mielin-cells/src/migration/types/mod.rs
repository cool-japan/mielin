//! Migration types module
//!
//! This module contains all types related to agent migration, organized into logical submodules.

pub mod audit;
pub mod callbacks;
pub mod core;
pub mod delta;
pub mod progress;
pub mod recovery;
pub mod validation;

// Re-export all public types
pub use audit::{AuditEntry, AuditEventType, MigrationAuditLog};
pub use callbacks::{CollectingCallback, LoggingCallback, MultiCallback};
pub use core::{MigrationManager, MigrationSnapshot};
pub use delta::{DeltaSnapshot, DirtyPage, DirtyPageTracker, IncrementalMigrator, MigrationStats};
pub use progress::{MigrationPhase, MigrationProgressEvent, MigrationProgressTracker};
pub use recovery::{
    MigrationError, MigrationErrorType, MigrationRecoveryManager, RecoveryAttempt, RecoveryConfig,
    RecoveryStrategy, RollbackInfo,
};
pub use validation::{
    AgentRequirements, CompatibilityStatus, CompatibilityValidator, MigrationValidator,
    NodeCapabilities, StateVerifier, VerificationResult,
};
