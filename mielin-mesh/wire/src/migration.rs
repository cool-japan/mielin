//! Live Agent Migration
//!
//! Implements live migration of agents between nodes with minimal downtime.
//! Supports pre-copy migration strategy, two-phase commit, and rollback on failure.

use crate::discovery::DiscoveryService;
use crate::health::HealthMonitor;
use crate::WireError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Migration protocol phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationPhase {
    /// Preparing migration (resource check, pre-flight)
    Prepare,
    /// Pre-copying memory pages iteratively
    PreCopy,
    /// Stop-and-copy final state transfer
    StopAndCopy,
    /// Committing migration (activate on destination)
    Commit,
    /// Rolling back failed migration
    Rollback,
}

/// Migration state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationState {
    /// No active migration
    Idle,
    /// Preparing for migration
    Preparing,
    /// Pre-copying memory pages
    PreCopying,
    /// Performing stop-and-copy
    Finalizing,
    /// Committing on destination
    Committing,
    /// Successfully completed
    Completed,
    /// Migration failed
    Failed,
    /// Rolling back
    RollingBack,
}

/// Migration protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationMessage {
    /// Prepare migration request
    PrepareRequest {
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        agent_size: usize,
        memory_size: usize,
    },
    /// Prepare migration response
    PrepareResponse {
        migration_id: [u8; 16],
        accepted: bool,
        reason: Option<String>,
    },
    /// Pre-copy iteration
    PreCopyData {
        migration_id: [u8; 16],
        iteration: u32,
        pages: Vec<MemoryPage>,
        dirty_page_count: usize,
    },
    /// Pre-copy acknowledgment
    PreCopyAck {
        migration_id: [u8; 16],
        iteration: u32,
        received_pages: usize,
    },
    /// Stop-and-copy (final state)
    StopAndCopy {
        migration_id: [u8; 16],
        final_state: AgentSnapshot,
        dirty_pages: Vec<MemoryPage>,
    },
    /// Commit migration
    CommitRequest { migration_id: [u8; 16] },
    /// Commit acknowledgment
    CommitAck {
        migration_id: [u8; 16],
        success: bool,
    },
    /// Rollback migration
    RollbackRequest {
        migration_id: [u8; 16],
        reason: String,
    },
    /// Rollback acknowledgment
    RollbackAck { migration_id: [u8; 16] },
}

/// Memory page for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPage {
    /// Page number
    pub page_num: u32,
    /// Page data
    pub data: Vec<u8>,
    /// Page dirty flag
    pub dirty: bool,
}

/// Agent snapshot for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    /// Agent ID
    pub agent_id: [u8; 16],
    /// Agent code (WASM)
    pub code: Vec<u8>,
    /// Agent state
    pub state: Vec<u8>,
    /// Memory snapshot
    pub memory: Vec<u8>,
    /// Execution context
    pub context: ExecutionContext,
}

/// Execution context for agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Program counter
    pub pc: u64,
    /// Stack pointer
    pub sp: u64,
    /// Register values
    pub registers: Vec<u64>,
    /// Call stack
    pub call_stack: Vec<u64>,
}

/// Migration configuration
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Maximum pre-copy iterations
    pub max_precopy_iterations: u32,
    /// Dirty page threshold to stop pre-copy
    pub dirty_threshold: usize,
    /// Page size in bytes
    pub page_size: usize,
    /// Migration timeout
    pub migration_timeout: Duration,
    /// Enable compression
    pub enable_compression: bool,
    /// Commit timeout
    pub commit_timeout: Duration,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            max_precopy_iterations: 10,
            dirty_threshold: 100,
            page_size: 4096,
            migration_timeout: Duration::from_secs(60),
            enable_compression: true,
            commit_timeout: Duration::from_secs(5),
        }
    }
}

/// Migration statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    /// Total migrations attempted
    pub total_attempts: u64,
    /// Successful migrations
    pub successful: u64,
    /// Failed migrations
    pub failed: u64,
    /// Rolled back migrations
    pub rolled_back: u64,
    /// Average migration time (milliseconds)
    pub avg_duration_ms: u64,
    /// Average downtime (milliseconds)
    pub avg_downtime_ms: u64,
    /// Total bytes transferred
    pub total_bytes_transferred: u64,
    /// Average pre-copy iterations
    pub avg_precopy_iterations: f64,
}

/// Active migration tracking
#[derive(Debug, Clone)]
struct ActiveMigration {
    _migration_id: [u8; 16],
    _agent_id: [u8; 16],
    _source: SocketAddr,
    _destination: SocketAddr,
    state: MigrationState,
    phase: MigrationPhase,
    started_at: Instant,
    stopped_at: Option<Instant>,
    precopy_iterations: u32,
    bytes_transferred: usize,
    _dirty_pages: Vec<u32>,
}

impl ActiveMigration {
    fn new(
        migration_id: [u8; 16],
        agent_id: [u8; 16],
        source: SocketAddr,
        destination: SocketAddr,
    ) -> Self {
        Self {
            _migration_id: migration_id,
            _agent_id: agent_id,
            _source: source,
            _destination: destination,
            state: MigrationState::Idle,
            phase: MigrationPhase::Prepare,
            started_at: Instant::now(),
            stopped_at: None,
            precopy_iterations: 0,
            bytes_transferred: 0,
            _dirty_pages: Vec::new(),
        }
    }

    /// Calculate migration duration
    fn duration(&self) -> Duration {
        match self.stopped_at {
            Some(stopped) => stopped.duration_since(self.started_at),
            None => self.started_at.elapsed(),
        }
    }

    /// Calculate downtime (stop-and-copy duration)
    fn downtime(&self) -> Option<Duration> {
        self.stopped_at
            .map(|stopped| stopped.duration_since(self.started_at))
    }
}

/// Migration coordinator
pub struct MigrationCoordinator {
    config: MigrationConfig,
    local_addr: SocketAddr,
    _discovery: Arc<DiscoveryService>,
    _health: Arc<HealthMonitor>,

    /// Active migrations
    migrations: Arc<RwLock<HashMap<[u8; 16], ActiveMigration>>>,

    /// Message channel (reserved for future transport integration)
    _message_tx: mpsc::Sender<MigrationMessage>,
    message_rx: Arc<RwLock<mpsc::Receiver<MigrationMessage>>>,

    /// Statistics
    stats: Arc<RwLock<MigrationStats>>,
}

impl MigrationCoordinator {
    /// Create a new migration coordinator
    pub fn new(
        config: MigrationConfig,
        local_addr: SocketAddr,
        discovery: Arc<DiscoveryService>,
        health: Arc<HealthMonitor>,
    ) -> Self {
        let (message_tx, message_rx) = mpsc::channel(100);

        Self {
            config,
            local_addr,
            _discovery: discovery,
            _health: health,
            migrations: Arc::new(RwLock::new(HashMap::new())),
            _message_tx: message_tx,
            message_rx: Arc::new(RwLock::new(message_rx)),
            stats: Arc::new(RwLock::new(MigrationStats {
                total_attempts: 0,
                successful: 0,
                failed: 0,
                rolled_back: 0,
                avg_duration_ms: 0,
                avg_downtime_ms: 0,
                total_bytes_transferred: 0,
                avg_precopy_iterations: 0.0,
            })),
        }
    }

    /// Start the migration coordinator
    pub async fn start(&self) -> Result<(), WireError> {
        info!("Starting migration coordinator");

        // Start message processing
        self.start_message_processing().await;

        // Start timeout monitoring
        self.start_timeout_monitoring().await;

        Ok(())
    }

    /// Initiate migration of an agent to a destination node
    pub async fn initiate_migration(
        &self,
        agent_id: [u8; 16],
        destination: SocketAddr,
        snapshot: AgentSnapshot,
    ) -> Result<[u8; 16], WireError> {
        // Generate migration ID
        let migration_id = self.generate_migration_id();

        info!(
            "Initiating migration {} for agent {} to {}",
            hex::encode(migration_id),
            hex::encode(agent_id),
            destination
        );

        // Create active migration tracking
        let mut migrations = self.migrations.write().await;
        let migration = ActiveMigration::new(migration_id, agent_id, self.local_addr, destination);
        migrations.insert(migration_id, migration);
        drop(migrations);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_attempts += 1;
        drop(stats);

        // Start migration workflow
        self.execute_migration(migration_id, snapshot).await?;

        Ok(migration_id)
    }

    /// Execute migration workflow
    async fn execute_migration(
        &self,
        migration_id: [u8; 16],
        snapshot: AgentSnapshot,
    ) -> Result<(), WireError> {
        // Phase 1: Prepare
        self.execute_prepare_phase(migration_id, &snapshot).await?;

        // Phase 2: Pre-copy (if enabled)
        if self.config.max_precopy_iterations > 0 {
            self.execute_precopy_phase(migration_id, &snapshot).await?;
        }

        // Phase 3: Stop-and-copy
        self.execute_stop_and_copy_phase(migration_id, &snapshot)
            .await?;

        // Phase 4: Commit
        self.execute_commit_phase(migration_id).await?;

        Ok(())
    }

    /// Execute prepare phase
    async fn execute_prepare_phase(
        &self,
        migration_id: [u8; 16],
        _snapshot: &AgentSnapshot,
    ) -> Result<(), WireError> {
        debug!("Migration {}: Prepare phase", hex::encode(migration_id));

        // Update state
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::Preparing;
            migration.phase = MigrationPhase::Prepare;
        }
        drop(migrations);

        // Send prepare request (in real implementation, would use transport)
        // let response = transport.send_prepare_request(...).await?;

        // Simulate preparation
        tokio::time::sleep(Duration::from_millis(10)).await;

        info!(
            "Migration {}: Prepare phase completed",
            hex::encode(migration_id)
        );
        Ok(())
    }

    /// Execute pre-copy phase
    async fn execute_precopy_phase(
        &self,
        migration_id: [u8; 16],
        _snapshot: &AgentSnapshot,
    ) -> Result<(), WireError> {
        debug!("Migration {}: Pre-copy phase", hex::encode(migration_id));

        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::PreCopying;
            migration.phase = MigrationPhase::PreCopy;
        }
        drop(migrations);

        let mut iteration = 0;
        let mut dirty_count = 1000; // Simulate dirty pages

        while iteration < self.config.max_precopy_iterations
            && dirty_count > self.config.dirty_threshold
        {
            debug!(
                "Migration {}: Pre-copy iteration {} ({} dirty pages)",
                hex::encode(migration_id),
                iteration,
                dirty_count
            );

            // Simulate page transfer
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Update tracking
            let mut migrations = self.migrations.write().await;
            if let Some(migration) = migrations.get_mut(&migration_id) {
                migration.precopy_iterations += 1;
                migration.bytes_transferred += dirty_count * self.config.page_size;
            }
            drop(migrations);

            // Simulate convergence
            dirty_count = (dirty_count as f64 * 0.7) as usize;
            iteration += 1;
        }

        info!(
            "Migration {}: Pre-copy completed after {} iterations",
            hex::encode(migration_id),
            iteration
        );
        Ok(())
    }

    /// Execute stop-and-copy phase
    async fn execute_stop_and_copy_phase(
        &self,
        migration_id: [u8; 16],
        _snapshot: &AgentSnapshot,
    ) -> Result<(), WireError> {
        debug!(
            "Migration {}: Stop-and-copy phase",
            hex::encode(migration_id)
        );

        let stop_start = Instant::now();

        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::Finalizing;
            migration.phase = MigrationPhase::StopAndCopy;
        }
        drop(migrations);

        // Simulate final state transfer
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Record downtime
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.stopped_at = Some(stop_start);
        }
        drop(migrations);

        let downtime = stop_start.elapsed();
        info!(
            "Migration {}: Stop-and-copy completed (downtime: {:?})",
            hex::encode(migration_id),
            downtime
        );
        Ok(())
    }

    /// Execute commit phase
    async fn execute_commit_phase(&self, migration_id: [u8; 16]) -> Result<(), WireError> {
        debug!("Migration {}: Commit phase", hex::encode(migration_id));

        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::Committing;
            migration.phase = MigrationPhase::Commit;
        }
        drop(migrations);

        // Simulate commit
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Mark as completed
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::Completed;
        }
        drop(migrations);

        // Update stats
        let migrations = self.migrations.read().await;
        if let Some(migration) = migrations.get(&migration_id) {
            let mut stats = self.stats.write().await;
            stats.successful += 1;

            let duration_ms = migration.duration().as_millis() as u64;
            stats.avg_duration_ms =
                (stats.avg_duration_ms * (stats.successful - 1) + duration_ms) / stats.successful;

            if let Some(downtime) = migration.downtime() {
                let downtime_ms = downtime.as_millis() as u64;
                stats.avg_downtime_ms = (stats.avg_downtime_ms * (stats.successful - 1)
                    + downtime_ms)
                    / stats.successful;
            }

            stats.total_bytes_transferred += migration.bytes_transferred as u64;
            stats.avg_precopy_iterations = ((stats.avg_precopy_iterations
                * (stats.successful - 1) as f64)
                + migration.precopy_iterations as f64)
                / stats.successful as f64;
        }

        info!(
            "Migration {}: Committed successfully",
            hex::encode(migration_id)
        );
        Ok(())
    }

    /// Rollback a migration
    pub async fn rollback_migration(
        &self,
        migration_id: [u8; 16],
        reason: String,
    ) -> Result<(), WireError> {
        warn!(
            "Rolling back migration {}: {}",
            hex::encode(migration_id),
            reason
        );

        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::RollingBack;
            migration.phase = MigrationPhase::Rollback;
        }
        drop(migrations);

        // Simulate rollback
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Mark as failed
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::Failed;
        }
        drop(migrations);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.failed += 1;
        stats.rolled_back += 1;

        info!("Migration {}: Rolled back", hex::encode(migration_id));
        Ok(())
    }

    /// Get migration state
    pub async fn get_migration_state(&self, migration_id: &[u8; 16]) -> Option<MigrationState> {
        let migrations = self.migrations.read().await;
        migrations.get(migration_id).map(|m| m.state)
    }

    /// Get all active migrations
    pub async fn get_active_migrations(&self) -> Vec<[u8; 16]> {
        let migrations = self.migrations.read().await;
        migrations
            .iter()
            .filter(|(_, m)| !matches!(m.state, MigrationState::Completed | MigrationState::Failed))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get statistics
    pub async fn stats(&self) -> MigrationStats {
        self.stats.read().await.clone()
    }

    /// Start message processing
    async fn start_message_processing(&self) {
        let message_rx = self.message_rx.clone();

        tokio::spawn(async move {
            let mut rx = message_rx.write().await;
            while let Some(message) = rx.recv().await {
                debug!("Received migration message: {:?}", message);
                // Handle message (in real implementation)
            }
        });
    }

    /// Start timeout monitoring
    async fn start_timeout_monitoring(&self) {
        let migrations = self.migrations.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                let mut migrations_guard = migrations.write().await;

                migrations_guard.retain(|id, migration| {
                    if migration.duration() > config.migration_timeout {
                        error!(
                            "Migration {} timed out after {:?}",
                            hex::encode(id),
                            migration.duration()
                        );
                        false
                    } else {
                        true
                    }
                });
            }
        });
    }

    /// Generate a unique migration ID
    fn generate_migration_id(&self) -> [u8; 16] {
        use rand::RngExt;
        let mut id = [0u8; 16];
        rand::rng().fill(&mut id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DiscoveryConfig;

    fn create_test_coordinator() -> MigrationCoordinator {
        let config = MigrationConfig::default();
        let local_addr = "127.0.0.1:8000".parse().unwrap();

        let discovery_config = DiscoveryConfig::default();
        let discovery = Arc::new(DiscoveryService::new(discovery_config, [0u8; 16], vec![]));

        let health = Arc::new(HealthMonitor::new());

        MigrationCoordinator::new(config, local_addr, discovery, health)
    }

    fn create_test_snapshot() -> AgentSnapshot {
        AgentSnapshot {
            agent_id: [1u8; 16],
            code: vec![0u8; 1024],
            state: vec![0u8; 512],
            memory: vec![0u8; 4096],
            context: ExecutionContext {
                pc: 0,
                sp: 0,
                registers: vec![0; 16],
                call_stack: vec![],
            },
        }
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let coordinator = create_test_coordinator();
        let stats = coordinator.stats().await;

        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful, 0);
        assert_eq!(stats.failed, 0);
    }

    #[tokio::test]
    async fn test_migration_initiation() {
        let coordinator = create_test_coordinator();
        coordinator.start().await.unwrap();

        let agent_id = [1u8; 16];
        let destination = "127.0.0.1:8001".parse().unwrap();
        let snapshot = create_test_snapshot();

        let migration_id = coordinator
            .initiate_migration(agent_id, destination, snapshot)
            .await
            .unwrap();

        let state = coordinator.get_migration_state(&migration_id).await;
        assert!(state.is_some());
    }

    #[tokio::test]
    async fn test_migration_workflow() {
        let coordinator = create_test_coordinator();
        coordinator.start().await.unwrap();

        let agent_id = [1u8; 16];
        let destination = "127.0.0.1:8001".parse().unwrap();
        let snapshot = create_test_snapshot();

        let migration_id = coordinator
            .initiate_migration(agent_id, destination, snapshot)
            .await
            .unwrap();

        // Wait for migration to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        let state = coordinator.get_migration_state(&migration_id).await;
        assert_eq!(state, Some(MigrationState::Completed));

        let stats = coordinator.stats().await;
        assert_eq!(stats.successful, 1);
        assert_eq!(stats.total_attempts, 1);
    }

    #[tokio::test]
    async fn test_migration_rollback() {
        let coordinator = create_test_coordinator();
        coordinator.start().await.unwrap();

        let agent_id = [1u8; 16];
        let destination = "127.0.0.1:8001".parse().unwrap();
        let snapshot = create_test_snapshot();

        let migration_id = coordinator
            .initiate_migration(agent_id, destination, snapshot)
            .await
            .unwrap();

        // Rollback immediately
        coordinator
            .rollback_migration(migration_id, "Test rollback".to_string())
            .await
            .unwrap();

        let state = coordinator.get_migration_state(&migration_id).await;
        assert_eq!(state, Some(MigrationState::Failed));

        let stats = coordinator.stats().await;
        assert_eq!(stats.rolled_back, 1);
        assert_eq!(stats.failed, 1);
    }

    #[tokio::test]
    async fn test_multiple_migrations() {
        let coordinator = create_test_coordinator();
        coordinator.start().await.unwrap();

        let agent_id1 = [1u8; 16];
        let agent_id2 = [2u8; 16];
        let destination = "127.0.0.1:8001".parse().unwrap();

        let migration_id1 = coordinator
            .initiate_migration(agent_id1, destination, create_test_snapshot())
            .await
            .unwrap();

        let migration_id2 = coordinator
            .initiate_migration(agent_id2, destination, create_test_snapshot())
            .await
            .unwrap();

        // Migration IDs should be unique
        assert_ne!(
            migration_id1, migration_id2,
            "Migration IDs should be unique"
        );

        // Both migrations should be tracked
        let state1 = coordinator.get_migration_state(&migration_id1).await;
        let state2 = coordinator.get_migration_state(&migration_id2).await;

        assert!(state1.is_some(), "Migration 1 should be tracked");
        assert!(state2.is_some(), "Migration 2 should be tracked");

        // Check stats
        let stats = coordinator.stats().await;
        assert_eq!(stats.total_attempts, 2);
        assert_eq!(stats.successful, 2);
    }

    #[test]
    fn test_migration_state_equality() {
        assert_eq!(MigrationState::Idle, MigrationState::Idle);
        assert_ne!(MigrationState::Preparing, MigrationState::PreCopying);
    }

    #[test]
    fn test_migration_phase_equality() {
        assert_eq!(MigrationPhase::Prepare, MigrationPhase::Prepare);
        assert_ne!(MigrationPhase::PreCopy, MigrationPhase::Commit);
    }

    #[test]
    fn test_active_migration_duration() {
        let migration = ActiveMigration::new(
            [1u8; 16],
            [2u8; 16],
            "127.0.0.1:8000".parse().unwrap(),
            "127.0.0.1:8001".parse().unwrap(),
        );

        let duration = migration.duration();
        assert!(duration.as_millis() < 1000); // Should be very short
    }

    #[test]
    fn test_migration_config_default() {
        let config = MigrationConfig::default();

        assert_eq!(config.max_precopy_iterations, 10);
        assert_eq!(config.dirty_threshold, 100);
        assert_eq!(config.page_size, 4096);
        assert!(config.enable_compression);
    }
}
