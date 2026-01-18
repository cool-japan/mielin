//! Migration recovery and rollback types

use super::validation::MigrationValidator;
use crate::migration::functions::simple_checksum;
use crate::{Agent, CellError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Classification of migration errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationErrorType {
    /// Transient network error (can retry)
    NetworkTransient,
    /// Permanent network error (cannot retry)
    NetworkPermanent,
    /// Insufficient resources on target (can retry on different target)
    InsufficientResources,
    /// Incompatible target (cannot retry on same target)
    IncompatibleTarget,
    /// State corruption during transfer
    StateCorruption,
    /// Timeout during migration
    Timeout,
    /// Target node unreachable
    TargetUnreachable,
    /// Unknown error
    Unknown,
}

impl MigrationErrorType {
    /// Check if this error type is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkTransient
                | Self::InsufficientResources
                | Self::Timeout
                | Self::TargetUnreachable
        )
    }

    /// Check if a different target should be selected
    pub fn should_change_target(&self) -> bool {
        matches!(
            self,
            Self::InsufficientResources | Self::IncompatibleTarget | Self::TargetUnreachable
        )
    }
}

/// Detailed migration error with recovery information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationError {
    /// Error type classification
    pub error_type: MigrationErrorType,
    /// Human-readable error message
    pub message: String,
    /// Original agent ID
    pub agent_id: [u8; 16],
    /// Target node that failed
    pub target_node: Option<[u8; 16]>,
    /// Timestamp of error
    pub timestamp: u64,
    /// Number of retry attempts so far
    pub retry_count: u32,
}

impl MigrationError {
    /// Create a new migration error
    pub fn new(
        error_type: MigrationErrorType,
        message: String,
        agent_id: [u8; 16],
        target_node: Option<[u8; 16]>,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            error_type,
            message,
            agent_id,
            target_node,
            timestamp,
            retry_count: 0,
        }
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Get age of this error in seconds
    pub fn age_secs(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.timestamp)
    }
}

/// Recovery strategy for failed migrations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Retry on the same target
    RetryOnSameTarget,
    /// Retry on a different target
    RetryOnDifferentTarget,
    /// Rollback to original state
    Rollback,
    /// Give up (manual intervention required)
    GiveUp,
}

/// Recovery attempt record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttempt {
    /// Attempt number (1-based)
    pub attempt_number: u32,
    /// Strategy used for this attempt
    pub strategy: RecoveryStrategy,
    /// Timestamp of attempt
    pub timestamp: u64,
    /// Target node for this attempt
    pub target_node: Option<[u8; 16]>,
    /// Whether the attempt succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Configuration for recovery behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial retry delay in seconds
    pub initial_retry_delay_secs: u32,
    /// Maximum retry delay in seconds
    pub max_retry_delay_secs: u32,
    /// Whether to use exponential backoff
    pub use_exponential_backoff: bool,
    /// Timeout for recovery operations in seconds
    pub recovery_timeout_secs: u32,
    /// Whether to auto-rollback on permanent failures
    pub auto_rollback: bool,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay_secs: 10,
            max_retry_delay_secs: 300,
            use_exponential_backoff: true,
            recovery_timeout_secs: 600,
            auto_rollback: true,
        }
    }
}

impl RecoveryConfig {
    /// Calculate retry delay based on attempt number
    pub fn calculate_retry_delay(&self, attempt: u32) -> u32 {
        if !self.use_exponential_backoff {
            return self.initial_retry_delay_secs;
        }
        let delay = self.initial_retry_delay_secs * 2_u32.pow(attempt.saturating_sub(1));
        delay.min(self.max_retry_delay_secs)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.initial_retry_delay_secs == 0 {
            return Err("initial_retry_delay_secs must be > 0".to_string());
        }
        if self.max_retry_delay_secs < self.initial_retry_delay_secs {
            return Err("max_retry_delay_secs must be >= initial_retry_delay_secs".to_string());
        }
        if self.recovery_timeout_secs == 0 {
            return Err("recovery_timeout_secs must be > 0".to_string());
        }
        Ok(())
    }
}

/// Rollback information for failed migrations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackInfo {
    /// Agent ID
    pub agent_id: [u8; 16],
    /// Original state snapshot
    pub original_state: Vec<u8>,
    /// Original state checksum
    pub original_checksum: u32,
    /// Original policy (serialized)
    pub original_policy_data: Vec<u8>,
    /// Timestamp when rollback info was created
    pub created_at: u64,
    /// Source node ID
    pub source_node: Option<[u8; 16]>,
}

impl RollbackInfo {
    /// Create rollback info from an agent's current state
    pub fn capture(agent: &Agent, state: &[u8]) -> Result<Self, CellError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("Time error".to_string()))?
            .as_secs();
        let policy_data = oxicode::encode_to_vec(&oxicode::serde::Compat(agent.policy()))
            .map_err(|e| CellError::InvalidState(format!("Policy serialization failed: {}", e)))?;
        Ok(Self {
            agent_id: *agent.id().as_bytes(),
            original_state: state.to_vec(),
            original_checksum: simple_checksum(state),
            original_policy_data: policy_data,
            created_at: timestamp,
            source_node: None,
        })
    }

    /// Get the age of this rollback info in seconds
    pub fn age_secs(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.created_at)
    }

    /// Check if rollback info is still valid (not too old)
    pub fn is_valid(&self, max_age_secs: u64) -> bool {
        self.age_secs() < max_age_secs
    }
}

/// Migration recovery manager
#[derive(Debug)]
pub struct MigrationRecoveryManager {
    /// Recovery configuration
    config: RecoveryConfig,
    /// Failed migrations pending recovery
    pending_recoveries: HashMap<[u8; 16], MigrationError>,
    /// Recovery attempt history
    recovery_history: HashMap<[u8; 16], Vec<RecoveryAttempt>>,
    /// Migration validator for rollback
    validator: MigrationValidator,
}

impl MigrationRecoveryManager {
    /// Create a new recovery manager
    pub fn new(config: RecoveryConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            pending_recoveries: HashMap::new(),
            recovery_history: HashMap::new(),
            validator: MigrationValidator::new(),
        })
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self {
            config: RecoveryConfig::default(),
            pending_recoveries: HashMap::new(),
            recovery_history: HashMap::new(),
            validator: MigrationValidator::new(),
        }
    }

    /// Record a migration failure
    pub fn record_failure(&mut self, error: MigrationError) {
        self.pending_recoveries
            .insert(error.agent_id, error.clone());
        self.recovery_history.entry(error.agent_id).or_default();
    }

    /// Determine the recovery strategy for a failed migration
    pub fn determine_strategy(&self, error: &MigrationError) -> RecoveryStrategy {
        if error.retry_count >= self.config.max_retries {
            if self.config.auto_rollback {
                return RecoveryStrategy::Rollback;
            } else {
                return RecoveryStrategy::GiveUp;
            }
        }
        match error.error_type {
            MigrationErrorType::NetworkTransient | MigrationErrorType::Timeout => {
                RecoveryStrategy::RetryOnSameTarget
            }
            MigrationErrorType::InsufficientResources
            | MigrationErrorType::IncompatibleTarget
            | MigrationErrorType::TargetUnreachable => RecoveryStrategy::RetryOnDifferentTarget,
            MigrationErrorType::StateCorruption
            | MigrationErrorType::NetworkPermanent
            | MigrationErrorType::Unknown => {
                if self.config.auto_rollback {
                    RecoveryStrategy::Rollback
                } else {
                    RecoveryStrategy::GiveUp
                }
            }
        }
    }

    /// Execute recovery for a failed migration
    pub fn execute_recovery(&mut self, agent_id: &[u8; 16]) -> Result<RecoveryStrategy, String> {
        let (strategy, attempt_number, target_node) = {
            let error = self
                .pending_recoveries
                .get(agent_id)
                .ok_or_else(|| "No pending recovery for agent".to_string())?;
            let strategy = self.determine_strategy(error);
            let attempt_number = error.retry_count + 1;
            let target_node = error.target_node;
            (strategy, attempt_number, target_node)
        };
        let attempt = RecoveryAttempt {
            attempt_number,
            strategy,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            target_node,
            success: false,
            error: None,
        };
        self.recovery_history
            .entry(*agent_id)
            .or_default()
            .push(attempt);
        if let Some(error) = self.pending_recoveries.get_mut(agent_id) {
            error.increment_retry();
        }
        Ok(strategy)
    }

    /// Mark a recovery attempt as successful
    pub fn mark_recovery_success(&mut self, agent_id: &[u8; 16]) {
        self.pending_recoveries.remove(agent_id);
        if let Some(history) = self.recovery_history.get_mut(agent_id) {
            if let Some(last_attempt) = history.last_mut() {
                last_attempt.success = true;
            }
        }
    }

    /// Mark a recovery attempt as failed
    pub fn mark_recovery_failure(&mut self, agent_id: &[u8; 16], error_msg: String) {
        if let Some(history) = self.recovery_history.get_mut(agent_id) {
            if let Some(last_attempt) = history.last_mut() {
                last_attempt.success = false;
                last_attempt.error = Some(error_msg);
            }
        }
    }

    /// Get pending recovery count
    pub fn pending_count(&self) -> usize {
        self.pending_recoveries.len()
    }

    /// Get recovery history for an agent
    pub fn get_history(&self, agent_id: &[u8; 16]) -> Option<&Vec<RecoveryAttempt>> {
        self.recovery_history.get(agent_id)
    }

    /// Get all pending recoveries
    pub fn get_pending(&self) -> Vec<&MigrationError> {
        self.pending_recoveries.values().collect()
    }

    /// Calculate next retry time for an agent
    pub fn next_retry_time(&self, agent_id: &[u8; 16]) -> Option<u64> {
        let error = self.pending_recoveries.get(agent_id)?;
        let delay = self.config.calculate_retry_delay(error.retry_count);
        Some(error.timestamp + u64::from(delay))
    }

    /// Check if an agent is ready for retry
    pub fn is_ready_for_retry(&self, agent_id: &[u8; 16]) -> bool {
        if let Some(next_time) = self.next_retry_time(agent_id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            now >= next_time
        } else {
            false
        }
    }

    /// Get agents ready for retry
    pub fn get_ready_for_retry(&self) -> Vec<[u8; 16]> {
        self.pending_recoveries
            .keys()
            .filter(|&id| self.is_ready_for_retry(id))
            .copied()
            .collect()
    }

    /// Clear recovery history for an agent
    pub fn clear_history(&mut self, agent_id: &[u8; 16]) {
        self.recovery_history.remove(agent_id);
        self.pending_recoveries.remove(agent_id);
    }

    /// Get the recovery configuration
    pub fn config(&self) -> &RecoveryConfig {
        &self.config
    }

    /// Update recovery configuration
    pub fn set_config(&mut self, config: RecoveryConfig) -> Result<(), String> {
        config.validate()?;
        self.config = config;
        Ok(())
    }

    /// Access the migration validator
    pub fn validator(&self) -> &MigrationValidator {
        &self.validator
    }

    /// Access the migration validator mutably
    pub fn validator_mut(&mut self) -> &mut MigrationValidator {
        &mut self.validator
    }
}
