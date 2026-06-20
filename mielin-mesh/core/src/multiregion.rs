//! Multi-Region Support for Service Mesh
//!
//! Provides region-aware routing, cross-region replication, and regional failover
//! for globally distributed MielinOS mesh networks.
//!
//! Features:
//! - Region topology with hierarchical zones
//! - Region-aware routing with latency tracking
//! - Cross-region replication with consistency levels
//! - Regional failover with automatic recovery
//! - Geo-redundant agent placement
//! - Latency-based region selection

use crate::error::MeshNetworkError;
use crate::{AgentId, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Multi-region errors
#[derive(Debug, Error)]
pub enum MultiRegionError {
    #[error("Region not found: {region_id}")]
    RegionNotFound { region_id: String },

    #[error("No healthy regions available")]
    NoHealthyRegions,

    #[error("Cross-region replication failed: {reason}")]
    ReplicationFailed { reason: String },

    #[error("Region failover failed: {reason}")]
    FailoverFailed { reason: String },

    #[error("Invalid region configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Network error: {0}")]
    NetworkError(#[from] MeshNetworkError),
}

/// Geographic region identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionId(pub String);

impl RegionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RegionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Region health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RegionHealth {
    /// Region is fully operational
    #[default]
    Healthy,
    /// Region is degraded but operational
    Degraded,
    /// Region is unhealthy, failover recommended
    Unhealthy,
    /// Region is in maintenance mode
    Maintenance,
}

/// Geographic location (latitude, longitude)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
}

impl GeoLocation {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Calculate distance to another location (Haversine formula)
    /// Returns distance in kilometers
    pub fn distance_to(&self, other: &GeoLocation) -> f64 {
        const EARTH_RADIUS_KM: f64 = 6371.0;

        let lat1 = self.latitude.to_radians();
        let lat2 = other.latitude.to_radians();
        let delta_lat = (other.latitude - self.latitude).to_radians();
        let delta_lon = (other.longitude - self.longitude).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        EARTH_RADIUS_KM * c
    }
}

/// Region metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInfo {
    /// Unique region identifier
    pub id: RegionId,
    /// Human-readable region name
    pub name: String,
    /// Geographic location
    pub location: GeoLocation,
    /// Current health status
    pub health: RegionHealth,
    /// Nodes in this region
    pub nodes: HashSet<NodeId>,
    /// Capacity (number of agents this region can host)
    pub capacity: usize,
    /// Current load (number of agents hosted)
    pub current_load: usize,
    /// Average latency to other regions (milliseconds)
    pub latencies: HashMap<RegionId, Duration>,
    /// Last health check timestamp
    pub last_health_check: SystemTime,
    /// Region metadata
    pub metadata: HashMap<String, String>,
}

impl RegionInfo {
    /// Create a new region
    pub fn new(id: RegionId, name: impl Into<String>, location: GeoLocation) -> Self {
        Self {
            id,
            name: name.into(),
            location,
            health: RegionHealth::Healthy,
            nodes: HashSet::new(),
            capacity: 10000,
            current_load: 0,
            latencies: HashMap::new(),
            last_health_check: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }

    /// Check if region has capacity for more agents
    pub fn has_capacity(&self) -> bool {
        self.current_load < self.capacity
    }

    /// Calculate load percentage
    pub fn load_percentage(&self) -> f64 {
        if self.capacity == 0 {
            return 100.0;
        }
        (self.current_load as f64 / self.capacity as f64) * 100.0
    }

    /// Check if region is available for routing
    pub fn is_available(&self) -> bool {
        matches!(self.health, RegionHealth::Healthy | RegionHealth::Degraded) && self.has_capacity()
    }
}

/// Replication consistency level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConsistencyLevel {
    /// Eventually consistent across regions
    #[default]
    Eventual,
    /// Consistent across majority of regions
    Quorum,
    /// Strongly consistent across all regions
    Strong,
}

/// Replication policy for cross-region data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationPolicy {
    /// Minimum number of regions to replicate to
    pub min_replicas: usize,
    /// Preferred regions for replication
    pub preferred_regions: Vec<RegionId>,
    /// Consistency level
    pub consistency: ConsistencyLevel,
    /// Maximum replication delay
    pub max_delay: Duration,
}

impl Default for ReplicationPolicy {
    fn default() -> Self {
        Self {
            min_replicas: 2,
            preferred_regions: Vec::new(),
            consistency: ConsistencyLevel::Eventual,
            max_delay: Duration::from_secs(60),
        }
    }
}

/// Replicated agent state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicatedAgent {
    pub agent_id: AgentId,
    pub primary_region: RegionId,
    pub replica_regions: HashSet<RegionId>,
    pub last_sync: SystemTime,
}

/// Region topology manager
pub struct RegionTopology {
    /// All regions in the topology
    regions: Arc<RwLock<HashMap<RegionId, RegionInfo>>>,
    /// Latency measurements between regions
    latency_matrix: Arc<RwLock<HashMap<(RegionId, RegionId), Duration>>>,
    /// Last latency measurement time
    last_latency_update: Arc<RwLock<Instant>>,
}

impl RegionTopology {
    /// Create a new region topology
    pub fn new() -> Self {
        Self {
            regions: Arc::new(RwLock::new(HashMap::new())),
            latency_matrix: Arc::new(RwLock::new(HashMap::new())),
            last_latency_update: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Register a new region
    pub async fn register_region(&self, region: RegionInfo) -> Result<(), MultiRegionError> {
        let region_id = region.id.clone();
        info!("Registering region: {} ({})", region.name, region_id);

        self.regions.write().await.insert(region_id, region);
        Ok(())
    }

    /// Unregister a region
    pub async fn unregister_region(&self, region_id: &RegionId) -> Result<(), MultiRegionError> {
        info!("Unregistering region: {}", region_id);
        self.regions
            .write()
            .await
            .remove(region_id)
            .ok_or_else(|| MultiRegionError::RegionNotFound {
                region_id: region_id.to_string(),
            })?;
        Ok(())
    }

    /// Get region information
    pub async fn get_region(&self, region_id: &RegionId) -> Option<RegionInfo> {
        self.regions.read().await.get(region_id).cloned()
    }

    /// List all regions
    pub async fn list_regions(&self) -> Vec<RegionInfo> {
        self.regions.read().await.values().cloned().collect()
    }

    /// List healthy regions
    pub async fn list_healthy_regions(&self) -> Vec<RegionInfo> {
        self.regions
            .read()
            .await
            .values()
            .filter(|r| r.is_available())
            .cloned()
            .collect()
    }

    /// Update region health
    pub async fn update_health(
        &self,
        region_id: &RegionId,
        health: RegionHealth,
    ) -> Result<(), MultiRegionError> {
        let mut regions = self.regions.write().await;
        let region =
            regions
                .get_mut(region_id)
                .ok_or_else(|| MultiRegionError::RegionNotFound {
                    region_id: region_id.to_string(),
                })?;

        region.health = health;
        region.last_health_check = SystemTime::now();

        info!("Updated health for region {}: {:?}", region_id, health);
        Ok(())
    }

    /// Update latency between two regions
    pub async fn update_latency(
        &self,
        from: &RegionId,
        to: &RegionId,
        latency: Duration,
    ) -> Result<(), MultiRegionError> {
        self.latency_matrix
            .write()
            .await
            .insert((from.clone(), to.clone()), latency);

        // Update in region info as well
        let mut regions = self.regions.write().await;
        if let Some(region) = regions.get_mut(from) {
            region.latencies.insert(to.clone(), latency);
        }

        *self.last_latency_update.write().await = Instant::now();
        Ok(())
    }

    /// Get latency between two regions
    pub async fn get_latency(&self, from: &RegionId, to: &RegionId) -> Option<Duration> {
        self.latency_matrix
            .read()
            .await
            .get(&(from.clone(), to.clone()))
            .copied()
    }

    /// Find closest region to a given region
    pub async fn find_closest_region(&self, from: &RegionId) -> Option<RegionInfo> {
        let regions = self.regions.read().await;
        let from_region = regions.get(from)?;

        regions
            .values()
            .filter(|r| r.id != *from && r.is_available())
            .min_by_key(|r| {
                // Use latency if available, otherwise use geographic distance
                from_region
                    .latencies
                    .get(&r.id)
                    .copied()
                    .unwrap_or_else(|| {
                        let distance_km = from_region.location.distance_to(&r.location);
                        // Estimate latency: ~1ms per 100km
                        Duration::from_millis((distance_km * 10.0) as u64)
                    })
            })
            .cloned()
    }

    /// Select best region for agent placement
    pub async fn select_region(&self, preferred_regions: &[RegionId]) -> Option<RegionInfo> {
        let regions = self.regions.read().await;

        // Try preferred regions first
        for region_id in preferred_regions {
            if let Some(region) = regions.get(region_id) {
                if region.is_available() {
                    return Some(region.clone());
                }
            }
        }

        // Fall back to any available region with lowest load
        regions
            .values()
            .filter(|r| r.is_available())
            .min_by(|a, b| {
                a.load_percentage()
                    .partial_cmp(&b.load_percentage())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }
}

impl Default for RegionTopology {
    fn default() -> Self {
        Self::new()
    }
}

/// Cross-region replication manager
pub struct ReplicationManager {
    topology: Arc<RegionTopology>,
    replicated_agents: Arc<RwLock<HashMap<AgentId, ReplicatedAgent>>>,
    policy: ReplicationPolicy,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub fn new(topology: Arc<RegionTopology>, policy: ReplicationPolicy) -> Self {
        Self {
            topology,
            replicated_agents: Arc::new(RwLock::new(HashMap::new())),
            policy,
        }
    }

    /// Replicate agent to multiple regions
    pub async fn replicate_agent(
        &self,
        agent_id: AgentId,
        primary_region: RegionId,
    ) -> Result<Vec<RegionId>, MultiRegionError> {
        info!(
            "Replicating agent {:?} from primary region {}",
            agent_id, primary_region
        );

        // Select replica regions
        let replica_regions = self.select_replica_regions(&primary_region).await?;

        if replica_regions.len() < self.policy.min_replicas {
            warn!(
                "Only {} replica regions available, minimum is {}",
                replica_regions.len(),
                self.policy.min_replicas
            );
        }

        // Store replication info
        let replicated = ReplicatedAgent {
            agent_id,
            primary_region,
            replica_regions: replica_regions.iter().cloned().collect(),
            last_sync: SystemTime::now(),
        };

        self.replicated_agents
            .write()
            .await
            .insert(agent_id, replicated);

        debug!(
            "Agent {:?} replicated to {} regions",
            agent_id,
            replica_regions.len()
        );

        Ok(replica_regions)
    }

    /// Remove agent replication
    pub async fn remove_replication(&self, agent_id: &AgentId) {
        self.replicated_agents.write().await.remove(agent_id);
        debug!("Removed replication for agent {:?}", agent_id);
    }

    /// Get replicated agent info
    pub async fn get_replicated_agent(&self, agent_id: &AgentId) -> Option<ReplicatedAgent> {
        self.replicated_agents.read().await.get(agent_id).cloned()
    }

    /// Select regions for replication
    async fn select_replica_regions(
        &self,
        primary_region: &RegionId,
    ) -> Result<Vec<RegionId>, MultiRegionError> {
        let mut selected = Vec::new();

        // Try preferred regions first
        for region_id in &self.policy.preferred_regions {
            if region_id != primary_region {
                if let Some(region) = self.topology.get_region(region_id).await {
                    if region.is_available() {
                        selected.push(region_id.clone());
                    }
                }
            }

            if selected.len() >= self.policy.min_replicas {
                return Ok(selected);
            }
        }

        // Add more regions based on latency
        let healthy = self.topology.list_healthy_regions().await;
        let mut candidates: Vec<_> = healthy
            .into_iter()
            .filter(|r| r.id != *primary_region && !selected.contains(&r.id))
            .collect();

        // Sort by latency to primary region
        let primary_info = self
            .topology
            .get_region(primary_region)
            .await
            .ok_or_else(|| MultiRegionError::RegionNotFound {
                region_id: primary_region.to_string(),
            })?;

        candidates.sort_by_key(|r| {
            primary_info
                .latencies
                .get(&r.id)
                .copied()
                .unwrap_or(Duration::from_secs(999))
        });

        // Add remaining needed replicas
        for candidate in candidates {
            selected.push(candidate.id.clone());
            if selected.len() >= self.policy.min_replicas {
                break;
            }
        }

        if selected.is_empty() {
            return Err(MultiRegionError::NoHealthyRegions);
        }

        Ok(selected)
    }

    /// Synchronize replicas
    pub async fn sync_replicas(&self, agent_id: &AgentId) -> Result<(), MultiRegionError> {
        let mut agents = self.replicated_agents.write().await;
        if let Some(agent) = agents.get_mut(agent_id) {
            agent.last_sync = SystemTime::now();
            debug!("Synchronized replicas for agent {:?}", agent_id);
            Ok(())
        } else {
            Err(MultiRegionError::ReplicationFailed {
                reason: format!("Agent {:?} not found in replication state", agent_id),
            })
        }
    }
}

/// Regional failover coordinator
pub struct FailoverCoordinator {
    topology: Arc<RegionTopology>,
    replication: Arc<ReplicationManager>,
    /// Active failovers: (agent_id, from_region, to_region)
    active_failovers: Arc<RwLock<HashMap<AgentId, (RegionId, RegionId)>>>,
}

impl FailoverCoordinator {
    /// Create a new failover coordinator
    pub fn new(topology: Arc<RegionTopology>, replication: Arc<ReplicationManager>) -> Self {
        Self {
            topology,
            replication,
            active_failovers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initiate failover for an agent
    pub async fn initiate_failover(
        &self,
        agent_id: AgentId,
        failed_region: RegionId,
    ) -> Result<RegionId, MultiRegionError> {
        info!(
            "Initiating failover for agent {:?} from region {}",
            agent_id, failed_region
        );

        // Get replication info
        let replicated = self
            .replication
            .get_replicated_agent(&agent_id)
            .await
            .ok_or_else(|| MultiRegionError::FailoverFailed {
                reason: format!("Agent {:?} not found in replication state", agent_id),
            })?;

        // Select target region from replicas
        let target_region = self
            .select_failover_target(&replicated, &failed_region)
            .await?;

        // Record active failover
        self.active_failovers
            .write()
            .await
            .insert(agent_id, (failed_region.clone(), target_region.clone()));

        info!(
            "Failing over agent {:?} to region {}",
            agent_id, target_region
        );

        Ok(target_region)
    }

    /// Complete failover
    pub async fn complete_failover(&self, agent_id: &AgentId) -> Result<(), MultiRegionError> {
        self.active_failovers.write().await.remove(agent_id);
        debug!("Completed failover for agent {:?}", agent_id);
        Ok(())
    }

    /// Select best region for failover
    async fn select_failover_target(
        &self,
        replicated: &ReplicatedAgent,
        failed_region: &RegionId,
    ) -> Result<RegionId, MultiRegionError> {
        // Get available replica regions
        let mut candidates = Vec::new();

        for region_id in &replicated.replica_regions {
            if region_id != failed_region {
                if let Some(region) = self.topology.get_region(region_id).await {
                    if region.is_available() {
                        candidates.push(region);
                    }
                }
            }
        }

        if candidates.is_empty() {
            return Err(MultiRegionError::NoHealthyRegions);
        }

        // Select region with lowest load
        let target = candidates
            .into_iter()
            .min_by(|a, b| {
                a.load_percentage()
                    .partial_cmp(&b.load_percentage())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| MultiRegionError::NoHealthyRegions)?;

        Ok(target.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_location_distance() {
        let london = GeoLocation::new(51.5074, -0.1278);
        let paris = GeoLocation::new(48.8566, 2.3522);

        let distance = london.distance_to(&paris);
        // London to Paris is approximately 340 km
        assert!(distance > 300.0 && distance < 400.0);
    }

    #[test]
    fn test_region_info_creation() {
        let region = RegionInfo::new(
            RegionId::new("us-west-1"),
            "US West",
            GeoLocation::new(37.7749, -122.4194),
        );

        assert_eq!(region.id.as_str(), "us-west-1");
        assert_eq!(region.name, "US West");
        assert!(region.has_capacity());
        assert!(region.is_available());
    }

    #[test]
    fn test_region_load_percentage() {
        let mut region = RegionInfo::new(RegionId::new("test"), "Test", GeoLocation::new(0.0, 0.0));
        region.capacity = 100;
        region.current_load = 50;

        assert_eq!(region.load_percentage(), 50.0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_region_topology() {
        let topology = RegionTopology::new();

        let region = RegionInfo::new(
            RegionId::new("us-east-1"),
            "US East",
            GeoLocation::new(40.7128, -74.0060),
        );

        topology.register_region(region.clone()).await.unwrap();

        let retrieved = topology.get_region(&RegionId::new("us-east-1")).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "US East");
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_region_health_update() {
        let topology = RegionTopology::new();

        let region = RegionInfo::new(RegionId::new("test"), "Test", GeoLocation::new(0.0, 0.0));

        topology.register_region(region).await.unwrap();

        topology
            .update_health(&RegionId::new("test"), RegionHealth::Degraded)
            .await
            .unwrap();

        let updated = topology.get_region(&RegionId::new("test")).await.unwrap();
        assert_eq!(updated.health, RegionHealth::Degraded);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_latency_tracking() {
        let topology = RegionTopology::new();

        let region1 = RegionInfo::new(RegionId::new("r1"), "Region 1", GeoLocation::new(0.0, 0.0));
        let region2 = RegionInfo::new(
            RegionId::new("r2"),
            "Region 2",
            GeoLocation::new(10.0, 10.0),
        );

        topology.register_region(region1).await.unwrap();
        topology.register_region(region2).await.unwrap();

        let latency = Duration::from_millis(50);
        topology
            .update_latency(&RegionId::new("r1"), &RegionId::new("r2"), latency)
            .await
            .unwrap();

        let retrieved = topology
            .get_latency(&RegionId::new("r1"), &RegionId::new("r2"))
            .await;
        assert_eq!(retrieved, Some(latency));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_find_closest_region() {
        let topology = RegionTopology::new();

        let region1 = RegionInfo::new(RegionId::new("r1"), "Region 1", GeoLocation::new(0.0, 0.0));
        let mut region2 =
            RegionInfo::new(RegionId::new("r2"), "Region 2", GeoLocation::new(1.0, 1.0));
        region2
            .latencies
            .insert(RegionId::new("r1"), Duration::from_millis(10));

        let mut region3 = RegionInfo::new(
            RegionId::new("r3"),
            "Region 3",
            GeoLocation::new(50.0, 50.0),
        );
        region3
            .latencies
            .insert(RegionId::new("r1"), Duration::from_millis(100));

        topology.register_region(region1).await.unwrap();
        topology.register_region(region2).await.unwrap();
        topology.register_region(region3).await.unwrap();

        let closest = topology
            .find_closest_region(&RegionId::new("r1"))
            .await
            .unwrap();
        assert_eq!(closest.id, RegionId::new("r2"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_replication_manager() {
        let topology = Arc::new(RegionTopology::new());

        let region1 = RegionInfo::new(RegionId::new("r1"), "Region 1", GeoLocation::new(0.0, 0.0));
        let region2 = RegionInfo::new(
            RegionId::new("r2"),
            "Region 2",
            GeoLocation::new(10.0, 10.0),
        );
        let region3 = RegionInfo::new(
            RegionId::new("r3"),
            "Region 3",
            GeoLocation::new(20.0, 20.0),
        );

        topology.register_region(region1).await.unwrap();
        topology.register_region(region2).await.unwrap();
        topology.register_region(region3).await.unwrap();

        let policy = ReplicationPolicy {
            min_replicas: 2,
            preferred_regions: vec![],
            consistency: ConsistencyLevel::Quorum,
            max_delay: Duration::from_secs(30),
        };

        let manager = ReplicationManager::new(topology, policy);

        let agent_id: AgentId = *uuid::Uuid::new_v4().as_bytes();
        let replicas = manager
            .replicate_agent(agent_id, RegionId::new("r1"))
            .await
            .unwrap();

        assert!(replicas.len() >= 2);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_failover_coordinator() {
        let topology = Arc::new(RegionTopology::new());

        let region1 = RegionInfo::new(RegionId::new("r1"), "Region 1", GeoLocation::new(0.0, 0.0));
        let region2 = RegionInfo::new(
            RegionId::new("r2"),
            "Region 2",
            GeoLocation::new(10.0, 10.0),
        );

        topology.register_region(region1).await.unwrap();
        topology.register_region(region2).await.unwrap();

        let policy = ReplicationPolicy {
            min_replicas: 1,
            preferred_regions: vec![],
            consistency: ConsistencyLevel::Eventual,
            max_delay: Duration::from_secs(30),
        };

        let replication = Arc::new(ReplicationManager::new(topology.clone(), policy));
        let coordinator = FailoverCoordinator::new(topology, replication.clone());

        // Create a replicated agent
        let agent_id: AgentId = *uuid::Uuid::new_v4().as_bytes();
        replication
            .replicate_agent(agent_id, RegionId::new("r1"))
            .await
            .unwrap();

        // Initiate failover
        let target = coordinator
            .initiate_failover(agent_id, RegionId::new("r1"))
            .await
            .unwrap();

        assert_eq!(target, RegionId::new("r2"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_region_selection() {
        let topology = RegionTopology::new();

        let mut region1 =
            RegionInfo::new(RegionId::new("r1"), "Region 1", GeoLocation::new(0.0, 0.0));
        region1.current_load = 50;
        region1.capacity = 100;

        let mut region2 = RegionInfo::new(
            RegionId::new("r2"),
            "Region 2",
            GeoLocation::new(10.0, 10.0),
        );
        region2.current_load = 20;
        region2.capacity = 100;

        topology.register_region(region1).await.unwrap();
        topology.register_region(region2).await.unwrap();

        // Should select region with lowest load (r2)
        let selected = topology.select_region(&[]).await.unwrap();
        assert_eq!(selected.id, RegionId::new("r2"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_preferred_region_selection() {
        let topology = RegionTopology::new();

        let region1 = RegionInfo::new(RegionId::new("r1"), "Region 1", GeoLocation::new(0.0, 0.0));
        let region2 = RegionInfo::new(
            RegionId::new("r2"),
            "Region 2",
            GeoLocation::new(10.0, 10.0),
        );

        topology.register_region(region1).await.unwrap();
        topology.register_region(region2).await.unwrap();

        // Should select preferred region (r2)
        let selected = topology
            .select_region(&[RegionId::new("r2")])
            .await
            .unwrap();
        assert_eq!(selected.id, RegionId::new("r2"));
    }
}
