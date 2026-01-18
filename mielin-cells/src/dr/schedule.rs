//! Backup Scheduling Module

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScheduleError {
    #[error("Schedule error: {0}")]
    ScheduleFailed(String),
}

pub type ScheduleResult<T> = Result<T, ScheduleError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub frequency: Duration,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPolicy {
    pub full_backup_schedule: BackupSchedule,
    pub incremental_schedule: Option<BackupSchedule>,
}

#[derive(Debug, Clone)]
pub struct BackupRetention {
    pub daily_retention: u32,
    pub weekly_retention: u32,
    pub monthly_retention: u32,
}

#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    pub policy: BackupPolicy,
    pub retention: BackupRetention,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            policy: BackupPolicy {
                full_backup_schedule: BackupSchedule {
                    frequency: Duration::from_secs(86400),
                    retention_days: 30,
                },
                incremental_schedule: Some(BackupSchedule {
                    frequency: Duration::from_secs(3600),
                    retention_days: 7,
                }),
            },
            retention: BackupRetention {
                daily_retention: 7,
                weekly_retention: 4,
                monthly_retention: 12,
            },
        }
    }
}

pub struct BackupScheduler {
    #[allow(dead_code)]
    config: ScheduleConfig,
}

impl BackupScheduler {
    pub fn new(config: ScheduleConfig) -> Self {
        Self { config }
    }
}
