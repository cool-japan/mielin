//! Agent Versioning System
//!
//! Provides semantic versioning for agents, version-aware migration,
//! rolling updates, A/B testing, and canary deployments.

use crate::{AgentId, Dna};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Semantic version for agents
///
/// Follows semantic versioning specification (major.minor.patch)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Check if this version is compatible with another version
    /// Compatible if major version matches and minor version is >= target
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Check if this is a breaking change from another version
    pub fn is_breaking_change(&self, other: &Version) -> bool {
        self.major != other.major
    }

    /// Check if this is a minor update from another version
    pub fn is_minor_update(&self, other: &Version) -> bool {
        self.major == other.major && self.minor != other.minor
    }

    /// Check if this is a patch update from another version
    pub fn is_patch_update(&self, other: &Version) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch != other.patch
    }

    /// Get the next major version
    pub fn next_major(&self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }

    /// Get the next minor version
    pub fn next_minor(&self) -> Self {
        Self::new(self.major, self.minor + 1, 0)
    }

    /// Get the next patch version
    pub fn next_patch(&self) -> Self {
        Self::new(self.major, self.minor, self.patch + 1)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                other => other,
            },
            other => other,
        }
    }
}

/// Version-related errors
#[derive(Debug, Error)]
pub enum VersionError {
    #[error("Incompatible version: {0} is not compatible with {1}")]
    IncompatibleVersion(Version, Version),
    #[error("Version not found: {0}")]
    VersionNotFound(Version),
    #[error("Agent version not found: {0}")]
    AgentVersionNotFound(AgentId),
    #[error("Invalid version string: {0}")]
    InvalidVersionString(String),
    #[error("Deployment error: {0}")]
    DeploymentError(String),
    #[error("Migration error: {0}")]
    MigrationError(String),
}

/// Metadata for a versioned agent DNA
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub version: Version,
    pub dna: Dna,
    pub description: String,
    pub changelog: Vec<String>,
    pub deprecated: bool,
    pub migration_path: Option<Vec<Version>>, // Required intermediate versions for migration
}

impl VersionMetadata {
    pub fn new(version: Version, dna: Dna, description: String) -> Self {
        Self {
            version,
            dna,
            description,
            changelog: Vec::new(),
            deprecated: false,
            migration_path: None,
        }
    }

    pub fn with_changelog(mut self, changelog: Vec<String>) -> Self {
        self.changelog = changelog;
        self
    }

    pub fn with_migration_path(mut self, path: Vec<Version>) -> Self {
        self.migration_path = Some(path);
        self
    }

    pub fn deprecate(mut self) -> Self {
        self.deprecated = true;
        self
    }
}

/// Registry for managing agent versions
pub struct VersionRegistry {
    versions: Arc<RwLock<HashMap<Version, VersionMetadata>>>,
    agent_versions: Arc<RwLock<HashMap<AgentId, Version>>>,
}

impl VersionRegistry {
    pub fn new() -> Self {
        Self {
            versions: Arc::new(RwLock::new(HashMap::new())),
            agent_versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new version
    pub fn register_version(&self, metadata: VersionMetadata) -> Result<(), VersionError> {
        let mut versions = self.versions.write().expect("Lock poisoned: versions");
        versions.insert(metadata.version, metadata);
        Ok(())
    }

    /// Get version metadata
    pub fn get_version(&self, version: &Version) -> Result<VersionMetadata, VersionError> {
        let versions = self.versions.read().expect("Lock poisoned: versions");
        versions
            .get(version)
            .cloned()
            .ok_or(VersionError::VersionNotFound(*version))
    }

    /// List all registered versions
    pub fn list_versions(&self) -> Vec<Version> {
        let versions = self.versions.read().expect("Lock poisoned: versions");
        let mut result: Vec<Version> = versions.keys().copied().collect();
        result.sort();
        result
    }

    /// Get latest version
    pub fn latest_version(&self) -> Option<Version> {
        let versions = self.versions.read().expect("Lock poisoned: versions");
        versions.keys().max().copied()
    }

    /// Register an agent with a version
    pub fn register_agent(&self, agent_id: AgentId, version: Version) -> Result<(), VersionError> {
        // Verify version exists
        self.get_version(&version)?;

        let mut agent_versions = self
            .agent_versions
            .write()
            .expect("Lock poisoned: agent_versions");
        agent_versions.insert(agent_id, version);
        Ok(())
    }

    /// Get agent's version
    pub fn get_agent_version(&self, agent_id: &AgentId) -> Result<Version, VersionError> {
        let agent_versions = self
            .agent_versions
            .read()
            .expect("Lock poisoned: agent_versions");
        agent_versions
            .get(agent_id)
            .copied()
            .ok_or(VersionError::AgentVersionNotFound(*agent_id))
    }

    /// Update agent's version
    pub fn update_agent_version(
        &self,
        agent_id: AgentId,
        new_version: Version,
    ) -> Result<Version, VersionError> {
        // Verify new version exists
        self.get_version(&new_version)?;

        let mut agent_versions = self
            .agent_versions
            .write()
            .expect("Lock poisoned: agent_versions");
        let old_version = agent_versions
            .get(&agent_id)
            .copied()
            .ok_or(VersionError::AgentVersionNotFound(agent_id))?;

        agent_versions.insert(agent_id, new_version);
        Ok(old_version)
    }

    /// Get agents by version
    pub fn get_agents_by_version(&self, version: &Version) -> Vec<AgentId> {
        let agent_versions = self
            .agent_versions
            .read()
            .expect("Lock poisoned: agent_versions");
        agent_versions
            .iter()
            .filter(|(_, v)| *v == version)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Check if migration is possible between versions
    pub fn can_migrate(&self, from: &Version, to: &Version) -> Result<bool, VersionError> {
        let _from_meta = self.get_version(from)?;
        let to_meta = self.get_version(to)?;

        // Can't migrate to deprecated version
        if to_meta.deprecated {
            return Ok(false);
        }

        // Check compatibility
        if to.is_compatible_with(from) {
            return Ok(true);
        }

        // Check if there's a migration path
        if let Some(path) = &to_meta.migration_path {
            Ok(path.contains(from))
        } else {
            Ok(false)
        }
    }
}

impl Default for VersionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Rolling update strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollingUpdateStrategy {
    /// Update all agents at once
    Immediate,
    /// Update agents in batches
    Batched {
        batch_size: usize,
        delay_between_batches_ms: u64,
    },
    /// Update one agent at a time
    Sequential { delay_between_agents_ms: u64 },
    /// Update based on percentage of total agents
    Percentage {
        percentage: f32,
        delay_between_batches_ms: u64,
    },
}

/// Rolling update configuration
#[derive(Debug, Clone)]
pub struct RollingUpdateConfig {
    pub from_version: Version,
    pub to_version: Version,
    pub strategy: RollingUpdateStrategy,
    pub health_check_required: bool,
    pub rollback_on_failure: bool,
    pub max_failures: usize,
}

impl RollingUpdateConfig {
    pub fn new(from_version: Version, to_version: Version) -> Self {
        Self {
            from_version,
            to_version,
            strategy: RollingUpdateStrategy::Batched {
                batch_size: 10,
                delay_between_batches_ms: 1000,
            },
            health_check_required: true,
            rollback_on_failure: true,
            max_failures: 0,
        }
    }

    pub fn with_strategy(mut self, strategy: RollingUpdateStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_health_check(mut self, required: bool) -> Self {
        self.health_check_required = required;
        self
    }

    pub fn with_rollback(mut self, enabled: bool) -> Self {
        self.rollback_on_failure = enabled;
        self
    }

    pub fn with_max_failures(mut self, max: usize) -> Self {
        self.max_failures = max;
        self
    }
}

/// A/B testing configuration
#[derive(Debug, Clone)]
pub struct ABTestConfig {
    pub version_a: Version,
    pub version_b: Version,
    pub traffic_split: f32, // Percentage of traffic to version B (0.0-1.0)
    pub duration_seconds: Option<u64>,
    pub metrics: Vec<String>, // Metrics to track
}

impl ABTestConfig {
    pub fn new(version_a: Version, version_b: Version, traffic_split: f32) -> Self {
        Self {
            version_a,
            version_b,
            traffic_split: traffic_split.clamp(0.0, 1.0),
            duration_seconds: None,
            metrics: Vec::new(),
        }
    }

    pub fn with_duration(mut self, seconds: u64) -> Self {
        self.duration_seconds = Some(seconds);
        self
    }

    pub fn with_metrics(mut self, metrics: Vec<String>) -> Self {
        self.metrics = metrics;
        self
    }
}

/// Canary deployment configuration
#[derive(Debug, Clone)]
pub struct CanaryConfig {
    pub stable_version: Version,
    pub canary_version: Version,
    pub canary_percentage: f32, // Initial percentage for canary (0.0-1.0)
    pub increment_percentage: f32, // How much to increase each step
    pub increment_interval_seconds: u64,
    pub success_threshold: f32, // Success rate threshold (0.0-1.0)
    pub max_failures: usize,
}

impl CanaryConfig {
    pub fn new(stable_version: Version, canary_version: Version) -> Self {
        Self {
            stable_version,
            canary_version,
            canary_percentage: 0.05, // Start with 5%
            increment_percentage: 0.05,
            increment_interval_seconds: 300, // 5 minutes
            success_threshold: 0.95,         // 95% success rate
            max_failures: 3,
        }
    }

    pub fn with_initial_percentage(mut self, percentage: f32) -> Self {
        self.canary_percentage = percentage.clamp(0.0, 1.0);
        self
    }

    pub fn with_increment(mut self, percentage: f32, interval_seconds: u64) -> Self {
        self.increment_percentage = percentage.clamp(0.0, 1.0);
        self.increment_interval_seconds = interval_seconds;
        self
    }

    pub fn with_success_threshold(mut self, threshold: f32) -> Self {
        self.success_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub fn with_max_failures(mut self, max: usize) -> Self {
        self.max_failures = max;
        self
    }
}

/// Version deployment manager
pub struct VersionDeployer {
    registry: Arc<VersionRegistry>,
}

impl VersionDeployer {
    pub fn new(registry: Arc<VersionRegistry>) -> Self {
        Self { registry }
    }

    /// Perform a rolling update
    pub async fn rolling_update(
        &self,
        config: RollingUpdateConfig,
        agents: Vec<AgentId>,
    ) -> Result<Vec<AgentId>, VersionError> {
        // Verify versions exist and migration is possible
        if !self
            .registry
            .can_migrate(&config.from_version, &config.to_version)?
        {
            return Err(VersionError::IncompatibleVersion(
                config.from_version,
                config.to_version,
            ));
        }

        let mut updated_agents = Vec::new();
        let mut failed_count = 0;

        // Filter agents by current version
        let target_agents: Vec<AgentId> = agents
            .into_iter()
            .filter(|id| {
                self.registry
                    .get_agent_version(id)
                    .map(|v| v == config.from_version)
                    .unwrap_or(false)
            })
            .collect();

        match config.strategy {
            RollingUpdateStrategy::Immediate => {
                for agent_id in target_agents {
                    match self.update_agent(agent_id, config.to_version).await {
                        Ok(_) => updated_agents.push(agent_id),
                        Err(_) => {
                            failed_count += 1;
                            if failed_count > config.max_failures {
                                if config.rollback_on_failure {
                                    self.rollback_updates(&updated_agents, config.from_version)
                                        .await?;
                                }
                                return Err(VersionError::DeploymentError(
                                    "Too many failures during rolling update".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
            RollingUpdateStrategy::Batched {
                batch_size,
                delay_between_batches_ms,
            } => {
                for batch in target_agents.chunks(batch_size) {
                    for &agent_id in batch {
                        match self.update_agent(agent_id, config.to_version).await {
                            Ok(_) => updated_agents.push(agent_id),
                            Err(_) => {
                                failed_count += 1;
                                if failed_count > config.max_failures {
                                    if config.rollback_on_failure {
                                        self.rollback_updates(&updated_agents, config.from_version)
                                            .await?;
                                    }
                                    return Err(VersionError::DeploymentError(
                                        "Too many failures during rolling update".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        delay_between_batches_ms,
                    ))
                    .await;
                }
            }
            RollingUpdateStrategy::Sequential {
                delay_between_agents_ms,
            } => {
                for agent_id in target_agents {
                    match self.update_agent(agent_id, config.to_version).await {
                        Ok(_) => updated_agents.push(agent_id),
                        Err(_) => {
                            failed_count += 1;
                            if failed_count > config.max_failures {
                                if config.rollback_on_failure {
                                    self.rollback_updates(&updated_agents, config.from_version)
                                        .await?;
                                }
                                return Err(VersionError::DeploymentError(
                                    "Too many failures during rolling update".to_string(),
                                ));
                            }
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_between_agents_ms))
                        .await;
                }
            }
            RollingUpdateStrategy::Percentage {
                percentage,
                delay_between_batches_ms,
            } => {
                let batch_size = ((target_agents.len() as f32 * percentage) as usize).max(1);
                for batch in target_agents.chunks(batch_size) {
                    for &agent_id in batch {
                        match self.update_agent(agent_id, config.to_version).await {
                            Ok(_) => updated_agents.push(agent_id),
                            Err(_) => {
                                failed_count += 1;
                                if failed_count > config.max_failures {
                                    if config.rollback_on_failure {
                                        self.rollback_updates(&updated_agents, config.from_version)
                                            .await?;
                                    }
                                    return Err(VersionError::DeploymentError(
                                        "Too many failures during rolling update".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        delay_between_batches_ms,
                    ))
                    .await;
                }
            }
        }

        Ok(updated_agents)
    }

    /// Rollback updates to previous version
    async fn rollback_updates(
        &self,
        agents: &[AgentId],
        version: Version,
    ) -> Result<(), VersionError> {
        for &agent_id in agents {
            self.update_agent(agent_id, version).await?;
        }
        Ok(())
    }

    /// Update a single agent
    async fn update_agent(
        &self,
        agent_id: AgentId,
        new_version: Version,
    ) -> Result<(), VersionError> {
        self.registry
            .update_agent_version(agent_id, new_version)
            .map(|_| ())
    }

    /// Start an A/B test
    pub fn start_ab_test(&self, config: ABTestConfig) -> Result<ABTestDeployment, VersionError> {
        // Verify versions exist
        self.registry.get_version(&config.version_a)?;
        self.registry.get_version(&config.version_b)?;

        Ok(ABTestDeployment {
            config,
            version_a_agents: HashSet::new(),
            version_b_agents: HashSet::new(),
        })
    }

    /// Start a canary deployment
    pub fn start_canary(&self, config: CanaryConfig) -> Result<CanaryDeployment, VersionError> {
        // Verify versions exist
        self.registry.get_version(&config.stable_version)?;
        self.registry.get_version(&config.canary_version)?;

        Ok(CanaryDeployment {
            config,
            canary_agents: HashSet::new(),
            stable_agents: HashSet::new(),
            current_percentage: 0.0,
            success_count: 0,
            failure_count: 0,
        })
    }
}

/// A/B test deployment state
pub struct ABTestDeployment {
    pub config: ABTestConfig,
    pub version_a_agents: HashSet<AgentId>,
    pub version_b_agents: HashSet<AgentId>,
}

impl ABTestDeployment {
    /// Assign an agent to a version based on traffic split
    pub fn assign_agent(&mut self, agent_id: AgentId) -> Version {
        use rand::Rng;
        let mut rng = rand::rng();

        if rng.random::<f32>() < self.config.traffic_split {
            self.version_b_agents.insert(agent_id);
            self.config.version_b
        } else {
            self.version_a_agents.insert(agent_id);
            self.config.version_a
        }
    }

    /// Get statistics
    pub fn stats(&self) -> ABTestStats {
        ABTestStats {
            version_a_count: self.version_a_agents.len(),
            version_b_count: self.version_b_agents.len(),
            actual_split: if self.version_a_agents.is_empty() && self.version_b_agents.is_empty() {
                0.0
            } else {
                self.version_b_agents.len() as f32
                    / (self.version_a_agents.len() + self.version_b_agents.len()) as f32
            },
        }
    }
}

/// A/B test statistics
#[derive(Debug, Clone)]
pub struct ABTestStats {
    pub version_a_count: usize,
    pub version_b_count: usize,
    pub actual_split: f32,
}

/// Canary deployment state
pub struct CanaryDeployment {
    pub config: CanaryConfig,
    pub canary_agents: HashSet<AgentId>,
    pub stable_agents: HashSet<AgentId>,
    pub current_percentage: f32,
    pub success_count: usize,
    pub failure_count: usize,
}

impl CanaryDeployment {
    /// Assign an agent to canary or stable version
    pub fn assign_agent(&mut self, agent_id: AgentId) -> Version {
        use rand::Rng;
        let mut rng = rand::rng();

        if rng.random::<f32>() < self.current_percentage {
            self.canary_agents.insert(agent_id);
            self.config.canary_version
        } else {
            self.stable_agents.insert(agent_id);
            self.config.stable_version
        }
    }

    /// Record a success for canary
    pub fn record_success(&mut self) {
        self.success_count += 1;
    }

    /// Record a failure for canary
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
    }

    /// Check if canary should be promoted
    pub fn should_promote(&self) -> bool {
        if self.success_count + self.failure_count == 0 {
            return false;
        }

        let success_rate =
            self.success_count as f32 / (self.success_count + self.failure_count) as f32;
        success_rate >= self.config.success_threshold
            && self.failure_count < self.config.max_failures
    }

    /// Check if canary should be aborted
    pub fn should_abort(&self) -> bool {
        self.failure_count >= self.config.max_failures
    }

    /// Increment canary percentage
    pub fn increment(&mut self) {
        self.current_percentage =
            (self.current_percentage + self.config.increment_percentage).min(1.0);
    }

    /// Get statistics
    pub fn stats(&self) -> CanaryStats {
        CanaryStats {
            canary_count: self.canary_agents.len(),
            stable_count: self.stable_agents.len(),
            current_percentage: self.current_percentage,
            success_count: self.success_count,
            failure_count: self.failure_count,
            success_rate: if self.success_count + self.failure_count == 0 {
                0.0
            } else {
                self.success_count as f32 / (self.success_count + self.failure_count) as f32
            },
        }
    }
}

/// Canary deployment statistics
#[derive(Debug, Clone)]
pub struct CanaryStats {
    pub canary_count: usize,
    pub stable_count: usize,
    pub current_percentage: f32,
    pub success_count: usize,
    pub failure_count: usize,
    pub success_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_ordering() {
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_0_1 = Version::new(1, 0, 1);
        let v1_1_0 = Version::new(1, 1, 0);
        let v2_0_0 = Version::new(2, 0, 0);

        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 < v1_1_0);
        assert!(v1_1_0 < v2_0_0);
    }

    #[test]
    fn test_version_compatibility() {
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_1_0 = Version::new(1, 1, 0);
        let v2_0_0 = Version::new(2, 0, 0);

        assert!(v1_1_0.is_compatible_with(&v1_0_0));
        assert!(!v1_0_0.is_compatible_with(&v1_1_0));
        assert!(!v2_0_0.is_compatible_with(&v1_0_0));
    }

    #[test]
    fn test_version_change_detection() {
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_0_1 = Version::new(1, 0, 1);
        let v1_1_0 = Version::new(1, 1, 0);
        let v2_0_0 = Version::new(2, 0, 0);

        assert!(v1_0_1.is_patch_update(&v1_0_0));
        assert!(v1_1_0.is_minor_update(&v1_0_0));
        assert!(v2_0_0.is_breaking_change(&v1_0_0));
    }

    #[test]
    fn test_version_registry() {
        let registry = VersionRegistry::new();
        let v1_0_0 = Version::new(1, 0, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);

        let metadata = VersionMetadata::new(v1_0_0, dna, "Initial release".to_string());
        registry.register_version(metadata).unwrap();

        let retrieved = registry.get_version(&v1_0_0).unwrap();
        assert_eq!(retrieved.version, v1_0_0);
    }

    #[test]
    fn test_agent_version_tracking() {
        let registry = VersionRegistry::new();
        let v1_0_0 = Version::new(1, 0, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);
        let agent_id = AgentId::new_v4();

        let metadata = VersionMetadata::new(v1_0_0, dna, "Initial release".to_string());
        registry.register_version(metadata).unwrap();
        registry.register_agent(agent_id, v1_0_0).unwrap();

        let version = registry.get_agent_version(&agent_id).unwrap();
        assert_eq!(version, v1_0_0);
    }

    #[test]
    fn test_version_update() {
        let registry = VersionRegistry::new();
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_1_0 = Version::new(1, 1, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);
        let agent_id = AgentId::new_v4();

        registry
            .register_version(VersionMetadata::new(
                v1_0_0,
                dna.clone(),
                "v1.0.0".to_string(),
            ))
            .unwrap();
        registry
            .register_version(VersionMetadata::new(v1_1_0, dna, "v1.1.0".to_string()))
            .unwrap();
        registry.register_agent(agent_id, v1_0_0).unwrap();

        let old_version = registry.update_agent_version(agent_id, v1_1_0).unwrap();
        assert_eq!(old_version, v1_0_0);

        let new_version = registry.get_agent_version(&agent_id).unwrap();
        assert_eq!(new_version, v1_1_0);
    }

    #[test]
    fn test_get_agents_by_version() {
        let registry = VersionRegistry::new();
        let v1_0_0 = Version::new(1, 0, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);

        registry
            .register_version(VersionMetadata::new(v1_0_0, dna, "v1.0.0".to_string()))
            .unwrap();

        let agent1 = AgentId::new_v4();
        let agent2 = AgentId::new_v4();
        registry.register_agent(agent1, v1_0_0).unwrap();
        registry.register_agent(agent2, v1_0_0).unwrap();

        let agents = registry.get_agents_by_version(&v1_0_0);
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&agent1));
        assert!(agents.contains(&agent2));
    }

    #[test]
    fn test_migration_compatibility() {
        let registry = VersionRegistry::new();
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_1_0 = Version::new(1, 1, 0);
        let v2_0_0 = Version::new(2, 0, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);

        registry
            .register_version(VersionMetadata::new(
                v1_0_0,
                dna.clone(),
                "v1.0.0".to_string(),
            ))
            .unwrap();
        registry
            .register_version(VersionMetadata::new(
                v1_1_0,
                dna.clone(),
                "v1.1.0".to_string(),
            ))
            .unwrap();
        registry
            .register_version(
                VersionMetadata::new(v2_0_0, dna, "v2.0.0".to_string())
                    .with_migration_path(vec![v1_1_0]),
            )
            .unwrap();

        assert!(registry.can_migrate(&v1_0_0, &v1_1_0).unwrap());
        assert!(!registry.can_migrate(&v1_0_0, &v2_0_0).unwrap());
        assert!(registry.can_migrate(&v1_1_0, &v2_0_0).unwrap());
    }

    #[test]
    fn test_ab_test_assignment() {
        let mut deployment = ABTestDeployment {
            config: ABTestConfig::new(Version::new(1, 0, 0), Version::new(1, 1, 0), 0.5),
            version_a_agents: HashSet::new(),
            version_b_agents: HashSet::new(),
        };

        // Assign 100 agents
        for _ in 0..100 {
            let agent_id = AgentId::new_v4();
            deployment.assign_agent(agent_id);
        }

        let stats = deployment.stats();
        // With 50/50 split, we expect roughly equal distribution
        // Allow for some variance due to randomness
        assert!(stats.version_a_count > 30 && stats.version_a_count < 70);
        assert!(stats.version_b_count > 30 && stats.version_b_count < 70);
    }

    #[test]
    fn test_canary_deployment() {
        let mut deployment = CanaryDeployment {
            config: CanaryConfig::new(Version::new(1, 0, 0), Version::new(1, 1, 0))
                .with_initial_percentage(0.1)
                .with_max_failures(10), // Allow up to 10 failures
            canary_agents: HashSet::new(),
            stable_agents: HashSet::new(),
            current_percentage: 0.1,
            success_count: 0,
            failure_count: 0,
        };

        // Record successful operations
        for _ in 0..95 {
            deployment.record_success();
        }
        for _ in 0..5 {
            deployment.record_failure();
        }

        assert!(deployment.should_promote());
        assert!(!deployment.should_abort());

        let stats = deployment.stats();
        assert_eq!(stats.success_rate, 0.95);
    }

    #[test]
    fn test_canary_abort() {
        let mut deployment = CanaryDeployment {
            config: CanaryConfig::new(Version::new(1, 0, 0), Version::new(1, 1, 0))
                .with_max_failures(5),
            canary_agents: HashSet::new(),
            stable_agents: HashSet::new(),
            current_percentage: 0.1,
            success_count: 0,
            failure_count: 0,
        };

        for _ in 0..5 {
            deployment.record_failure();
        }

        assert!(deployment.should_abort());
    }

    #[tokio::test]
    async fn test_rolling_update_immediate() {
        let registry = Arc::new(VersionRegistry::new());
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_1_0 = Version::new(1, 1, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);

        registry
            .register_version(VersionMetadata::new(
                v1_0_0,
                dna.clone(),
                "v1.0.0".to_string(),
            ))
            .unwrap();
        registry
            .register_version(VersionMetadata::new(v1_1_0, dna, "v1.1.0".to_string()))
            .unwrap();

        let mut agents = Vec::new();
        for _ in 0..10 {
            let agent_id = AgentId::new_v4();
            registry.register_agent(agent_id, v1_0_0).unwrap();
            agents.push(agent_id);
        }

        let deployer = VersionDeployer::new(registry.clone());
        let config = RollingUpdateConfig::new(v1_0_0, v1_1_0)
            .with_strategy(RollingUpdateStrategy::Immediate);

        let updated = deployer
            .rolling_update(config, agents.clone())
            .await
            .unwrap();
        assert_eq!(updated.len(), 10);

        for agent_id in agents {
            let version = registry.get_agent_version(&agent_id).unwrap();
            assert_eq!(version, v1_1_0);
        }
    }

    #[tokio::test]
    async fn test_rolling_update_batched() {
        let registry = Arc::new(VersionRegistry::new());
        let v1_0_0 = Version::new(1, 0, 0);
        let v1_1_0 = Version::new(1, 1, 0);
        let dna = Dna::new(vec![1, 2, 3, 4]);

        registry
            .register_version(VersionMetadata::new(
                v1_0_0,
                dna.clone(),
                "v1.0.0".to_string(),
            ))
            .unwrap();
        registry
            .register_version(VersionMetadata::new(v1_1_0, dna, "v1.1.0".to_string()))
            .unwrap();

        let mut agents = Vec::new();
        for _ in 0..20 {
            let agent_id = AgentId::new_v4();
            registry.register_agent(agent_id, v1_0_0).unwrap();
            agents.push(agent_id);
        }

        let deployer = VersionDeployer::new(registry.clone());
        let config = RollingUpdateConfig::new(v1_0_0, v1_1_0).with_strategy(
            RollingUpdateStrategy::Batched {
                batch_size: 5,
                delay_between_batches_ms: 10,
            },
        );

        let updated = deployer
            .rolling_update(config, agents.clone())
            .await
            .unwrap();
        assert_eq!(updated.len(), 20);
    }
}
