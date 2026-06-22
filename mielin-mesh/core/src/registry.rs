//! Distributed Agent Registry
//!
//! Provides DHT-based agent location tracking and discovery across the mesh.
//! Enables agents to be located and migrated between nodes efficiently.
//!
//! # Features
//!
//! - **Sharding**: Distributes agents across shards based on agent ID hash
//! - **Pagination**: Supports paginated queries for large result sets
//! - **Background Replication**: Asynchronously replicates entries to peer nodes
//! - **Hot-spot Mitigation**: Tracks and redistributes hot keys

use crate::{dht::Dht, Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Agent ID type (content-addressable hash)
pub type AgentId = [u8; 16];

/// Registry entry TTL - entries expire after this duration
const REGISTRY_TTL: Duration = Duration::from_secs(600); // 10 minutes

/// Replication factor - number of nodes to store each agent location
const REPLICATION_FACTOR: usize = 3;

/// Default number of shards
const DEFAULT_SHARD_COUNT: usize = 16;

/// Default page size for pagination
const DEFAULT_PAGE_SIZE: usize = 100;

/// Hot key threshold (queries per minute)
const HOT_KEY_THRESHOLD: u64 = 100;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Agent not found: {0:?}")]
    AgentNotFound(AgentId),
    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),
    #[error("Invalid registry entry: {0}")]
    InvalidEntry(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Agent location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLocation {
    pub agent_id: AgentId,
    pub node_id: NodeId,
    pub address: SocketAddr,
    pub registered_at: SystemTime,
    pub last_updated: SystemTime,
    pub metadata: HashMap<String, String>,
}

impl AgentLocation {
    pub fn new(agent_id: AgentId, node_id: NodeId, address: SocketAddr) -> Self {
        let now = SystemTime::now();
        Self {
            agent_id,
            node_id,
            address,
            registered_at: now,
            last_updated: now,
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn is_expired(&self) -> bool {
        self.last_updated.elapsed().unwrap_or_default() > REGISTRY_TTL
    }

    pub fn age(&self) -> Duration {
        self.registered_at.elapsed().unwrap_or_default()
    }
}

// =============================================================================
// Sharding
// =============================================================================

/// Configuration for registry sharding
#[derive(Debug, Clone, Copy)]
pub struct ShardConfig {
    /// Number of shards
    pub shard_count: usize,
    /// Maximum entries per shard before triggering redistribution
    pub max_entries_per_shard: usize,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_count: DEFAULT_SHARD_COUNT,
            max_entries_per_shard: 10000,
        }
    }
}

impl ShardConfig {
    /// Create config for small clusters
    pub fn small() -> Self {
        Self {
            shard_count: 4,
            max_entries_per_shard: 1000,
        }
    }

    /// Create config for large clusters
    pub fn large() -> Self {
        Self {
            shard_count: 64,
            max_entries_per_shard: 50000,
        }
    }
}

/// A single shard containing agent locations
#[derive(Debug)]
pub struct RegistryShard {
    /// Shard ID
    id: usize,
    /// Agents in this shard
    agents: HashMap<AgentId, AgentLocation>,
    /// Statistics
    stats: ShardStats,
}

/// Statistics for a shard
#[derive(Debug, Default)]
pub struct ShardStats {
    /// Number of lookups
    pub lookups: AtomicU64,
    /// Number of inserts
    pub inserts: AtomicU64,
    /// Number of deletes
    pub deletes: AtomicU64,
    /// Last access time (epoch seconds)
    pub last_access: AtomicU64,
}

impl RegistryShard {
    /// Create a new shard
    pub fn new(id: usize) -> Self {
        Self {
            id,
            agents: HashMap::new(),
            stats: ShardStats::default(),
        }
    }

    /// Get shard ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Get agent count
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Insert an agent
    pub fn insert(&mut self, agent_id: AgentId, location: AgentLocation) {
        self.agents.insert(agent_id, location);
        self.stats.inserts.fetch_add(1, Ordering::Relaxed);
        self.update_access_time();
    }

    /// Get an agent
    pub fn get(&self, agent_id: &AgentId) -> Option<&AgentLocation> {
        self.stats.lookups.fetch_add(1, Ordering::Relaxed);
        self.agents.get(agent_id)
    }

    /// Get mutable reference to agent
    pub fn get_mut(&mut self, agent_id: &AgentId) -> Option<&mut AgentLocation> {
        self.stats.lookups.fetch_add(1, Ordering::Relaxed);
        self.agents.get_mut(agent_id)
    }

    /// Remove an agent
    pub fn remove(&mut self, agent_id: &AgentId) -> Option<AgentLocation> {
        self.stats.deletes.fetch_add(1, Ordering::Relaxed);
        self.agents.remove(agent_id)
    }

    /// Check if shard contains agent
    pub fn contains(&self, agent_id: &AgentId) -> bool {
        self.agents.contains_key(agent_id)
    }

    /// Get all agents in shard
    pub fn agents(&self) -> impl Iterator<Item = (&AgentId, &AgentLocation)> {
        self.agents.iter()
    }

    /// Cleanup expired entries
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.agents.len();
        self.agents.retain(|_, loc| !loc.is_expired());
        before - self.agents.len()
    }

    fn update_access_time(&self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.stats.last_access.store(now, Ordering::Relaxed);
    }
}

/// Sharded registry for scalability
#[derive(Debug)]
pub struct ShardedRegistry {
    /// Configuration
    config: ShardConfig,
    /// Shards
    shards: Vec<RwLock<RegistryShard>>,
}

impl ShardedRegistry {
    /// Create a new sharded registry
    pub fn new(config: ShardConfig) -> Self {
        let shards = (0..config.shard_count)
            .map(|i| RwLock::new(RegistryShard::new(i)))
            .collect();
        Self { config, shards }
    }

    /// Get shard index for an agent ID
    pub fn shard_for(&self, agent_id: &AgentId) -> usize {
        // Use FNV-1a hash for better distribution
        let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
        for byte in agent_id.iter() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3); // FNV prime
        }
        (hash as usize) % self.config.shard_count
    }

    /// Insert an agent
    pub async fn insert(&self, agent_id: AgentId, location: AgentLocation) {
        let shard_idx = self.shard_for(&agent_id);
        let mut shard = self.shards[shard_idx].write().await;
        shard.insert(agent_id, location);
    }

    /// Get an agent
    pub async fn get(&self, agent_id: &AgentId) -> Option<AgentLocation> {
        let shard_idx = self.shard_for(agent_id);
        let shard = self.shards[shard_idx].read().await;
        shard.get(agent_id).cloned()
    }

    /// Remove an agent
    pub async fn remove(&self, agent_id: &AgentId) -> Option<AgentLocation> {
        let shard_idx = self.shard_for(agent_id);
        let mut shard = self.shards[shard_idx].write().await;
        shard.remove(agent_id)
    }

    /// Get total agent count across all shards
    pub async fn total_count(&self) -> usize {
        let mut total = 0;
        for shard in &self.shards {
            total += shard.read().await.len();
        }
        total
    }

    /// Get shard statistics
    pub async fn shard_stats(&self) -> Vec<(usize, usize, u64)> {
        let mut stats = Vec::new();
        for (i, shard) in self.shards.iter().enumerate() {
            let s = shard.read().await;
            let lookups = s.stats.lookups.load(Ordering::Relaxed);
            stats.push((i, s.len(), lookups));
        }
        stats
    }

    /// Cleanup expired entries in all shards
    pub async fn cleanup_expired(&self) -> usize {
        let mut total = 0;
        for shard in &self.shards {
            let mut s = shard.write().await;
            total += s.cleanup_expired();
        }
        total
    }

    /// Get all agents (for iteration)
    pub async fn all_agents(&self) -> Vec<AgentLocation> {
        let mut all = Vec::new();
        for shard in &self.shards {
            let s = shard.read().await;
            all.extend(s.agents.values().cloned());
        }
        all
    }
}

// =============================================================================
// Pagination
// =============================================================================

/// Paginated query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResult<T> {
    /// Items in this page
    pub items: Vec<T>,
    /// Total number of items
    pub total: usize,
    /// Current page (0-indexed)
    pub page: usize,
    /// Page size
    pub page_size: usize,
    /// Whether there are more pages
    pub has_more: bool,
}

impl<T> PagedResult<T> {
    /// Create a new paged result
    pub fn new(items: Vec<T>, total: usize, page: usize, page_size: usize) -> Self {
        let has_more = (page + 1) * page_size < total;
        Self {
            items,
            total,
            page,
            page_size,
            has_more,
        }
    }

    /// Get total number of pages
    pub fn total_pages(&self) -> usize {
        if self.page_size == 0 {
            0
        } else {
            self.total.div_ceil(self.page_size)
        }
    }

    /// Check if this is the first page
    pub fn is_first(&self) -> bool {
        self.page == 0
    }

    /// Check if this is the last page
    pub fn is_last(&self) -> bool {
        !self.has_more
    }
}

/// Query options for paginated requests
#[derive(Debug, Clone, Copy)]
pub struct QueryOptions {
    /// Page number (0-indexed)
    pub page: usize,
    /// Page size
    pub page_size: usize,
    /// Sort order
    pub sort_by: SortOrder,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            page: 0,
            page_size: DEFAULT_PAGE_SIZE,
            sort_by: SortOrder::ByAge,
        }
    }
}

impl QueryOptions {
    /// Create options for first page
    pub fn first_page(page_size: usize) -> Self {
        Self {
            page: 0,
            page_size,
            sort_by: SortOrder::ByAge,
        }
    }

    /// Get next page options
    pub fn next_page(&self) -> Self {
        Self {
            page: self.page + 1,
            page_size: self.page_size,
            sort_by: self.sort_by,
        }
    }

    /// Calculate offset
    pub fn offset(&self) -> usize {
        self.page * self.page_size
    }
}

/// Sort order for queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Sort by registration age (oldest first)
    ByAge,
    /// Sort by last update (most recent first)
    ByLastUpdate,
    /// Sort by agent ID
    ById,
}

// =============================================================================
// Hot-spot Mitigation
// =============================================================================

/// Tracks access patterns for hot-spot detection
#[derive(Debug)]
pub struct AccessTracker {
    /// Access counts per agent (agent_id -> count in current window)
    counts: HashMap<AgentId, u64>,
    /// Window start time
    window_start: Instant,
    /// Window duration
    window_duration: Duration,
    /// Threshold for hot key detection
    threshold: u64,
}

impl AccessTracker {
    /// Create a new access tracker
    pub fn new(threshold: u64) -> Self {
        Self {
            counts: HashMap::new(),
            window_start: Instant::now(),
            window_duration: Duration::from_secs(60),
            threshold,
        }
    }

    /// Record an access
    pub fn record_access(&mut self, agent_id: AgentId) {
        self.maybe_reset_window();
        *self.counts.entry(agent_id).or_insert(0) += 1;
    }

    /// Check if an agent is a hot key
    pub fn is_hot(&self, agent_id: &AgentId) -> bool {
        self.counts.get(agent_id).copied().unwrap_or(0) >= self.threshold
    }

    /// Get all hot keys
    pub fn hot_keys(&self) -> Vec<AgentId> {
        self.counts
            .iter()
            .filter(|(_, &count)| count >= self.threshold)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get access count for an agent
    pub fn get_count(&self, agent_id: &AgentId) -> u64 {
        self.counts.get(agent_id).copied().unwrap_or(0)
    }

    /// Get top N accessed agents
    pub fn top_accessed(&self, n: usize) -> Vec<(AgentId, u64)> {
        let mut entries: Vec<_> = self.counts.iter().map(|(k, v)| (*k, *v)).collect();
        entries.sort_by_key(|x| std::cmp::Reverse(x.1));
        entries.truncate(n);
        entries
    }

    /// Reset the window if expired
    fn maybe_reset_window(&mut self) {
        if self.window_start.elapsed() >= self.window_duration {
            self.counts.clear();
            self.window_start = Instant::now();
        }
    }
}

impl Default for AccessTracker {
    fn default() -> Self {
        Self::new(HOT_KEY_THRESHOLD)
    }
}

/// Hot-spot mitigator for load distribution
#[derive(Debug)]
pub struct HotSpotMitigator {
    /// Access tracker
    tracker: AccessTracker,
    /// Cached copies of hot agents (for read distribution)
    hot_cache: HashMap<AgentId, Vec<NodeId>>,
    /// Number of replicas for hot keys
    replica_count: usize,
}

impl HotSpotMitigator {
    /// Create a new hot-spot mitigator
    pub fn new(threshold: u64, replica_count: usize) -> Self {
        Self {
            tracker: AccessTracker::new(threshold),
            hot_cache: HashMap::new(),
            replica_count,
        }
    }

    /// Record an access and check for hot-spot
    pub fn record_and_check(&mut self, agent_id: AgentId) -> bool {
        self.tracker.record_access(agent_id);
        self.tracker.is_hot(&agent_id)
    }

    /// Get replicas for a hot key
    pub fn get_replicas(&self, agent_id: &AgentId) -> Option<&Vec<NodeId>> {
        self.hot_cache.get(agent_id)
    }

    /// Set replicas for a hot key
    pub fn set_replicas(&mut self, agent_id: AgentId, replicas: Vec<NodeId>) {
        self.hot_cache.insert(agent_id, replicas);
    }

    /// Remove replicas for an agent
    pub fn remove_replicas(&mut self, agent_id: &AgentId) {
        self.hot_cache.remove(agent_id);
    }

    /// Get all currently hot agents
    pub fn hot_agents(&self) -> Vec<AgentId> {
        self.tracker.hot_keys()
    }

    /// Get the desired replica count for hot keys
    pub fn replica_count(&self) -> usize {
        self.replica_count
    }
}

impl Default for HotSpotMitigator {
    fn default() -> Self {
        Self::new(HOT_KEY_THRESHOLD, REPLICATION_FACTOR * 2)
    }
}

// =============================================================================
// Background Replication
// =============================================================================

/// Replication task status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationStatus {
    /// Pending replication
    Pending,
    /// Currently replicating
    InProgress,
    /// Successfully replicated
    Completed,
    /// Replication failed
    Failed,
}

/// A pending replication task
#[derive(Debug, Clone)]
pub struct ReplicationTask {
    /// Agent to replicate
    pub agent_id: AgentId,
    /// Target nodes for replication
    pub targets: Vec<NodeId>,
    /// Status
    pub status: ReplicationStatus,
    /// Number of attempts
    pub attempts: u32,
    /// Maximum attempts
    pub max_attempts: u32,
    /// Created at
    pub created_at: Instant,
}

impl ReplicationTask {
    /// Create a new replication task
    pub fn new(agent_id: AgentId, targets: Vec<NodeId>) -> Self {
        Self {
            agent_id,
            targets,
            status: ReplicationStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            created_at: Instant::now(),
        }
    }

    /// Check if task should be retried
    pub fn should_retry(&self) -> bool {
        self.status == ReplicationStatus::Failed && self.attempts < self.max_attempts
    }

    /// Mark as in progress
    pub fn start(&mut self) {
        self.status = ReplicationStatus::InProgress;
        self.attempts += 1;
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.status = ReplicationStatus::Completed;
    }

    /// Mark as failed
    pub fn fail(&mut self) {
        self.status = ReplicationStatus::Failed;
    }

    /// Get task age
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Manages background replication of registry entries
#[derive(Debug)]
pub struct ReplicationManager {
    /// Pending tasks
    pending: Vec<ReplicationTask>,
    /// Completed task count
    completed_count: u64,
    /// Failed task count
    failed_count: u64,
    /// Maximum pending tasks
    max_pending: usize,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: Vec::new(),
            completed_count: 0,
            failed_count: 0,
            max_pending,
        }
    }

    /// Queue a replication task
    pub fn queue(&mut self, task: ReplicationTask) -> bool {
        if self.pending.len() >= self.max_pending {
            warn!(
                "Replication queue full, dropping task for {:?}",
                task.agent_id
            );
            return false;
        }
        self.pending.push(task);
        true
    }

    /// Get next pending task
    pub fn next_pending(&mut self) -> Option<&mut ReplicationTask> {
        self.pending
            .iter_mut()
            .find(|t| t.status == ReplicationStatus::Pending || t.should_retry())
    }

    /// Complete a task
    pub fn complete_task(&mut self, agent_id: &AgentId) {
        if let Some(task) = self.pending.iter_mut().find(|t| &t.agent_id == agent_id) {
            task.complete();
            self.completed_count += 1;
        }
    }

    /// Fail a task
    pub fn fail_task(&mut self, agent_id: &AgentId) {
        if let Some(task) = self.pending.iter_mut().find(|t| &t.agent_id == agent_id) {
            task.fail();
            if !task.should_retry() {
                self.failed_count += 1;
            }
        }
    }

    /// Cleanup completed and permanently failed tasks
    pub fn cleanup(&mut self) {
        self.pending.retain(|t| {
            t.status == ReplicationStatus::Pending
                || t.status == ReplicationStatus::InProgress
                || t.should_retry()
        });
    }

    /// Get pending task count
    pub fn pending_count(&self) -> usize {
        self.pending
            .iter()
            .filter(|t| t.status == ReplicationStatus::Pending)
            .count()
    }

    /// Get statistics
    pub fn stats(&self) -> ReplicationStats {
        ReplicationStats {
            pending: self.pending_count(),
            in_progress: self
                .pending
                .iter()
                .filter(|t| t.status == ReplicationStatus::InProgress)
                .count(),
            completed: self.completed_count,
            failed: self.failed_count,
        }
    }
}

impl Default for ReplicationManager {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Replication statistics
#[derive(Debug, Clone, Copy)]
pub struct ReplicationStats {
    /// Pending tasks
    pub pending: usize,
    /// In-progress tasks
    pub in_progress: usize,
    /// Completed tasks
    pub completed: u64,
    /// Failed tasks
    pub failed: u64,
}

/// Agent registry messages for DHT-based lookups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryMessage {
    /// Register an agent at a location
    Register { location: AgentLocation },
    /// Update an existing agent location
    Update { location: AgentLocation },
    /// Query for an agent's location
    Query {
        agent_id: AgentId,
        requester: NodeId,
    },
    /// Response with agent location
    QueryResponse {
        agent_id: AgentId,
        location: Option<AgentLocation>,
    },
    /// Remove an agent from registry (after migration)
    Deregister { agent_id: AgentId, node_id: NodeId },
    /// Replicate registry entry to peer nodes
    Replicate { location: AgentLocation },
}

/// Distributed agent registry
pub struct AgentRegistry {
    local_node: Arc<Node>,
    dht: Arc<RwLock<Dht>>,
    local_agents: Arc<RwLock<HashMap<AgentId, AgentLocation>>>,
    remote_cache: Arc<RwLock<HashMap<AgentId, AgentLocation>>>,
}

impl AgentRegistry {
    pub fn new(node: Arc<Node>, dht: Arc<RwLock<Dht>>) -> Self {
        Self {
            local_node: node,
            dht,
            local_agents: Arc::new(RwLock::new(HashMap::new())),
            remote_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the registry maintenance tasks
    pub async fn start(&self) {
        info!("Starting agent registry for node {}", self.local_node.id());
        self.spawn_cleanup_task();
    }

    /// Spawn periodic cleanup task for expired entries
    fn spawn_cleanup_task(&self) {
        let local_agents = self.local_agents.clone();
        let remote_cache = self.remote_cache.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;

                // Clean up expired local entries
                let mut local = local_agents.write().await;
                let expired_local: Vec<AgentId> = local
                    .iter()
                    .filter(|(_, loc)| loc.is_expired())
                    .map(|(id, _)| *id)
                    .collect();

                for id in &expired_local {
                    local.remove(id);
                    debug!("Removed expired local agent: {:?}", id);
                }

                drop(local);

                // Clean up expired cache entries
                let mut cache = remote_cache.write().await;
                let expired_cache: Vec<AgentId> = cache
                    .iter()
                    .filter(|(_, loc)| loc.is_expired())
                    .map(|(id, _)| *id)
                    .collect();

                for id in &expired_cache {
                    cache.remove(id);
                    debug!("Removed expired cached agent: {:?}", id);
                }

                if !expired_local.is_empty() || !expired_cache.is_empty() {
                    info!(
                        "Registry cleanup: removed {} local, {} cached agents",
                        expired_local.len(),
                        expired_cache.len()
                    );
                }
            }
        });
    }

    /// Register a local agent in the registry
    pub async fn register_agent(
        &self,
        agent_id: AgentId,
        address: SocketAddr,
    ) -> Result<(), RegistryError> {
        let location = AgentLocation::new(agent_id, *self.local_node.id(), address);

        let mut local_agents = self.local_agents.write().await;
        local_agents.insert(agent_id, location.clone());
        info!("Registered agent {:?} at {}", agent_id, address);

        drop(local_agents);

        // In a real implementation, this would replicate to peer nodes
        // via DHT for fault tolerance
        Ok(())
    }

    /// Update an agent's location (e.g., after migration)
    pub async fn update_agent_location(
        &self,
        agent_id: AgentId,
        new_node_id: NodeId,
        new_address: SocketAddr,
    ) -> Result<(), RegistryError> {
        // Check if this is a local agent
        let mut local_agents = self.local_agents.write().await;
        if let Some(location) = local_agents.get_mut(&agent_id) {
            location.node_id = new_node_id;
            location.address = new_address;
            location.last_updated = SystemTime::now();
            info!(
                "Updated agent {:?} location to node {} at {}",
                agent_id, new_node_id, new_address
            );
            return Ok(());
        }
        drop(local_agents);

        // Check remote cache
        let mut cache = self.remote_cache.write().await;
        if let Some(location) = cache.get_mut(&agent_id) {
            location.node_id = new_node_id;
            location.address = new_address;
            location.last_updated = SystemTime::now();
            debug!("Updated cached agent {:?} location", agent_id);
            return Ok(());
        }

        Err(RegistryError::AgentNotFound(agent_id))
    }

    /// Deregister an agent (e.g., before migration)
    pub async fn deregister_agent(&self, agent_id: AgentId) -> Result<(), RegistryError> {
        let mut local_agents = self.local_agents.write().await;
        if local_agents.remove(&agent_id).is_some() {
            info!("Deregistered agent {:?}", agent_id);
            Ok(())
        } else {
            Err(RegistryError::AgentNotFound(agent_id))
        }
    }

    /// Query for an agent's location
    pub async fn query_agent(&self, agent_id: AgentId) -> Result<AgentLocation, RegistryError> {
        // Check local agents first
        let local_agents = self.local_agents.read().await;
        if let Some(location) = local_agents.get(&agent_id) {
            debug!("Found agent {:?} locally", agent_id);
            return Ok(location.clone());
        }
        drop(local_agents);

        // Check remote cache
        let cache = self.remote_cache.read().await;
        if let Some(location) = cache.get(&agent_id) {
            if !location.is_expired() {
                debug!("Found agent {:?} in cache", agent_id);
                return Ok(location.clone());
            }
        }
        drop(cache);

        // In a real implementation, this would query the DHT
        // For now, return not found
        Err(RegistryError::AgentNotFound(agent_id))
    }

    /// Get all local agents
    pub async fn get_local_agents(&self) -> Vec<AgentLocation> {
        let local_agents = self.local_agents.read().await;
        local_agents.values().cloned().collect()
    }

    /// Get local agent count
    pub async fn local_agent_count(&self) -> usize {
        let local_agents = self.local_agents.read().await;
        local_agents.len()
    }

    /// Get total agent count (local + cached)
    pub async fn total_agent_count(&self) -> usize {
        let local = self.local_agents.read().await;
        let cache = self.remote_cache.read().await;
        local.len() + cache.len()
    }

    /// Handle registry message from network
    pub async fn handle_message(
        &self,
        message: RegistryMessage,
    ) -> Result<Option<RegistryMessage>, RegistryError> {
        match message {
            RegistryMessage::Register { location } => {
                self.handle_register(location).await?;
                Ok(None)
            }
            RegistryMessage::Update { location } => {
                self.handle_update(location).await?;
                Ok(None)
            }
            RegistryMessage::Query {
                agent_id,
                requester,
            } => {
                let response = self.handle_query(agent_id, requester).await?;
                Ok(Some(response))
            }
            RegistryMessage::QueryResponse { agent_id, location } => {
                self.handle_query_response(agent_id, location).await?;
                Ok(None)
            }
            RegistryMessage::Deregister { agent_id, node_id } => {
                self.handle_deregister(agent_id, node_id).await?;
                Ok(None)
            }
            RegistryMessage::Replicate { location } => {
                self.handle_replicate(location).await?;
                Ok(None)
            }
        }
    }

    /// Handle register message
    async fn handle_register(&self, location: AgentLocation) -> Result<(), RegistryError> {
        let mut cache = self.remote_cache.write().await;
        cache.insert(location.agent_id, location.clone());
        debug!("Cached agent {:?} from registration", location.agent_id);
        Ok(())
    }

    /// Handle update message
    async fn handle_update(&self, location: AgentLocation) -> Result<(), RegistryError> {
        let mut cache = self.remote_cache.write().await;
        if let Some(existing) = cache.get_mut(&location.agent_id) {
            if location.last_updated > existing.last_updated {
                *existing = location;
                debug!("Updated cached agent location");
            }
        } else {
            cache.insert(location.agent_id, location);
            debug!("Added agent to cache from update");
        }
        Ok(())
    }

    /// Handle query message
    async fn handle_query(
        &self,
        agent_id: AgentId,
        _requester: NodeId,
    ) -> Result<RegistryMessage, RegistryError> {
        let location = self.query_agent(agent_id).await.ok();
        Ok(RegistryMessage::QueryResponse { agent_id, location })
    }

    /// Handle query response
    async fn handle_query_response(
        &self,
        agent_id: AgentId,
        location: Option<AgentLocation>,
    ) -> Result<(), RegistryError> {
        if let Some(loc) = location {
            let mut cache = self.remote_cache.write().await;
            cache.insert(agent_id, loc);
            debug!("Cached agent {:?} from query response", agent_id);
        }
        Ok(())
    }

    /// Handle deregister message
    async fn handle_deregister(
        &self,
        agent_id: AgentId,
        _node_id: NodeId,
    ) -> Result<(), RegistryError> {
        let mut cache = self.remote_cache.write().await;
        cache.remove(&agent_id);
        debug!("Removed agent {:?} from cache", agent_id);
        Ok(())
    }

    /// Handle replicate message
    async fn handle_replicate(&self, location: AgentLocation) -> Result<(), RegistryError> {
        let mut cache = self.remote_cache.write().await;
        cache.insert(location.agent_id, location.clone());
        debug!("Replicated agent {:?}", location.agent_id);
        Ok(())
    }

    /// Get responsible nodes for an agent (for DHT replication)
    pub async fn get_responsible_nodes(&self, agent_id: AgentId) -> Vec<NodeId> {
        let dht = self.dht.read().await;

        // Convert agent_id to NodeId for DHT lookup
        let target_id = uuid::Uuid::from_bytes(agent_id);

        // Find closest nodes to the agent ID (use fresh results for replication decisions)
        dht.find_closest_fresh(&target_id, REPLICATION_FACTOR)
    }

    /// Add metadata to a local agent
    pub async fn add_agent_metadata(
        &self,
        agent_id: AgentId,
        key: String,
        value: String,
    ) -> Result<(), RegistryError> {
        let mut local_agents = self.local_agents.write().await;
        if let Some(location) = local_agents.get_mut(&agent_id) {
            location.metadata.insert(key.clone(), value.clone());
            debug!("Added metadata {}={} to agent {:?}", key, value, agent_id);
            Ok(())
        } else {
            Err(RegistryError::AgentNotFound(agent_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dht::PeerInfo, NodeRole};

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_agent_location_creation() {
        let agent_id = [1u8; 16];
        let node_id = NodeId::new_v4();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let location = AgentLocation::new(agent_id, node_id, address);

        assert_eq!(location.agent_id, agent_id);
        assert_eq!(location.node_id, node_id);
        assert_eq!(location.address, address);
        assert!(!location.is_expired());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_registry_creation() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        assert_eq!(registry.local_agent_count().await, 0);
        assert_eq!(registry.total_agent_count().await, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_register_agent() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        let agent_id = [1u8; 16];
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        registry.register_agent(agent_id, address).await.unwrap();

        assert_eq!(registry.local_agent_count().await, 1);

        let location = registry.query_agent(agent_id).await.unwrap();
        assert_eq!(location.agent_id, agent_id);
        assert_eq!(location.address, address);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_deregister_agent() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        let agent_id = [1u8; 16];
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        registry.register_agent(agent_id, address).await.unwrap();
        assert_eq!(registry.local_agent_count().await, 1);

        registry.deregister_agent(agent_id).await.unwrap();
        assert_eq!(registry.local_agent_count().await, 0);

        assert!(registry.query_agent(agent_id).await.is_err());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_update_agent_location() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        let agent_id = [1u8; 16];
        let address1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let address2: SocketAddr = "127.0.0.1:9090".parse().unwrap();
        let new_node_id = NodeId::new_v4();

        registry.register_agent(agent_id, address1).await.unwrap();

        registry
            .update_agent_location(agent_id, new_node_id, address2)
            .await
            .unwrap();

        let location = registry.query_agent(agent_id).await.unwrap();
        assert_eq!(location.address, address2);
        assert_eq!(location.node_id, new_node_id);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_query_nonexistent_agent() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        let agent_id = [1u8; 16];
        let result = registry.query_agent(agent_id).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(RegistryError::AgentNotFound(_))));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_agent_metadata() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        let agent_id = [1u8; 16];
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        registry.register_agent(agent_id, address).await.unwrap();
        registry
            .add_agent_metadata(agent_id, "type".to_string(), "worker".to_string())
            .await
            .unwrap();

        let location = registry.query_agent(agent_id).await.unwrap();
        assert_eq!(location.metadata.get("type"), Some(&"worker".to_string()));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_get_local_agents() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node, dht);

        let agent1 = [1u8; 16];
        let agent2 = [2u8; 16];
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        registry.register_agent(agent1, address).await.unwrap();
        registry.register_agent(agent2, address).await.unwrap();

        let agents = registry.get_local_agents().await;
        assert_eq!(agents.len(), 2);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_registry_message_handling() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));
        let registry = AgentRegistry::new(node.clone(), dht);

        let agent_id = [1u8; 16];
        let location = AgentLocation::new(agent_id, *node.id(), "127.0.0.1:8080".parse().unwrap());

        let msg = RegistryMessage::Register {
            location: location.clone(),
        };

        registry.handle_message(msg).await.unwrap();

        // Agent should be in remote cache
        assert_eq!(registry.total_agent_count().await, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_responsible_nodes() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let mut dht = Dht::new(*node.id());

        // Add some peers
        for _ in 0..5 {
            let peer = PeerInfo::new(NodeId::new_v4(), "127.0.0.1:8080".to_string());
            dht.insert_peer(peer);
        }

        let dht = Arc::new(RwLock::new(dht));
        let registry = AgentRegistry::new(node, dht);

        let agent_id = [1u8; 16];
        let responsible = registry.get_responsible_nodes(agent_id).await;

        assert!(!responsible.is_empty());
        assert!(responsible.len() <= REPLICATION_FACTOR);
    }

    // ==========================================================================
    // Sharding Tests
    // ==========================================================================

    #[test]
    fn test_shard_config_default() {
        let config = ShardConfig::default();
        assert_eq!(config.shard_count, DEFAULT_SHARD_COUNT);
        assert_eq!(config.max_entries_per_shard, 10000);
    }

    #[test]
    fn test_shard_config_presets() {
        let small = ShardConfig::small();
        assert_eq!(small.shard_count, 4);

        let large = ShardConfig::large();
        assert_eq!(large.shard_count, 64);
    }

    #[test]
    fn test_registry_shard_creation() {
        let shard = RegistryShard::new(0);
        assert_eq!(shard.id(), 0);
        assert!(shard.is_empty());
        assert_eq!(shard.len(), 0);
    }

    #[test]
    fn test_registry_shard_operations() {
        let mut shard = RegistryShard::new(0);
        let agent_id = [1u8; 16];
        let node_id = NodeId::new_v4();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let location = AgentLocation::new(agent_id, node_id, address);

        shard.insert(agent_id, location);
        assert_eq!(shard.len(), 1);
        assert!(shard.contains(&agent_id));
        assert!(shard.get(&agent_id).is_some());

        let removed = shard.remove(&agent_id);
        assert!(removed.is_some());
        assert!(shard.is_empty());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_sharded_registry_creation() {
        let registry = ShardedRegistry::new(ShardConfig::default());
        assert_eq!(registry.total_count().await, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_sharded_registry_operations() {
        let registry = ShardedRegistry::new(ShardConfig::default());
        let agent_id = [1u8; 16];
        let node_id = NodeId::new_v4();
        let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let location = AgentLocation::new(agent_id, node_id, address);

        registry.insert(agent_id, location).await;
        assert_eq!(registry.total_count().await, 1);

        let got = registry.get(&agent_id).await;
        assert!(got.is_some());
        assert_eq!(got.unwrap().agent_id, agent_id);

        registry.remove(&agent_id).await;
        assert_eq!(registry.total_count().await, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_sharded_registry_distribution() {
        let registry = ShardedRegistry::new(ShardConfig::default());

        // Insert agents with different IDs
        for i in 0u8..100 {
            let mut agent_id = [0u8; 16];
            agent_id[0] = i;
            let location = AgentLocation::new(
                agent_id,
                NodeId::new_v4(),
                "127.0.0.1:8080".parse().unwrap(),
            );
            registry.insert(agent_id, location).await;
        }

        assert_eq!(registry.total_count().await, 100);

        // Check distribution across shards
        let stats = registry.shard_stats().await;
        let non_empty_shards = stats.iter().filter(|(_, count, _)| *count > 0).count();
        assert!(non_empty_shards > 1); // Should be distributed
    }

    // ==========================================================================
    // Pagination Tests
    // ==========================================================================

    #[test]
    fn test_paged_result_creation() {
        let items = vec![1, 2, 3, 4, 5];
        let result = PagedResult::new(items, 15, 0, 5);

        assert_eq!(result.items.len(), 5);
        assert_eq!(result.total, 15);
        assert_eq!(result.page, 0);
        assert_eq!(result.page_size, 5);
        assert!(result.has_more);
        assert_eq!(result.total_pages(), 3);
    }

    #[test]
    fn test_paged_result_last_page() {
        let items = vec![1, 2, 3];
        let result = PagedResult::new(items, 13, 2, 5);

        assert!(!result.has_more);
        assert!(result.is_last());
    }

    #[test]
    fn test_query_options_default() {
        let opts = QueryOptions::default();
        assert_eq!(opts.page, 0);
        assert_eq!(opts.page_size, DEFAULT_PAGE_SIZE);
        assert_eq!(opts.sort_by, SortOrder::ByAge);
    }

    #[test]
    fn test_query_options_next_page() {
        let opts = QueryOptions::first_page(10);
        let next = opts.next_page();

        assert_eq!(next.page, 1);
        assert_eq!(next.page_size, 10);
    }

    #[test]
    fn test_query_options_offset() {
        let opts = QueryOptions {
            page: 2,
            page_size: 10,
            sort_by: SortOrder::ByAge,
        };
        assert_eq!(opts.offset(), 20);
    }

    // ==========================================================================
    // Hot-spot Mitigation Tests
    // ==========================================================================

    #[test]
    fn test_access_tracker_creation() {
        let tracker = AccessTracker::new(100);
        let agent_id = [1u8; 16];
        assert_eq!(tracker.get_count(&agent_id), 0);
        assert!(!tracker.is_hot(&agent_id));
    }

    #[test]
    fn test_access_tracker_recording() {
        let mut tracker = AccessTracker::new(5);
        let agent_id = [1u8; 16];

        for _ in 0..5 {
            tracker.record_access(agent_id);
        }

        assert_eq!(tracker.get_count(&agent_id), 5);
        assert!(tracker.is_hot(&agent_id));
    }

    #[test]
    fn test_access_tracker_hot_keys() {
        let mut tracker = AccessTracker::new(3);
        let hot_agent = [1u8; 16];
        let cold_agent = [2u8; 16];

        for _ in 0..5 {
            tracker.record_access(hot_agent);
        }
        tracker.record_access(cold_agent);

        let hot = tracker.hot_keys();
        assert_eq!(hot.len(), 1);
        assert_eq!(hot[0], hot_agent);
    }

    #[test]
    fn test_access_tracker_top_accessed() {
        let mut tracker = AccessTracker::new(100);
        let agent1 = [1u8; 16];
        let agent2 = [2u8; 16];

        for _ in 0..10 {
            tracker.record_access(agent1);
        }
        for _ in 0..5 {
            tracker.record_access(agent2);
        }

        let top = tracker.top_accessed(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, agent1);
        assert_eq!(top[0].1, 10);
    }

    #[test]
    fn test_hot_spot_mitigator_creation() {
        let mitigator = HotSpotMitigator::default();
        assert!(mitigator.hot_agents().is_empty());
    }

    #[test]
    fn test_hot_spot_mitigator_detection() {
        let mut mitigator = HotSpotMitigator::new(5, 6);
        let agent_id = [1u8; 16];

        for _ in 0..4 {
            assert!(!mitigator.record_and_check(agent_id));
        }
        assert!(mitigator.record_and_check(agent_id));
    }

    #[test]
    fn test_hot_spot_mitigator_replicas() {
        let mut mitigator = HotSpotMitigator::new(5, 6);
        let agent_id = [1u8; 16];
        let replicas = vec![NodeId::new_v4(), NodeId::new_v4()];

        mitigator.set_replicas(agent_id, replicas.clone());

        let got = mitigator.get_replicas(&agent_id);
        assert!(got.is_some());
        assert_eq!(got.unwrap().len(), 2);

        mitigator.remove_replicas(&agent_id);
        assert!(mitigator.get_replicas(&agent_id).is_none());
    }

    // ==========================================================================
    // Replication Tests
    // ==========================================================================

    #[test]
    fn test_replication_task_creation() {
        let agent_id = [1u8; 16];
        let targets = vec![NodeId::new_v4()];
        let task = ReplicationTask::new(agent_id, targets);

        assert_eq!(task.status, ReplicationStatus::Pending);
        assert_eq!(task.attempts, 0);
        assert!(!task.should_retry());
    }

    #[test]
    fn test_replication_task_lifecycle() {
        let agent_id = [1u8; 16];
        let mut task = ReplicationTask::new(agent_id, vec![NodeId::new_v4()]);

        task.start();
        assert_eq!(task.status, ReplicationStatus::InProgress);
        assert_eq!(task.attempts, 1);

        task.fail();
        assert_eq!(task.status, ReplicationStatus::Failed);
        assert!(task.should_retry());

        task.start();
        task.complete();
        assert_eq!(task.status, ReplicationStatus::Completed);
    }

    #[test]
    fn test_replication_manager_creation() {
        let manager = ReplicationManager::new(100);
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_replication_manager_queue() {
        let mut manager = ReplicationManager::new(100);
        let agent_id = [1u8; 16];
        let task = ReplicationTask::new(agent_id, vec![NodeId::new_v4()]);

        assert!(manager.queue(task));
        assert_eq!(manager.pending_count(), 1);
    }

    #[test]
    fn test_replication_manager_queue_full() {
        let mut manager = ReplicationManager::new(2);

        for i in 0u8..3 {
            let mut agent_id = [0u8; 16];
            agent_id[0] = i;
            let task = ReplicationTask::new(agent_id, vec![NodeId::new_v4()]);
            if i < 2 {
                assert!(manager.queue(task));
            } else {
                assert!(!manager.queue(task));
            }
        }
    }

    #[test]
    fn test_replication_manager_complete() {
        let mut manager = ReplicationManager::new(100);
        let agent_id = [1u8; 16];
        let task = ReplicationTask::new(agent_id, vec![NodeId::new_v4()]);

        manager.queue(task);
        manager.complete_task(&agent_id);

        let stats = manager.stats();
        assert_eq!(stats.completed, 1);
    }

    #[test]
    fn test_replication_manager_fail() {
        let mut manager = ReplicationManager::new(100);
        let agent_id = [1u8; 16];
        let task = ReplicationTask::new(agent_id, vec![NodeId::new_v4()]);

        manager.queue(task);

        // Fail 3 times (max_attempts)
        for _ in 0..3 {
            if let Some(t) = manager.next_pending() {
                t.start();
            }
            manager.fail_task(&agent_id);
        }

        let stats = manager.stats();
        assert_eq!(stats.failed, 1);
    }

    #[test]
    fn test_replication_manager_cleanup() {
        let mut manager = ReplicationManager::new(100);

        let agent1 = [1u8; 16];
        let agent2 = [2u8; 16];

        manager.queue(ReplicationTask::new(agent1, vec![NodeId::new_v4()]));
        manager.queue(ReplicationTask::new(agent2, vec![NodeId::new_v4()]));

        manager.complete_task(&agent1);
        manager.cleanup();

        // Only pending task should remain
        assert_eq!(manager.pending_count(), 1);
    }
}
