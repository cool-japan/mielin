//! State Replication Module
//!
//! Implements state replication across nodes for high availability.

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

pub type NodeId = String;

#[derive(Debug, Error)]
pub enum ReplicationError {
    #[error("Replication failed: {0}")]
    ReplicationFailed(String),
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("State mismatch: {0}")]
    StateMismatch(String),
    #[error("Replication timeout")]
    Timeout,
    #[error("Insufficient replicas: {current}/{required}")]
    InsufficientReplicas { current: usize, required: usize },
}

pub type ReplicationResult<T> = Result<T, ReplicationError>;

/// Replication strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationStrategy {
    /// Synchronous replication (wait for all replicas)
    Synchronous,
    /// Asynchronous replication (fire and forget)
    Asynchronous,
    /// Quorum-based (wait for majority)
    Quorum { min_replicas: usize },
    /// Best-effort (replicate to as many as possible)
    BestEffort,
}

/// Replication state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationState {
    /// Idle (no replication in progress)
    Idle,
    /// Replicating
    Replicating,
    /// Synchronized
    Synchronized,
    /// Failed
    Failed,
    /// Degraded (some replicas failed)
    Degraded,
}

/// Replication configuration
#[derive(Debug, Clone)]
pub struct ReplicationConfig {
    /// Replication strategy
    pub strategy: ReplicationStrategy,
    /// Replication factor (number of replicas)
    pub replication_factor: usize,
    /// Replication timeout
    pub timeout: Duration,
    /// Enable compression
    pub enable_compression: bool,
    /// Checksum validation
    pub verify_checksum: bool,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            strategy: ReplicationStrategy::Quorum { min_replicas: 2 },
            replication_factor: 3,
            timeout: Duration::from_secs(10),
            enable_compression: true,
            verify_checksum: true,
        }
    }
}

/// State replica
#[derive(Debug, Clone)]
pub struct StateReplica {
    /// Node holding the replica
    pub node_id: NodeId,
    /// Agent state data
    pub state_data: Vec<u8>,
    /// Checksum
    pub checksum: u64,
    /// Timestamp of last update
    pub last_updated: Instant,
    /// Whether replica is synchronized
    pub synchronized: bool,
}

/// Replication log entry
#[derive(Debug, Clone)]
pub struct ReplicationLog {
    /// Agent ID
    pub agent_id: AgentId,
    /// Source node
    pub source_node: NodeId,
    /// Target nodes
    pub target_nodes: Vec<NodeId>,
    /// Timestamp
    pub timestamp: Instant,
    /// Success count
    pub success_count: usize,
    /// Failure count
    pub failure_count: usize,
    /// Duration
    pub duration: Duration,
}

/// Replication manager
pub struct ReplicationManager {
    /// Configuration
    config: ReplicationConfig,
    /// Replicas by agent ID
    replicas: Arc<RwLock<HashMap<AgentId, Vec<StateReplica>>>>,
    /// Replication logs
    logs: Arc<RwLock<Vec<ReplicationLog>>>,
    /// Replication state by agent
    states: Arc<RwLock<HashMap<AgentId, ReplicationState>>>,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub fn new(config: ReplicationConfig) -> Self {
        Self {
            config,
            replicas: Arc::new(RwLock::new(HashMap::new())),
            logs: Arc::new(RwLock::new(Vec::new())),
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Replicate agent state to nodes
    pub fn replicate_state(
        &self,
        agent_id: &AgentId,
        state_data: Vec<u8>,
        target_nodes: Vec<NodeId>,
    ) -> ReplicationResult<ReplicationLog> {
        let start_time = Instant::now();

        // Update replication state
        self.set_state(agent_id, ReplicationState::Replicating)?;

        // Calculate checksum if enabled
        let checksum = if self.config.verify_checksum {
            self.calculate_checksum(&state_data)
        } else {
            0
        };

        // Compress if enabled (for network transport simulation)
        let _data_to_replicate = if self.config.enable_compression {
            self.compress_data(&state_data)?
        } else {
            state_data.clone()
        };

        // Replicate to target nodes
        let mut success_count = 0;
        let mut replicas = Vec::new();

        for node_id in &target_nodes {
            // Store original data in replica, not compressed (compression is for transport only)
            match self.replicate_to_node(agent_id, node_id, &state_data, checksum) {
                Ok(replica) => {
                    replicas.push(replica);
                    success_count += 1;
                }
                Err(_) => {
                    // Log error but continue
                }
            }

            // Check if we've met the strategy requirements
            if self.should_stop_replication(success_count, target_nodes.len())? {
                break;
            }
        }

        // Update replicas
        self.replicas
            .write()
            .map_err(|_| ReplicationError::ReplicationFailed("Failed to acquire lock".to_string()))?
            .insert(*agent_id, replicas);

        // Check if replication succeeded based on strategy
        let final_state = self.evaluate_replication_result(success_count, target_nodes.len())?;
        self.set_state(agent_id, final_state)?;

        // Create log entry
        let target_nodes_len = target_nodes.len();
        let log = ReplicationLog {
            agent_id: *agent_id,
            source_node: "local".to_string(),
            target_nodes,
            timestamp: start_time,
            success_count,
            failure_count: target_nodes_len - success_count,
            duration: start_time.elapsed(),
        };

        // Record log
        self.logs
            .write()
            .map_err(|_| ReplicationError::ReplicationFailed("Failed to acquire lock".to_string()))?
            .push(log.clone());

        Ok(log)
    }

    /// Replicate to a single node
    fn replicate_to_node(
        &self,
        _agent_id: &AgentId,
        node_id: &NodeId,
        data: &[u8],
        checksum: u64,
    ) -> ReplicationResult<StateReplica> {
        // In a real implementation, this would send data over the network
        // For now, we simulate successful replication
        Ok(StateReplica {
            node_id: node_id.clone(),
            state_data: data.to_vec(),
            checksum,
            last_updated: Instant::now(),
            synchronized: true,
        })
    }

    /// Check if we should stop replication based on strategy
    fn should_stop_replication(
        &self,
        success_count: usize,
        total_targets: usize,
    ) -> ReplicationResult<bool> {
        match &self.config.strategy {
            ReplicationStrategy::Synchronous => Ok(success_count == total_targets),
            ReplicationStrategy::Asynchronous => Ok(success_count > 0),
            ReplicationStrategy::Quorum { .. } => Ok(false), // Try all targets for Quorum
            ReplicationStrategy::BestEffort => Ok(false),    // Try all
        }
    }

    /// Evaluate replication result
    fn evaluate_replication_result(
        &self,
        success_count: usize,
        total_targets: usize,
    ) -> ReplicationResult<ReplicationState> {
        match &self.config.strategy {
            ReplicationStrategy::Synchronous => {
                if success_count == total_targets {
                    Ok(ReplicationState::Synchronized)
                } else {
                    Err(ReplicationError::InsufficientReplicas {
                        current: success_count,
                        required: total_targets,
                    })
                }
            }
            ReplicationStrategy::Asynchronous | ReplicationStrategy::BestEffort => {
                if success_count > 0 {
                    Ok(if success_count == total_targets {
                        ReplicationState::Synchronized
                    } else {
                        ReplicationState::Degraded
                    })
                } else {
                    Err(ReplicationError::ReplicationFailed(
                        "No replicas created".to_string(),
                    ))
                }
            }
            ReplicationStrategy::Quorum { min_replicas } => {
                if success_count >= *min_replicas {
                    Ok(if success_count == total_targets {
                        ReplicationState::Synchronized
                    } else {
                        ReplicationState::Degraded
                    })
                } else {
                    Err(ReplicationError::InsufficientReplicas {
                        current: success_count,
                        required: *min_replicas,
                    })
                }
            }
        }
    }

    /// Get replicas for an agent
    pub fn get_replicas(&self, agent_id: &AgentId) -> ReplicationResult<Vec<StateReplica>> {
        Ok(self
            .replicas
            .read()
            .map_err(|_| ReplicationError::ReplicationFailed("Failed to acquire lock".to_string()))?
            .get(agent_id)
            .cloned()
            .unwrap_or_default())
    }

    /// Get replication state
    pub fn get_state(&self, agent_id: &AgentId) -> ReplicationResult<ReplicationState> {
        Ok(self
            .states
            .read()
            .map_err(|_| ReplicationError::ReplicationFailed("Failed to acquire lock".to_string()))?
            .get(agent_id)
            .cloned()
            .unwrap_or(ReplicationState::Idle))
    }

    /// Set replication state
    fn set_state(&self, agent_id: &AgentId, state: ReplicationState) -> ReplicationResult<()> {
        self.states
            .write()
            .map_err(|_| ReplicationError::ReplicationFailed("Failed to acquire lock".to_string()))?
            .insert(*agent_id, state);
        Ok(())
    }

    /// Get replication logs
    pub fn get_logs(&self) -> ReplicationResult<Vec<ReplicationLog>> {
        Ok(self
            .logs
            .read()
            .map_err(|_| ReplicationError::ReplicationFailed("Failed to acquire lock".to_string()))?
            .clone())
    }

    /// Calculate checksum
    fn calculate_checksum(&self, data: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        hasher.finish()
    }

    /// Compress data
    fn compress_data(&self, data: &[u8]) -> ReplicationResult<Vec<u8>> {
        // Simple RLE compression for demonstration
        let mut compressed = Vec::new();
        let mut count = 1u8;
        let mut current = data.first().copied();

        for &byte in data.iter().skip(1) {
            if Some(byte) == current && count < 255 {
                count += 1;
            } else {
                if let Some(c) = current {
                    compressed.push(count);
                    compressed.push(c);
                }
                current = Some(byte);
                count = 1;
            }
        }

        if let Some(c) = current {
            compressed.push(count);
            compressed.push(c);
        }

        Ok(compressed)
    }

    /// Verify replica integrity
    pub fn verify_replica(&self, replica: &StateReplica) -> ReplicationResult<bool> {
        if !self.config.verify_checksum {
            return Ok(true);
        }

        let calculated_checksum = self.calculate_checksum(&replica.state_data);
        Ok(calculated_checksum == replica.checksum)
    }

    /// Synchronize replicas
    pub fn synchronize(&self, agent_id: &AgentId) -> ReplicationResult<usize> {
        let replicas = self.get_replicas(agent_id)?;
        let mut synchronized_count = 0;

        for replica in replicas {
            if self.verify_replica(&replica)? {
                synchronized_count += 1;
            }
        }

        Ok(synchronized_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn test_replication_manager_creation() {
        let config = ReplicationConfig::default();
        let _manager = ReplicationManager::new(config);
    }

    #[test]
    fn test_replicate_state() {
        let config = ReplicationConfig {
            strategy: ReplicationStrategy::Quorum { min_replicas: 2 },
            replication_factor: 3,
            ..Default::default()
        };
        let manager = ReplicationManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state_data = vec![1, 2, 3, 4, 5];
        let targets = vec![
            "node1".to_string(),
            "node2".to_string(),
            "node3".to_string(),
        ];

        let log = manager
            .replicate_state(&agent.id(), state_data, targets)
            .expect("replicate state");

        assert!(log.success_count >= 2);
        assert_eq!(
            manager.get_state(&agent.id()).expect("get state"),
            ReplicationState::Synchronized
        );
    }

    #[test]
    fn test_synchronous_replication() {
        let config = ReplicationConfig {
            strategy: ReplicationStrategy::Synchronous,
            ..Default::default()
        };
        let manager = ReplicationManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state_data = vec![1, 2, 3, 4, 5];
        let targets = vec!["node1".to_string(), "node2".to_string()];

        let log = manager
            .replicate_state(&agent.id(), state_data, targets)
            .expect("replicate state");

        assert_eq!(log.success_count, 2);
        assert_eq!(log.failure_count, 0);
    }

    #[test]
    fn test_get_replicas() {
        let config = ReplicationConfig::default();
        let manager = ReplicationManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state_data = vec![1, 2, 3, 4, 5];
        let targets = vec!["node1".to_string(), "node2".to_string()];

        manager
            .replicate_state(&agent.id(), state_data, targets)
            .expect("replicate state");

        let replicas = manager.get_replicas(&agent.id()).expect("get replicas");
        assert!(!replicas.is_empty());
    }

    #[test]
    fn test_checksum_verification() {
        let config = ReplicationConfig {
            verify_checksum: true,
            ..Default::default()
        };
        let manager = ReplicationManager::new(config);

        let data = vec![1, 2, 3, 4, 5];
        let checksum = manager.calculate_checksum(&data);

        let replica = StateReplica {
            node_id: "node1".to_string(),
            state_data: data,
            checksum,
            last_updated: Instant::now(),
            synchronized: true,
        };

        assert!(manager.verify_replica(&replica).expect("verify replica"));
    }

    #[test]
    fn test_compression() {
        let config = ReplicationConfig::default();
        let manager = ReplicationManager::new(config);

        let data = vec![1, 1, 1, 2, 2, 3];
        let compressed = manager.compress_data(&data).expect("compress");

        // RLE should produce: [3, 1, 2, 2, 1, 3]
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_replication_logs() {
        let config = ReplicationConfig {
            strategy: ReplicationStrategy::BestEffort,
            ..Default::default()
        };
        let manager = ReplicationManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state_data = vec![1, 2, 3, 4, 5];
        let targets = vec!["node1".to_string()];

        manager
            .replicate_state(&agent.id(), state_data, targets)
            .expect("replicate state");

        let logs = manager.get_logs().expect("get logs");
        assert_eq!(logs.len(), 1);
    }

    #[test]
    fn test_synchronize() {
        let config = ReplicationConfig::default();
        let manager = ReplicationManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let state_data = vec![1, 2, 3, 4, 5];
        let targets = vec!["node1".to_string(), "node2".to_string()];

        manager
            .replicate_state(&agent.id(), state_data, targets)
            .expect("replicate state");

        let sync_count = manager.synchronize(&agent.id()).expect("synchronize");
        assert!(sync_count > 0);
    }
}
