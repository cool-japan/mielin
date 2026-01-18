//! Agent Discovery Service
//!
//! Provides service registry, capability-based discovery, location-aware routing,
//! and health checking for agents in a distributed system.

use crate::{AgentId, Version};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Discovery-related errors
#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("Agent not found: {0}")]
    AgentNotFound(AgentId),
    #[error("Service not found: {0}")]
    ServiceNotFound(String),
    #[error("No healthy agents available for service: {0}")]
    NoHealthyAgents(String),
    #[error("Registration failed: {0}")]
    RegistrationFailed(String),
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
}

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    pub version: Version,
    #[serde(skip)]
    pub parameters: HashMap<String, String>,
}

impl Hash for Capability {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.version.hash(state);
        // Intentionally skip parameters for hashing
    }
}

impl Capability {
    pub fn new(name: impl Into<String>, version: Version) -> Self {
        Self {
            name: name.into(),
            version,
            parameters: HashMap::new(),
        }
    }

    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }
}

/// Geographic location
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Calculate distance to another location in kilometers using Haversine formula
    pub fn distance_to(&self, other: &Location) -> f64 {
        const EARTH_RADIUS_KM: f64 = 6371.0;

        let lat1_rad = self.latitude.to_radians();
        let lat2_rad = other.latitude.to_radians();
        let delta_lat = (other.latitude - self.latitude).to_radians();
        let delta_lon = (other.longitude - self.longitude).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        EARTH_RADIUS_KM * c
    }
}

/// Health status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: HealthStatus,
    #[serde(skip, default = "Instant::now")]
    pub last_check: Instant,
    pub message: Option<String>,
    pub metrics: HashMap<String, f64>,
}

impl HealthCheck {
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            last_check: Instant::now(),
            message: None,
            metrics: HashMap::new(),
        }
    }

    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            last_check: Instant::now(),
            message: Some(message.into()),
            metrics: HashMap::new(),
        }
    }

    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            last_check: Instant::now(),
            message: Some(message.into()),
            metrics: HashMap::new(),
        }
    }

    pub fn with_metric(mut self, key: impl Into<String>, value: f64) -> Self {
        self.metrics.insert(key.into(), value);
        self
    }

    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.last_check.elapsed() > max_age
    }
}

/// Service registration information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    pub agent_id: AgentId,
    pub service_name: String,
    pub version: Version,
    pub capabilities: HashSet<Capability>,
    pub address: SocketAddr,
    pub location: Option<Location>,
    pub metadata: HashMap<String, String>,
    #[serde(skip, default = "Instant::now")]
    pub registered_at: Instant,
}

impl ServiceRegistration {
    pub fn new(
        agent_id: AgentId,
        service_name: impl Into<String>,
        version: Version,
        address: SocketAddr,
    ) -> Self {
        Self {
            agent_id,
            service_name: service_name.into(),
            version,
            capabilities: HashSet::new(),
            address,
            location: None,
            metadata: HashMap::new(),
            registered_at: Instant::now(),
        }
    }

    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.capabilities.insert(capability);
        self
    }

    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Discovery query for finding agents
#[derive(Debug, Clone, Default)]
pub struct DiscoveryQuery {
    pub service_name: Option<String>,
    pub capabilities: Vec<Capability>,
    pub version: Option<Version>,
    pub location: Option<Location>,
    pub max_distance_km: Option<f64>,
    pub required_health: Option<HealthStatus>,
    pub max_results: Option<usize>,
}

impl DiscoveryQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn service(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    pub fn capability(mut self, capability: Capability) -> Self {
        self.capabilities.push(capability);
        self
    }

    pub fn version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    pub fn near(mut self, location: Location, max_distance_km: f64) -> Self {
        self.location = Some(location);
        self.max_distance_km = Some(max_distance_km);
        self
    }

    pub fn healthy(mut self) -> Self {
        self.required_health = Some(HealthStatus::Healthy);
        self
    }

    pub fn limit(mut self, max: usize) -> Self {
        self.max_results = Some(max);
        self
    }
}

/// Service discovery result
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub registration: ServiceRegistration,
    pub health: HealthCheck,
    pub distance_km: Option<f64>,
    pub score: f64, // Higher is better
}

impl DiscoveryResult {
    pub fn is_healthy(&self) -> bool {
        matches!(self.health.status, HealthStatus::Healthy)
    }
}

/// Service registry for agent discovery
pub struct ServiceRegistry {
    registrations: Arc<RwLock<HashMap<AgentId, ServiceRegistration>>>,
    health_checks: Arc<RwLock<HashMap<AgentId, HealthCheck>>>,
    service_index: Arc<RwLock<HashMap<String, HashSet<AgentId>>>>,
    capability_index: Arc<RwLock<HashMap<String, HashSet<AgentId>>>>,
    health_check_interval: Duration,
    max_health_age: Duration,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            registrations: Arc::new(RwLock::new(HashMap::new())),
            health_checks: Arc::new(RwLock::new(HashMap::new())),
            service_index: Arc::new(RwLock::new(HashMap::new())),
            capability_index: Arc::new(RwLock::new(HashMap::new())),
            health_check_interval: Duration::from_secs(30),
            max_health_age: Duration::from_secs(60),
        }
    }

    pub fn with_health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    pub fn with_max_health_age(mut self, max_age: Duration) -> Self {
        self.max_health_age = max_age;
        self
    }

    /// Register a service
    pub fn register(&self, registration: ServiceRegistration) -> Result<(), DiscoveryError> {
        let agent_id = registration.agent_id;
        let service_name = registration.service_name.clone();

        // Update registrations
        {
            let mut registrations = self
                .registrations
                .write()
                .expect("Lock poisoned: registrations");
            registrations.insert(agent_id, registration.clone());
        }

        // Update service index
        {
            let mut service_index = self
                .service_index
                .write()
                .expect("Lock poisoned: service_index");
            service_index
                .entry(service_name)
                .or_default()
                .insert(agent_id);
        }

        // Update capability index
        {
            let mut capability_index = self
                .capability_index
                .write()
                .expect("Lock poisoned: capability_index");
            for capability in &registration.capabilities {
                capability_index
                    .entry(capability.name.clone())
                    .or_default()
                    .insert(agent_id);
            }
        }

        // Initialize health check
        {
            let mut health_checks = self
                .health_checks
                .write()
                .expect("Lock poisoned: health_checks");
            health_checks.insert(agent_id, HealthCheck::healthy());
        }

        Ok(())
    }

    /// Deregister a service
    pub fn deregister(&self, agent_id: &AgentId) -> Result<(), DiscoveryError> {
        let registration = {
            let mut registrations = self
                .registrations
                .write()
                .expect("Lock poisoned: registrations");
            registrations
                .remove(agent_id)
                .ok_or(DiscoveryError::AgentNotFound(*agent_id))?
        };

        // Remove from service index
        {
            let mut service_index = self
                .service_index
                .write()
                .expect("Lock poisoned: service_index");
            if let Some(agents) = service_index.get_mut(&registration.service_name) {
                agents.remove(agent_id);
                if agents.is_empty() {
                    service_index.remove(&registration.service_name);
                }
            }
        }

        // Remove from capability index
        {
            let mut capability_index = self
                .capability_index
                .write()
                .expect("Lock poisoned: capability_index");
            for capability in &registration.capabilities {
                if let Some(agents) = capability_index.get_mut(&capability.name) {
                    agents.remove(agent_id);
                    if agents.is_empty() {
                        capability_index.remove(&capability.name);
                    }
                }
            }
        }

        // Remove health check
        {
            let mut health_checks = self
                .health_checks
                .write()
                .expect("Lock poisoned: health_checks");
            health_checks.remove(agent_id);
        }

        Ok(())
    }

    /// Update health check for an agent
    pub fn update_health(
        &self,
        agent_id: &AgentId,
        health: HealthCheck,
    ) -> Result<(), DiscoveryError> {
        let mut health_checks = self
            .health_checks
            .write()
            .expect("Lock poisoned: health_checks");
        if health_checks.contains_key(agent_id) {
            health_checks.insert(*agent_id, health);
            Ok(())
        } else {
            Err(DiscoveryError::AgentNotFound(*agent_id))
        }
    }

    /// Get health check for an agent
    pub fn get_health(&self, agent_id: &AgentId) -> Result<HealthCheck, DiscoveryError> {
        let health_checks = self
            .health_checks
            .read()
            .expect("Lock poisoned: health_checks");
        health_checks
            .get(agent_id)
            .cloned()
            .ok_or(DiscoveryError::AgentNotFound(*agent_id))
    }

    /// Discover agents matching a query
    pub fn discover(&self, query: DiscoveryQuery) -> Vec<DiscoveryResult> {
        let registrations = self
            .registrations
            .read()
            .expect("Lock poisoned: registrations");
        let health_checks = self
            .health_checks
            .read()
            .expect("Lock poisoned: health_checks");

        let mut results = Vec::new();

        for (agent_id, registration) in registrations.iter() {
            // Filter by service name
            if let Some(ref service_name) = query.service_name {
                if &registration.service_name != service_name {
                    continue;
                }
            }

            // Filter by version
            if let Some(ref version) = query.version {
                if &registration.version != version {
                    continue;
                }
            }

            // Filter by capabilities
            if !query.capabilities.is_empty() {
                let has_all_capabilities = query.capabilities.iter().all(|cap| {
                    registration
                        .capabilities
                        .iter()
                        .any(|reg_cap| reg_cap.name == cap.name && reg_cap.version >= cap.version)
                });

                if !has_all_capabilities {
                    continue;
                }
            }

            // Calculate distance if location is specified
            let distance_km = if let (Some(query_loc), Some(reg_loc)) =
                (&query.location, &registration.location)
            {
                let dist = query_loc.distance_to(reg_loc);
                if let Some(max_dist) = query.max_distance_km {
                    if dist > max_dist {
                        continue;
                    }
                }
                Some(dist)
            } else {
                None
            };

            // Get health check
            let health = health_checks
                .get(agent_id)
                .cloned()
                .unwrap_or_else(|| HealthCheck {
                    status: HealthStatus::Unknown,
                    last_check: Instant::now(),
                    message: None,
                    metrics: HashMap::new(),
                });

            // Filter by health status
            if let Some(required_health) = query.required_health {
                if health.status != required_health {
                    continue;
                }
            }

            // Skip stale health checks
            if health.is_stale(self.max_health_age) && health.status != HealthStatus::Unknown {
                continue;
            }

            // Calculate score (lower distance is better)
            let distance_score = distance_km.map(|d| 1.0 / (1.0 + d)).unwrap_or(1.0);
            let health_score = match health.status {
                HealthStatus::Healthy => 1.0,
                HealthStatus::Degraded => 0.5,
                HealthStatus::Unhealthy => 0.1,
                HealthStatus::Unknown => 0.3,
            };
            let score = distance_score * health_score;

            results.push(DiscoveryResult {
                registration: registration.clone(),
                health,
                distance_km,
                score,
            });
        }

        // Sort by score (highest first)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results
        if let Some(max_results) = query.max_results {
            results.truncate(max_results);
        }

        results
    }

    /// Get all services by name
    pub fn get_service(&self, service_name: &str) -> Vec<AgentId> {
        let service_index = self
            .service_index
            .read()
            .expect("Lock poisoned: service_index");
        service_index
            .get(service_name)
            .map(|agents| agents.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get all agents with a capability
    pub fn get_by_capability(&self, capability_name: &str) -> Vec<AgentId> {
        let capability_index = self
            .capability_index
            .read()
            .expect("Lock poisoned: capability_index");
        capability_index
            .get(capability_name)
            .map(|agents| agents.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get all registered services
    pub fn list_services(&self) -> Vec<String> {
        let service_index = self
            .service_index
            .read()
            .expect("Lock poisoned: service_index");
        service_index.keys().cloned().collect()
    }

    /// Get all registered agents
    pub fn list_agents(&self) -> Vec<AgentId> {
        let registrations = self
            .registrations
            .read()
            .expect("Lock poisoned: registrations");
        registrations.keys().copied().collect()
    }

    /// Get registration for an agent
    pub fn get_registration(
        &self,
        agent_id: &AgentId,
    ) -> Result<ServiceRegistration, DiscoveryError> {
        let registrations = self
            .registrations
            .read()
            .expect("Lock poisoned: registrations");
        registrations
            .get(agent_id)
            .cloned()
            .ok_or(DiscoveryError::AgentNotFound(*agent_id))
    }

    /// Get healthy agent count for a service
    pub fn healthy_count(&self, service_name: &str) -> usize {
        let service_index = self
            .service_index
            .read()
            .expect("Lock poisoned: service_index");
        let health_checks = self
            .health_checks
            .read()
            .expect("Lock poisoned: health_checks");

        service_index
            .get(service_name)
            .map(|agents| {
                agents
                    .iter()
                    .filter(|agent_id| {
                        health_checks
                            .get(agent_id)
                            .map(|h| matches!(h.status, HealthStatus::Healthy))
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Load balancer for routing requests to agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    Random,
    LeastConnections,
    LocationBased,
}

pub struct LoadBalancer {
    registry: Arc<ServiceRegistry>,
    strategy: LoadBalancingStrategy,
    current_index: Arc<RwLock<HashMap<String, usize>>>,
    connection_counts: Arc<RwLock<HashMap<AgentId, usize>>>,
}

impl LoadBalancer {
    pub fn new(registry: Arc<ServiceRegistry>, strategy: LoadBalancingStrategy) -> Self {
        Self {
            registry,
            strategy,
            current_index: Arc::new(RwLock::new(HashMap::new())),
            connection_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Select an agent for a service
    pub fn select(
        &self,
        service_name: &str,
        location: Option<Location>,
    ) -> Result<AgentId, DiscoveryError> {
        let mut query = DiscoveryQuery::new().service(service_name).healthy();

        if let Some(loc) = location {
            query = query.near(loc, f64::MAX);
        }

        let results = self.registry.discover(query);

        if results.is_empty() {
            return Err(DiscoveryError::NoHealthyAgents(service_name.to_string()));
        }

        let agent_id = match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let mut current_index = self
                    .current_index
                    .write()
                    .expect("Lock poisoned: current_index");
                let index = current_index.entry(service_name.to_string()).or_insert(0);
                let selected = results[*index % results.len()].registration.agent_id;
                *index = (*index + 1) % results.len();
                selected
            }
            LoadBalancingStrategy::Random => {
                use rand::Rng;
                let mut rng = rand::rng();
                let index = rng.random_range(0..results.len());
                results[index].registration.agent_id
            }
            LoadBalancingStrategy::LeastConnections => {
                let connection_counts = self
                    .connection_counts
                    .read()
                    .expect("Lock poisoned: connection_counts");
                results
                    .iter()
                    .min_by_key(|r| {
                        connection_counts
                            .get(&r.registration.agent_id)
                            .copied()
                            .unwrap_or(0)
                    })
                    .map(|r| r.registration.agent_id)
                    .expect("Results is not empty")
            }
            LoadBalancingStrategy::LocationBased => {
                // Already sorted by score which includes distance
                results[0].registration.agent_id
            }
        };

        // Increment connection count
        if matches!(self.strategy, LoadBalancingStrategy::LeastConnections) {
            let mut connection_counts = self
                .connection_counts
                .write()
                .expect("Lock poisoned: connection_counts");
            *connection_counts.entry(agent_id).or_insert(0) += 1;
        }

        Ok(agent_id)
    }

    /// Release a connection (for LeastConnections strategy)
    pub fn release(&self, agent_id: &AgentId) {
        let mut connection_counts = self
            .connection_counts
            .write()
            .expect("Lock poisoned: connection_counts");
        if let Some(count) = connection_counts.get_mut(agent_id) {
            *count = count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_location_distance() {
        let tokyo = Location::new(35.6762, 139.6503);
        let osaka = Location::new(34.6937, 135.5023);

        let distance = tokyo.distance_to(&osaka);
        // Distance between Tokyo and Osaka is approximately 400 km
        assert!(distance > 390.0 && distance < 410.0);
    }

    #[test]
    fn test_service_registration() {
        let registry = ServiceRegistry::new();
        let agent_id = AgentId::new_v4();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let registration =
            ServiceRegistration::new(agent_id, "test-service", Version::new(1, 0, 0), addr);

        registry.register(registration).unwrap();

        let agents = registry.get_service("test-service");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0], agent_id);
    }

    #[test]
    fn test_capability_discovery() {
        let registry = ServiceRegistry::new();
        let agent_id = AgentId::new_v4();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let capability = Capability::new("image-processing", Version::new(1, 0, 0));
        let registration =
            ServiceRegistration::new(agent_id, "media-service", Version::new(1, 0, 0), addr)
                .with_capability(capability.clone());

        registry.register(registration).unwrap();

        let query = DiscoveryQuery::new().capability(capability);
        let results = registry.discover(query);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].registration.agent_id, agent_id);
    }

    #[test]
    fn test_location_based_discovery() {
        let registry = ServiceRegistry::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        // Register agent in Tokyo
        let agent1 = AgentId::new_v4();
        let tokyo = Location::new(35.6762, 139.6503);
        let reg1 = ServiceRegistration::new(agent1, "geo-service", Version::new(1, 0, 0), addr)
            .with_location(tokyo);
        registry.register(reg1).unwrap();

        // Register agent in Osaka
        let agent2 = AgentId::new_v4();
        let osaka = Location::new(34.6937, 135.5023);
        let reg2 = ServiceRegistration::new(agent2, "geo-service", Version::new(1, 0, 0), addr)
            .with_location(osaka);
        registry.register(reg2).unwrap();

        // Query from near Tokyo (within 100km)
        let query_location = Location::new(35.7, 139.7);
        let query = DiscoveryQuery::new()
            .service("geo-service")
            .near(query_location, 100.0);

        let results = registry.discover(query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].registration.agent_id, agent1);
    }

    #[test]
    fn test_health_check() {
        let registry = ServiceRegistry::new();
        let agent_id = AgentId::new_v4();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let registration =
            ServiceRegistration::new(agent_id, "health-test", Version::new(1, 0, 0), addr);
        registry.register(registration).unwrap();

        // Initially healthy
        let health = registry.get_health(&agent_id).unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);

        // Update to unhealthy
        registry
            .update_health(&agent_id, HealthCheck::unhealthy("Service error"))
            .unwrap();

        let health = registry.get_health(&agent_id).unwrap();
        assert_eq!(health.status, HealthStatus::Unhealthy);

        // Query should not return unhealthy agents
        let query = DiscoveryQuery::new().service("health-test").healthy();
        let results = registry.discover(query);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_deregistration() {
        let registry = ServiceRegistry::new();
        let agent_id = AgentId::new_v4();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let registration =
            ServiceRegistration::new(agent_id, "temp-service", Version::new(1, 0, 0), addr);
        registry.register(registration).unwrap();

        assert_eq!(registry.get_service("temp-service").len(), 1);

        registry.deregister(&agent_id).unwrap();
        assert_eq!(registry.get_service("temp-service").len(), 0);
    }

    #[test]
    fn test_round_robin_load_balancer() {
        let registry = Arc::new(ServiceRegistry::new());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let agent1 = AgentId::new_v4();
        let agent2 = AgentId::new_v4();
        let agent3 = AgentId::new_v4();

        registry
            .register(ServiceRegistration::new(
                agent1,
                "lb-test",
                Version::new(1, 0, 0),
                addr,
            ))
            .unwrap();
        registry
            .register(ServiceRegistration::new(
                agent2,
                "lb-test",
                Version::new(1, 0, 0),
                addr,
            ))
            .unwrap();
        registry
            .register(ServiceRegistration::new(
                agent3,
                "lb-test",
                Version::new(1, 0, 0),
                addr,
            ))
            .unwrap();

        let lb = LoadBalancer::new(registry.clone(), LoadBalancingStrategy::RoundRobin);

        let mut selected = HashSet::new();
        for _ in 0..6 {
            let agent = lb.select("lb-test", None).unwrap();
            selected.insert(agent);
        }

        // Should have selected all 3 agents
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_random_load_balancer() {
        let registry = Arc::new(ServiceRegistry::new());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        for _ in 0..10 {
            let agent = AgentId::new_v4();
            registry
                .register(ServiceRegistration::new(
                    agent,
                    "random-test",
                    Version::new(1, 0, 0),
                    addr,
                ))
                .unwrap();
        }

        let lb = LoadBalancer::new(registry.clone(), LoadBalancingStrategy::Random);

        let mut selected = HashSet::new();
        for _ in 0..20 {
            let agent = lb.select("random-test", None).unwrap();
            selected.insert(agent);
        }

        // Should have selected multiple different agents
        assert!(selected.len() > 1);
    }

    #[test]
    fn test_least_connections_load_balancer() {
        let registry = Arc::new(ServiceRegistry::new());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let agent1 = AgentId::new_v4();
        let agent2 = AgentId::new_v4();

        registry
            .register(ServiceRegistration::new(
                agent1,
                "lc-test",
                Version::new(1, 0, 0),
                addr,
            ))
            .unwrap();
        registry
            .register(ServiceRegistration::new(
                agent2,
                "lc-test",
                Version::new(1, 0, 0),
                addr,
            ))
            .unwrap();

        let lb = LoadBalancer::new(registry.clone(), LoadBalancingStrategy::LeastConnections);

        // First selection
        let first = lb.select("lc-test", None).unwrap();
        // Second selection should pick the other agent
        let second = lb.select("lc-test", None).unwrap();

        assert_ne!(first, second);

        // Release first connection
        lb.release(&first);

        // Next selection should prefer the first agent again
        let third = lb.select("lc-test", None).unwrap();
        assert_eq!(third, first);
    }
}
