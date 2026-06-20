//! Live Agent Migration Service
//!
//! Orchestrates agent migration across the mesh with support for:
//! - Pre-copy migration (iterative dirty page transfer)
//! - Post-copy migration (demand paging)
//! - Hybrid migration strategies
//! - Migration telemetry and rollback

use crate::{registry::AgentId, Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Maximum migration duration before timeout
const MIGRATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum pre-copy iterations before switching to post-copy
const MAX_PRECOPY_ITERATIONS: usize = 3;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("Migration timeout after {0:?}")]
    Timeout(Duration),
    #[error("Migration failed: {0}")]
    Failed(String),
    #[error("Agent not found: {0:?}")]
    AgentNotFound(AgentId),
    #[error("Target node unreachable: {0}")]
    NodeUnreachable(NodeId),
    #[error("Incompatible migration state: {0}")]
    IncompatibleState(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Migration cancelled: {0}")]
    Cancelled(String),
}

/// Migration strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStrategy {
    /// Pre-copy: iteratively copy memory while agent runs, then pause and copy delta
    PreCopy,
    /// Post-copy: pause agent, copy minimal state, resume on target, copy rest on demand
    PostCopy,
    /// Hybrid: start with pre-copy, switch to post-copy if progress stalls
    Hybrid,
}

/// Migration phase tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationPhase {
    /// Planning the migration
    Planning,
    /// Pre-copy: iteratively copying dirty pages
    PreCopy { iteration: usize },
    /// Pausing the agent
    Pausing,
    /// Copying final state
    Copying,
    /// Transferring over network
    Transferring,
    /// Validating on target
    Validating,
    /// Resuming on target
    Resuming,
    /// Cleanup on source
    Cleanup,
    /// Migration complete
    Complete,
    /// Migration failed
    Failed,
}

/// Migration telemetry data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMetrics {
    pub agent_id: AgentId,
    pub source_node: NodeId,
    pub target_node: NodeId,
    pub strategy: MigrationStrategy,
    pub phase: MigrationPhase,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub total_bytes: usize,
    pub transferred_bytes: usize,
    pub downtime_ms: u64,
    pub total_duration_ms: u64,
    pub precopy_iterations: usize,
    pub success: bool,
    pub error_message: Option<String>,
}

impl MigrationMetrics {
    pub fn new(
        agent_id: AgentId,
        source_node: NodeId,
        target_node: NodeId,
        strategy: MigrationStrategy,
    ) -> Self {
        Self {
            agent_id,
            source_node,
            target_node,
            strategy,
            phase: MigrationPhase::Planning,
            started_at: SystemTime::now(),
            completed_at: None,
            total_bytes: 0,
            transferred_bytes: 0,
            downtime_ms: 0,
            total_duration_ms: 0,
            precopy_iterations: 0,
            success: false,
            error_message: None,
        }
    }

    pub fn update_phase(&mut self, phase: MigrationPhase) {
        self.phase = phase;
    }

    pub fn complete(&mut self, success: bool, error: Option<String>) {
        self.completed_at = Some(SystemTime::now());
        self.success = success;
        self.error_message = error;
        self.total_duration_ms = self.started_at.elapsed().unwrap_or_default().as_millis() as u64;
    }
}

/// Migration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRequest {
    pub agent_id: AgentId,
    pub source_node: NodeId,
    pub target_node: NodeId,
    pub target_address: SocketAddr,
    pub strategy: MigrationStrategy,
    pub priority: u8,
}

/// Migration state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSnapshot {
    pub agent_id: AgentId,
    pub snapshot_data: Vec<u8>,
    pub memory_pages: Vec<MemoryPage>,
    pub iteration: usize,
    pub is_final: bool,
}

/// Memory page for incremental migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPage {
    pub offset: usize,
    pub data: Vec<u8>,
    pub dirty: bool,
}

/// Migration coordinator service
pub struct MigrationCoordinator {
    local_node: Arc<Node>,
    active_migrations: Arc<RwLock<HashMap<AgentId, MigrationMetrics>>>,
    completed_migrations: Arc<RwLock<Vec<MigrationMetrics>>>,
    cancellation_tokens: Arc<RwLock<HashMap<AgentId, CancellationToken>>>,
    cancellation_tx: broadcast::Sender<AgentId>,
}

impl MigrationCoordinator {
    pub fn new(node: Arc<Node>) -> Self {
        let (cancellation_tx, _) = broadcast::channel(16);

        Self {
            local_node: node,
            active_migrations: Arc::new(RwLock::new(HashMap::new())),
            completed_migrations: Arc::new(RwLock::new(Vec::new())),
            cancellation_tokens: Arc::new(RwLock::new(HashMap::new())),
            cancellation_tx,
        }
    }

    /// Start the migration coordinator
    pub async fn start(&self) {
        info!(
            "Starting migration coordinator for node {}",
            self.local_node.id()
        );
        self.spawn_timeout_checker();
    }

    /// Spawn task to check for migration timeouts
    fn spawn_timeout_checker(&self) {
        let active_migrations = self.active_migrations.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                let mut migrations = active_migrations.write().await;
                let mut timeouts = Vec::new();

                for (agent_id, metrics) in migrations.iter() {
                    if metrics.started_at.elapsed().unwrap_or_default() > MIGRATION_TIMEOUT {
                        timeouts.push(*agent_id);
                        warn!("Migration timeout for agent {:?}", agent_id);
                    }
                }

                for agent_id in timeouts {
                    if let Some(mut metrics) = migrations.remove(&agent_id) {
                        metrics.complete(
                            false,
                            Some(format!("Migration timeout after {:?}", MIGRATION_TIMEOUT)),
                        );
                    }
                }
            }
        });
    }

    /// Initiate agent migration
    pub async fn initiate_migration(
        &self,
        request: MigrationRequest,
    ) -> Result<(), MigrationError> {
        info!(
            "Initiating {:?} migration for agent {:?} from {} to {}",
            request.strategy, request.agent_id, request.source_node, request.target_node
        );

        let mut metrics = MigrationMetrics::new(
            request.agent_id,
            request.source_node,
            request.target_node,
            request.strategy,
        );

        // Create cancellation token for this migration
        let cancel_token = CancellationToken::new();
        let mut tokens = self.cancellation_tokens.write().await;
        tokens.insert(request.agent_id, cancel_token);
        drop(tokens);

        // Add to active migrations
        let mut active = self.active_migrations.write().await;
        active.insert(request.agent_id, metrics.clone());
        drop(active);

        // Execute migration based on strategy
        let result = match request.strategy {
            MigrationStrategy::PreCopy => {
                self.execute_precopy_migration(&request, &mut metrics).await
            }
            MigrationStrategy::PostCopy => {
                self.execute_postcopy_migration(&request, &mut metrics)
                    .await
            }
            MigrationStrategy::Hybrid => {
                self.execute_hybrid_migration(&request, &mut metrics).await
            }
        };

        // Update metrics
        match &result {
            Ok(()) => {
                metrics.complete(true, None);
                info!(
                    "Migration completed successfully for agent {:?}",
                    request.agent_id
                );
            }
            Err(e) => {
                metrics.complete(false, Some(e.to_string()));
                warn!("Migration failed for agent {:?}: {}", request.agent_id, e);
            }
        }

        // Move to completed and cleanup
        let mut active = self.active_migrations.write().await;
        active.remove(&request.agent_id);
        drop(active);

        let mut tokens = self.cancellation_tokens.write().await;
        tokens.remove(&request.agent_id);
        drop(tokens);

        let mut completed = self.completed_migrations.write().await;
        completed.push(metrics);

        result
    }

    /// Execute pre-copy migration
    async fn execute_precopy_migration(
        &self,
        request: &MigrationRequest,
        metrics: &mut MigrationMetrics,
    ) -> Result<(), MigrationError> {
        debug!(
            "Starting pre-copy migration for agent {:?}",
            request.agent_id
        );

        // Phase 1: Iteratively copy memory while agent runs
        for iteration in 0..MAX_PRECOPY_ITERATIONS {
            // Check for cancellation
            if self.is_cancelled(&request.agent_id).await {
                return Err(MigrationError::Cancelled(
                    "Migration cancelled during pre-copy phase".to_string(),
                ));
            }

            metrics.update_phase(MigrationPhase::PreCopy { iteration });
            metrics.precopy_iterations = iteration + 1;

            // In real implementation:
            // 1. Snapshot dirty memory pages
            // 2. Transfer to target node
            // 3. Check if dirty set is small enough

            debug!(
                "Pre-copy iteration {} for agent {:?}",
                iteration, request.agent_id
            );

            // Simulate iteration delay
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Phase 2: Pause agent
        if self.is_cancelled(&request.agent_id).await {
            return Err(MigrationError::Cancelled(
                "Migration cancelled before pausing".to_string(),
            ));
        }
        metrics.update_phase(MigrationPhase::Pausing);
        let pause_start = Instant::now();

        // Phase 3: Copy final delta
        if self.is_cancelled(&request.agent_id).await {
            return Err(MigrationError::Cancelled(
                "Migration cancelled during copy phase".to_string(),
            ));
        }
        metrics.update_phase(MigrationPhase::Copying);

        // Phase 4: Transfer final state
        if self.is_cancelled(&request.agent_id).await {
            return Err(MigrationError::Cancelled(
                "Migration cancelled during transfer".to_string(),
            ));
        }
        metrics.update_phase(MigrationPhase::Transferring);

        // Phase 5: Validate on target
        if self.is_cancelled(&request.agent_id).await {
            return Err(MigrationError::Cancelled(
                "Migration cancelled during validation".to_string(),
            ));
        }
        metrics.update_phase(MigrationPhase::Validating);

        // Phase 6: Resume on target
        metrics.update_phase(MigrationPhase::Resuming);
        metrics.downtime_ms = pause_start.elapsed().as_millis() as u64;

        // Phase 7: Cleanup source
        metrics.update_phase(MigrationPhase::Cleanup);

        metrics.update_phase(MigrationPhase::Complete);
        Ok(())
    }

    /// Execute post-copy migration
    async fn execute_postcopy_migration(
        &self,
        request: &MigrationRequest,
        metrics: &mut MigrationMetrics,
    ) -> Result<(), MigrationError> {
        debug!(
            "Starting post-copy migration for agent {:?}",
            request.agent_id
        );

        // Phase 1: Pause agent
        metrics.update_phase(MigrationPhase::Pausing);
        let pause_start = Instant::now();

        // Phase 2: Copy minimal working set
        metrics.update_phase(MigrationPhase::Copying);

        // Phase 3: Transfer minimal state
        metrics.update_phase(MigrationPhase::Transferring);

        // Phase 4: Resume on target (agent can run while rest transfers)
        metrics.update_phase(MigrationPhase::Resuming);
        metrics.downtime_ms = pause_start.elapsed().as_millis() as u64;

        // Phase 5: Background copy remaining pages
        // In real implementation, this would continue in background

        // Phase 6: Validate
        metrics.update_phase(MigrationPhase::Validating);

        // Phase 7: Cleanup source
        metrics.update_phase(MigrationPhase::Cleanup);

        metrics.update_phase(MigrationPhase::Complete);
        Ok(())
    }

    /// Execute hybrid migration
    async fn execute_hybrid_migration(
        &self,
        request: &MigrationRequest,
        metrics: &mut MigrationMetrics,
    ) -> Result<(), MigrationError> {
        debug!("Starting hybrid migration for agent {:?}", request.agent_id);

        // Start with pre-copy
        for iteration in 0..MAX_PRECOPY_ITERATIONS {
            metrics.update_phase(MigrationPhase::PreCopy { iteration });
            metrics.precopy_iterations = iteration + 1;

            // Simulate checking if we should switch to post-copy
            // In real implementation:
            // - Check dirty page rate
            // - Check network bandwidth
            // - If progress is stalling, switch to post-copy

            tokio::time::sleep(Duration::from_millis(100)).await;

            // Example: switch to post-copy after 2 iterations
            if iteration >= 1 {
                info!(
                    "Switching from pre-copy to post-copy for agent {:?}",
                    request.agent_id
                );
                return self.execute_postcopy_migration(request, metrics).await;
            }
        }

        // Fall back to completing with pre-copy
        self.execute_precopy_migration(request, metrics).await
    }

    /// Get active migration count
    pub async fn active_migration_count(&self) -> usize {
        let active = self.active_migrations.read().await;
        active.len()
    }

    /// Get migration statistics
    pub async fn get_migration_stats(&self) -> MigrationStats {
        let active = self.active_migrations.read().await;
        let completed = self.completed_migrations.read().await;

        let total = active.len() + completed.len();
        let successful = completed.iter().filter(|m| m.success).count();
        let failed = completed.iter().filter(|m| !m.success).count();

        let avg_downtime = if !completed.is_empty() {
            completed.iter().map(|m| m.downtime_ms).sum::<u64>() / completed.len() as u64
        } else {
            0
        };

        let avg_duration = if !completed.is_empty() {
            completed.iter().map(|m| m.total_duration_ms).sum::<u64>() / completed.len() as u64
        } else {
            0
        };

        MigrationStats {
            total_migrations: total,
            active_migrations: active.len(),
            successful_migrations: successful,
            failed_migrations: failed,
            average_downtime_ms: avg_downtime,
            average_duration_ms: avg_duration,
        }
    }

    /// Get metrics for a specific migration
    pub async fn get_migration_metrics(&self, agent_id: AgentId) -> Option<MigrationMetrics> {
        let active = self.active_migrations.read().await;
        if let Some(metrics) = active.get(&agent_id) {
            return Some(metrics.clone());
        }
        drop(active);

        let completed = self.completed_migrations.read().await;
        completed.iter().find(|m| m.agent_id == agent_id).cloned()
    }

    /// Get recent migration history
    pub async fn get_migration_history(&self, limit: usize) -> Vec<MigrationMetrics> {
        let completed = self.completed_migrations.read().await;
        completed.iter().rev().take(limit).cloned().collect()
    }

    /// Cancel an active migration
    pub async fn cancel_migration(&self, agent_id: AgentId) -> Result<(), MigrationError> {
        info!("Cancelling migration for agent {:?}", agent_id);

        // Check if migration is active
        let active = self.active_migrations.read().await;
        if !active.contains_key(&agent_id) {
            return Err(MigrationError::AgentNotFound(agent_id));
        }
        drop(active);

        // Get and trigger cancellation token
        let mut tokens = self.cancellation_tokens.write().await;
        if let Some(token) = tokens.remove(&agent_id) {
            token.cancel();
        }
        drop(tokens);

        // Send cancellation notification
        let _ = self.cancellation_tx.send(agent_id);

        info!("Migration cancellation initiated for agent {:?}", agent_id);
        Ok(())
    }

    /// Cancel all active migrations
    pub async fn cancel_all_migrations(&self) -> usize {
        info!("Cancelling all active migrations");

        let active = self.active_migrations.read().await;
        let agent_ids: Vec<AgentId> = active.keys().copied().collect();
        drop(active);

        let mut cancelled_count = 0;
        for agent_id in agent_ids {
            if self.cancel_migration(agent_id).await.is_ok() {
                cancelled_count += 1;
            }
        }

        info!("Cancelled {} migrations", cancelled_count);
        cancelled_count
    }

    /// Subscribe to cancellation notifications
    pub fn subscribe_cancellations(&self) -> broadcast::Receiver<AgentId> {
        self.cancellation_tx.subscribe()
    }

    /// Check if migration is cancelled
    async fn is_cancelled(&self, agent_id: &AgentId) -> bool {
        let tokens = self.cancellation_tokens.read().await;
        if let Some(token) = tokens.get(agent_id) {
            token.is_cancelled()
        } else {
            false
        }
    }
}

/// Migration statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    pub total_migrations: usize,
    pub active_migrations: usize,
    pub successful_migrations: usize,
    pub failed_migrations: usize,
    pub average_downtime_ms: u64,
    pub average_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeRole;

    #[test]
    fn test_migration_metrics_creation() {
        let agent_id = [1u8; 16];
        let source = NodeId::new_v4();
        let target = NodeId::new_v4();

        let metrics = MigrationMetrics::new(agent_id, source, target, MigrationStrategy::PreCopy);

        assert_eq!(metrics.agent_id, agent_id);
        assert_eq!(metrics.source_node, source);
        assert_eq!(metrics.target_node, target);
        assert_eq!(metrics.strategy, MigrationStrategy::PreCopy);
        assert_eq!(metrics.phase, MigrationPhase::Planning);
        assert!(!metrics.success);
    }

    #[test]
    fn test_migration_metrics_completion() {
        let agent_id = [1u8; 16];
        let source = NodeId::new_v4();
        let target = NodeId::new_v4();

        let mut metrics =
            MigrationMetrics::new(agent_id, source, target, MigrationStrategy::PostCopy);

        // Wait a bit to ensure measurable duration
        std::thread::sleep(std::time::Duration::from_millis(10));

        metrics.complete(true, None);

        assert!(metrics.success);
        assert!(metrics.completed_at.is_some());
        assert!(metrics.total_duration_ms >= 10); // At least 10ms
        assert!(metrics.error_message.is_none());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_migration_coordinator_creation() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let coordinator = MigrationCoordinator::new(node);

        assert_eq!(coordinator.active_migration_count().await, 0);

        let stats = coordinator.get_migration_stats().await;
        assert_eq!(stats.total_migrations, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_precopy_migration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let coordinator = MigrationCoordinator::new(node.clone());

        let request = MigrationRequest {
            agent_id: [1u8; 16],
            source_node: *node.id(),
            target_node: NodeId::new_v4(),
            target_address: "127.0.0.1:8080".parse().unwrap(),
            strategy: MigrationStrategy::PreCopy,
            priority: 5,
        };

        let result = coordinator.initiate_migration(request.clone()).await;
        assert!(result.is_ok());

        let stats = coordinator.get_migration_stats().await;
        assert_eq!(stats.successful_migrations, 1);
        assert_eq!(stats.failed_migrations, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_postcopy_migration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let coordinator = MigrationCoordinator::new(node.clone());

        let request = MigrationRequest {
            agent_id: [2u8; 16],
            source_node: *node.id(),
            target_node: NodeId::new_v4(),
            target_address: "127.0.0.1:8080".parse().unwrap(),
            strategy: MigrationStrategy::PostCopy,
            priority: 5,
        };

        let result = coordinator.initiate_migration(request.clone()).await;
        assert!(result.is_ok());

        let metrics = coordinator.get_migration_metrics([2u8; 16]).await;
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert!(m.success);
        assert_eq!(m.strategy, MigrationStrategy::PostCopy);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_hybrid_migration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let coordinator = MigrationCoordinator::new(node.clone());

        let request = MigrationRequest {
            agent_id: [3u8; 16],
            source_node: *node.id(),
            target_node: NodeId::new_v4(),
            target_address: "127.0.0.1:8080".parse().unwrap(),
            strategy: MigrationStrategy::Hybrid,
            priority: 5,
        };

        let result = coordinator.initiate_migration(request.clone()).await;
        assert!(result.is_ok());

        let metrics = coordinator.get_migration_metrics([3u8; 16]).await;
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert!(m.success);
        assert!(m.precopy_iterations > 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_migration_history() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let coordinator = MigrationCoordinator::new(node.clone());

        // Perform multiple migrations
        for i in 0..3 {
            let request = MigrationRequest {
                agent_id: [i; 16],
                source_node: *node.id(),
                target_node: NodeId::new_v4(),
                target_address: "127.0.0.1:8080".parse().unwrap(),
                strategy: MigrationStrategy::PreCopy,
                priority: 5,
            };
            coordinator.initiate_migration(request).await.unwrap();
        }

        let history = coordinator.get_migration_history(10).await;
        assert_eq!(history.len(), 3);

        let stats = coordinator.get_migration_stats().await;
        assert_eq!(stats.total_migrations, 3);
        assert_eq!(stats.successful_migrations, 3);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_migration_phases() {
        let agent_id = [1u8; 16];
        let source = NodeId::new_v4();
        let target = NodeId::new_v4();

        let mut metrics =
            MigrationMetrics::new(agent_id, source, target, MigrationStrategy::PreCopy);

        assert_eq!(metrics.phase, MigrationPhase::Planning);

        metrics.update_phase(MigrationPhase::PreCopy { iteration: 0 });
        assert_eq!(metrics.phase, MigrationPhase::PreCopy { iteration: 0 });

        metrics.update_phase(MigrationPhase::Pausing);
        assert_eq!(metrics.phase, MigrationPhase::Pausing);

        metrics.update_phase(MigrationPhase::Complete);
        assert_eq!(metrics.phase, MigrationPhase::Complete);
    }
}
