//! Migration audit logging types

use serde::{Deserialize, Serialize};

use super::validation::CompatibilityStatus;
use super::validation::VerificationResult;

/// Migration audit event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    /// Migration initiated
    MigrationStarted,
    /// Pre-migration validation completed
    ValidationCompleted,
    /// State snapshot captured
    SnapshotCaptured,
    /// State transfer started
    TransferStarted,
    /// State transfer completed
    TransferCompleted,
    /// State verification completed
    VerificationCompleted,
    /// Migration completed successfully
    MigrationCompleted,
    /// Migration failed
    MigrationFailed,
    /// Rollback initiated
    RollbackStarted,
    /// Rollback completed
    RollbackCompleted,
}

/// A single audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique migration ID
    pub migration_id: [u8; 16],
    /// Agent ID
    pub agent_id: [u8; 16],
    /// Event type
    pub event_type: AuditEventType,
    /// Timestamp
    pub timestamp: u64,
    /// Source node
    pub source_node: Option<[u8; 16]>,
    /// Target node
    pub target_node: Option<[u8; 16]>,
    /// Additional details
    pub details: String,
    /// Success/failure indicator
    pub success: bool,
}

impl AuditEntry {
    /// Create a new audit entry
    pub fn new(
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        event_type: AuditEventType,
        details: String,
        success: bool,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            migration_id,
            agent_id,
            event_type,
            timestamp,
            source_node: None,
            target_node: None,
            details,
            success,
        }
    }

    /// Set source and target nodes
    pub fn with_nodes(mut self, source: Option<[u8; 16]>, target: Option<[u8; 16]>) -> Self {
        self.source_node = source;
        self.target_node = target;
        self
    }
}

/// Migration audit logger
#[derive(Debug, Clone, Default)]
pub struct MigrationAuditLog {
    /// Audit entries
    entries: Vec<AuditEntry>,
    /// Maximum entries to keep
    max_entries: usize,
}

impl MigrationAuditLog {
    /// Create a new audit log with default capacity
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 10000,
        }
    }

    /// Create an audit log with custom capacity
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries.min(1000)),
            max_entries,
        }
    }

    /// Add an audit entry
    pub fn log(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            let remove_count = self.entries.len() - self.max_entries;
            self.entries.drain(0..remove_count);
        }
    }

    /// Log a migration start event
    pub fn log_start(
        &mut self,
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        source: Option<[u8; 16]>,
        target: Option<[u8; 16]>,
    ) {
        self.log(
            AuditEntry::new(
                migration_id,
                agent_id,
                AuditEventType::MigrationStarted,
                "Migration initiated".to_string(),
                true,
            )
            .with_nodes(source, target),
        );
    }

    /// Log a migration completion event
    pub fn log_completion(
        &mut self,
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        success: bool,
        details: String,
    ) {
        let event_type = if success {
            AuditEventType::MigrationCompleted
        } else {
            AuditEventType::MigrationFailed
        };
        self.log(AuditEntry::new(
            migration_id,
            agent_id,
            event_type,
            details,
            success,
        ));
    }

    /// Log a validation event
    pub fn log_validation(
        &mut self,
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        status: &CompatibilityStatus,
    ) {
        let (success, details) = match status {
            CompatibilityStatus::Compatible => (true, "Compatibility check passed".to_string()),
            CompatibilityStatus::CompatibleWithWarnings(warnings) => (
                true,
                format!("Compatibility check passed with warnings: {:?}", warnings),
            ),
            CompatibilityStatus::Incompatible(reason) => {
                (false, format!("Incompatible: {}", reason))
            }
        };
        self.log(AuditEntry::new(
            migration_id,
            agent_id,
            AuditEventType::ValidationCompleted,
            details,
            success,
        ));
    }

    /// Log a verification event
    pub fn log_verification(
        &mut self,
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        result: &VerificationResult,
    ) {
        self.log(AuditEntry::new(
            migration_id,
            agent_id,
            AuditEventType::VerificationCompleted,
            result.details.join("; "),
            result.passed,
        ));
    }

    /// Log a rollback event
    pub fn log_rollback(
        &mut self,
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        started: bool,
        success: bool,
        details: String,
    ) {
        let event_type = if started {
            AuditEventType::RollbackStarted
        } else {
            AuditEventType::RollbackCompleted
        };
        self.log(AuditEntry::new(
            migration_id,
            agent_id,
            event_type,
            details,
            success,
        ));
    }

    /// Get all entries for a specific migration
    pub fn get_migration_entries(&self, migration_id: &[u8; 16]) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| &e.migration_id == migration_id)
            .collect()
    }

    /// Get all entries for a specific agent
    pub fn get_agent_entries(&self, agent_id: &[u8; 16]) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| &e.agent_id == agent_id)
            .collect()
    }

    /// Get entries of a specific type
    pub fn get_entries_by_type(&self, event_type: AuditEventType) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// Get failed migrations
    pub fn get_failures(&self) -> Vec<&AuditEntry> {
        self.entries.iter().filter(|e| !e.success).collect()
    }

    /// Get the total number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the log is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get entries within a time range
    pub fn get_entries_in_range(&self, start: u64, end: u64) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }
}
