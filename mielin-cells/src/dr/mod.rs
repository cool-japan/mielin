//! Disaster Recovery Module
//!
//! Provides disaster recovery features including:
//! - Backup and restore mechanisms
//! - Point-in-time recovery
//! - Backup scheduling and rotation
//! - Recovery strategies
//! - Backup verification and validation

pub mod backup;
pub mod recovery;
pub mod schedule;
pub mod verification;

pub use backup::{
    Backup, BackupConfig, BackupError, BackupManager, BackupMetadata, BackupResult, BackupStorage,
    BackupStrategy, BackupType,
};
pub use recovery::{
    RecoveryConfig, RecoveryError, RecoveryManager, RecoveryPlan, RecoveryPoint, RecoveryResult,
    RecoveryStrategy, RecoveryTarget,
};
pub use schedule::{
    BackupPolicy, BackupRetention, BackupSchedule, BackupScheduler, ScheduleConfig, ScheduleError,
    ScheduleResult,
};
pub use verification::{
    VerificationConfig, VerificationError, VerificationReport, VerificationResult,
    VerificationStatus, VerificationTest, Verifier,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DRError {
    #[error("Backup failed: {0}")]
    BackupFailed(String),
    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    #[error("Schedule error: {0}")]
    ScheduleError(String),
}
