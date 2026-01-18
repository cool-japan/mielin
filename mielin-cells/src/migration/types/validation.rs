//! Migration validation types

use super::audit::MigrationAuditLog;
use super::delta::DeltaSnapshot;
use super::recovery::RollbackInfo;
use crate::migration::functions::simple_checksum;
use crate::{Agent, CellError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Requirements for an agent to be migrated
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentRequirements {
    /// Required architecture
    pub architecture: Option<String>,
    /// Required memory
    pub required_memory: u64,
    /// Required storage
    pub required_storage: u64,
    /// Required WASM features
    pub required_wasm_features: Vec<String>,
    /// Current state size
    pub state_size: usize,
    /// Agent version
    pub version: String,
}

/// Target node capabilities for compatibility checking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Supported architectures
    pub architectures: Vec<String>,
    /// Available memory in bytes
    pub available_memory: u64,
    /// Available storage in bytes
    pub available_storage: u64,
    /// Supported WASM features
    pub wasm_features: Vec<String>,
    /// Maximum agent state size
    pub max_state_size: usize,
    /// Node version string
    pub version: String,
    /// Minimum compatible version
    pub min_compatible_version: String,
}

impl NodeCapabilities {
    /// Create capabilities with default values
    pub fn new() -> Self {
        Self {
            architectures: vec!["x86_64".to_string(), "aarch64".to_string()],
            available_memory: 8 * 1024 * 1024 * 1024,
            available_storage: 100 * 1024 * 1024 * 1024,
            wasm_features: vec!["bulk-memory".to_string(), "simd".to_string()],
            max_state_size: 1024 * 1024 * 1024,
            version: "0.1.0".to_string(),
            min_compatible_version: "0.1.0".to_string(),
        }
    }
}

/// Compatibility check result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityStatus {
    /// Fully compatible
    Compatible,
    /// Compatible with warnings
    CompatibleWithWarnings(Vec<String>),
    /// Incompatible
    Incompatible(String),
}

impl CompatibilityStatus {
    /// Check if migration can proceed
    pub fn can_proceed(&self) -> bool {
        !matches!(self, Self::Incompatible(_))
    }

    /// Get warnings if any
    pub fn warnings(&self) -> Vec<String> {
        match self {
            Self::CompatibleWithWarnings(warnings) => warnings.clone(),
            _ => vec![],
        }
    }
}

/// Result of post-migration verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification passed
    pub passed: bool,
    /// State checksum matches
    pub checksum_valid: bool,
    /// State size matches
    pub size_valid: bool,
    /// Agent is responsive
    pub agent_responsive: bool,
    /// Verification details
    pub details: Vec<String>,
}

impl VerificationResult {
    /// Create a successful verification result
    pub fn success() -> Self {
        Self {
            passed: true,
            checksum_valid: true,
            size_valid: true,
            agent_responsive: true,
            details: vec!["All verification checks passed".to_string()],
        }
    }

    /// Create a failed verification result
    pub fn failure(reason: &str) -> Self {
        Self {
            passed: false,
            checksum_valid: false,
            size_valid: false,
            agent_responsive: false,
            details: vec![reason.to_string()],
        }
    }
}

/// Pre-migration compatibility validator
#[derive(Debug, Clone)]
pub struct CompatibilityValidator;

impl CompatibilityValidator {
    /// Check if target node can accept the agent
    pub fn check_compatibility(
        requirements: &AgentRequirements,
        capabilities: &NodeCapabilities,
    ) -> CompatibilityStatus {
        let mut warnings = Vec::new();
        if let Some(ref arch) = requirements.architecture {
            if !capabilities.architectures.contains(arch) {
                return CompatibilityStatus::Incompatible(format!(
                    "Target does not support architecture: {}",
                    arch
                ));
            }
        }
        if requirements.required_memory > capabilities.available_memory {
            return CompatibilityStatus::Incompatible(format!(
                "Insufficient memory: required {} bytes, available {} bytes",
                requirements.required_memory, capabilities.available_memory
            ));
        }
        let memory_headroom = capabilities
            .available_memory
            .saturating_sub(requirements.required_memory);
        if memory_headroom < capabilities.available_memory / 5 {
            warnings.push(format!(
                "Low memory headroom: only {} bytes available after migration",
                memory_headroom
            ));
        }
        if requirements.required_storage > capabilities.available_storage {
            return CompatibilityStatus::Incompatible(format!(
                "Insufficient storage: required {} bytes, available {} bytes",
                requirements.required_storage, capabilities.available_storage
            ));
        }
        if requirements.state_size > capabilities.max_state_size {
            return CompatibilityStatus::Incompatible(format!(
                "Agent state too large: {} bytes exceeds maximum {} bytes",
                requirements.state_size, capabilities.max_state_size
            ));
        }
        for feature in &requirements.required_wasm_features {
            if !capabilities.wasm_features.contains(feature) {
                return CompatibilityStatus::Incompatible(format!(
                    "Target does not support WASM feature: {}",
                    feature
                ));
            }
        }
        if !Self::is_version_compatible(&requirements.version, &capabilities.min_compatible_version)
        {
            return CompatibilityStatus::Incompatible(format!(
                "Version incompatible: agent version {} not compatible with target minimum {}",
                requirements.version, capabilities.min_compatible_version
            ));
        }
        if warnings.is_empty() {
            CompatibilityStatus::Compatible
        } else {
            CompatibilityStatus::CompatibleWithWarnings(warnings)
        }
    }

    /// Check if two versions are compatible (simple semver check)
    fn is_version_compatible(agent_version: &str, min_version: &str) -> bool {
        let parse_version = |v: &str| -> (u32, u32, u32) {
            let parts: Vec<&str> = v.split('.').collect();
            let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            (major, minor, patch)
        };
        let agent = parse_version(agent_version);
        let min = parse_version(min_version);
        agent.0 == min.0 && (agent.1 > min.1 || (agent.1 == min.1 && agent.2 >= min.2))
    }
}

/// Post-migration state verifier
#[derive(Debug, Clone)]
pub struct StateVerifier;

impl StateVerifier {
    /// Verify that the migrated state matches the original
    pub fn verify_state(
        original_state: &[u8],
        migrated_state: &[u8],
        original_checksum: u32,
    ) -> VerificationResult {
        let mut result = VerificationResult {
            passed: true,
            checksum_valid: false,
            size_valid: false,
            agent_responsive: true,
            details: Vec::new(),
        };
        if original_state.len() == migrated_state.len() {
            result.size_valid = true;
            result
                .details
                .push(format!("Size verified: {} bytes", original_state.len()));
        } else {
            result.passed = false;
            result.details.push(format!(
                "Size mismatch: original {} bytes, migrated {} bytes",
                original_state.len(),
                migrated_state.len()
            ));
        }
        let migrated_checksum = simple_checksum(migrated_state);
        if migrated_checksum == original_checksum {
            result.checksum_valid = true;
            result.details.push("Checksum verified".to_string());
        } else {
            result.passed = false;
            result.details.push(format!(
                "Checksum mismatch: expected {}, got {}",
                original_checksum, migrated_checksum
            ));
        }
        result
    }

    /// Verify a delta snapshot can be applied correctly
    pub fn verify_delta(
        base_state: &[u8],
        delta: &DeltaSnapshot,
        expected_checksum: u32,
    ) -> VerificationResult {
        let mut result = VerificationResult {
            passed: true,
            checksum_valid: false,
            size_valid: false,
            agent_responsive: true,
            details: Vec::new(),
        };
        if delta.checksum == expected_checksum {
            result.checksum_valid = true;
            result.details.push("Delta checksum verified".to_string());
        } else {
            result.passed = false;
            result.checksum_valid = false;
            result.details.push(format!(
                "Delta checksum mismatch: expected {}, got {}",
                expected_checksum, delta.checksum
            ));
        }
        if delta.total_size == base_state.len() {
            result.size_valid = true;
            result
                .details
                .push("Delta size compatible with base state".to_string());
        } else {
            result.passed = false;
            result.size_valid = false;
            result.details.push(format!(
                "Delta size mismatch: delta expects {} bytes, base has {} bytes",
                delta.total_size,
                base_state.len()
            ));
        }
        result
    }
}

/// Coordinated migration validator that ties everything together
#[derive(Debug)]
pub struct MigrationValidator {
    /// Audit log
    audit_log: MigrationAuditLog,
    /// Active rollback info (keyed by migration_id)
    rollback_info: HashMap<[u8; 16], RollbackInfo>,
    /// Maximum rollback info age in seconds
    rollback_max_age_secs: u64,
}

impl MigrationValidator {
    /// Create a new migration validator
    pub fn new() -> Self {
        Self {
            audit_log: MigrationAuditLog::new(),
            rollback_info: HashMap::new(),
            rollback_max_age_secs: 3600,
        }
    }

    /// Set maximum rollback info age
    pub fn set_rollback_max_age(&mut self, secs: u64) {
        self.rollback_max_age_secs = secs;
    }

    /// Pre-migration validation
    pub fn validate_pre_migration(
        &mut self,
        migration_id: [u8; 16],
        agent: &Agent,
        requirements: &AgentRequirements,
        target_capabilities: &NodeCapabilities,
    ) -> Result<CompatibilityStatus, CellError> {
        let status = CompatibilityValidator::check_compatibility(requirements, target_capabilities);
        self.audit_log
            .log_validation(migration_id, *agent.id().as_bytes(), &status);
        Ok(status)
    }

    /// Capture rollback info before migration
    pub fn capture_rollback_info(
        &mut self,
        migration_id: [u8; 16],
        agent: &Agent,
        state: &[u8],
    ) -> Result<(), CellError> {
        let rollback = RollbackInfo::capture(agent, state)?;
        self.rollback_info.insert(migration_id, rollback);
        self.audit_log.log(super::audit::AuditEntry::new(
            migration_id,
            *agent.id().as_bytes(),
            super::audit::AuditEventType::SnapshotCaptured,
            format!("Rollback info captured: {} bytes", state.len()),
            true,
        ));
        Ok(())
    }

    /// Verify post-migration state
    pub fn verify_post_migration(
        &mut self,
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        original_state: &[u8],
        migrated_state: &[u8],
    ) -> VerificationResult {
        let original_checksum = simple_checksum(original_state);
        let result = StateVerifier::verify_state(original_state, migrated_state, original_checksum);
        self.audit_log
            .log_verification(migration_id, agent_id, &result);
        result
    }

    /// Execute rollback for a failed migration
    pub fn execute_rollback(&mut self, migration_id: [u8; 16]) -> Result<RollbackInfo, CellError> {
        if let Some(info) = self.rollback_info.get(&migration_id) {
            self.audit_log.log_rollback(
                migration_id,
                info.agent_id,
                true,
                true,
                "Rollback initiated".to_string(),
            );
        }
        let info = self.rollback_info.remove(&migration_id).ok_or_else(|| {
            CellError::InvalidState("No rollback info available for migration".to_string())
        })?;
        if !info.is_valid(self.rollback_max_age_secs) {
            self.audit_log.log_rollback(
                migration_id,
                info.agent_id,
                false,
                false,
                format!("Rollback info expired: {} seconds old", info.age_secs()),
            );
            return Err(CellError::InvalidState(
                "Rollback info has expired".to_string(),
            ));
        }
        self.audit_log.log_rollback(
            migration_id,
            info.agent_id,
            false,
            true,
            format!(
                "Rollback completed: restored {} bytes",
                info.original_state.len()
            ),
        );
        Ok(info)
    }

    /// Complete a migration (remove rollback info)
    pub fn complete_migration(&mut self, migration_id: [u8; 16], success: bool, details: String) {
        if let Some(info) = self.rollback_info.remove(&migration_id) {
            self.audit_log
                .log_completion(migration_id, info.agent_id, success, details);
        }
    }

    /// Get the audit log
    pub fn audit_log(&self) -> &MigrationAuditLog {
        &self.audit_log
    }

    /// Get mutable access to audit log
    pub fn audit_log_mut(&mut self) -> &mut MigrationAuditLog {
        &mut self.audit_log
    }

    /// Cleanup expired rollback info
    pub fn cleanup_expired(&mut self) {
        self.rollback_info
            .retain(|_, info| info.is_valid(self.rollback_max_age_secs));
    }
}
