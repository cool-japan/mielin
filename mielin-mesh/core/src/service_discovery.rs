//! Service Discovery for Service Mesh
//!
//! Provides service registration, discovery, and health tracking for microservices
//! running on the MielinOS mesh network.
//!
//! Features:
//! - Service registration with metadata (name, version, endpoints)
//! - Query services by name, tags, or health status
//! - Watch for service changes (additions, removals, health updates)
//! - Automatic deregistration on service failure
//! - Integration with mesh gossip for distributed service catalog

use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

/// Service discovery errors
#[derive(Debug, Error)]
pub enum ServiceDiscoveryError {
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Invalid service registration: {0}")]
    InvalidRegistration(String),

    #[error("Service already registered: {0}")]
    ServiceAlreadyRegistered(String),

    #[error("Query error: {0}")]
    QueryError(String),
}

/// Service health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ServiceHealth {
    /// Service is healthy and accepting requests
    #[default]
    Healthy,
    /// Service is degraded but still functional
    Degraded,
    /// Service is unhealthy and should not receive traffic
    Unhealthy,
    /// Service is in maintenance mode
    Maintenance,
}

/// Service endpoint information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ServiceEndpoint {
    /// Network address
    pub address: SocketAddr,
    /// Protocol (http, grpc, custom, etc.)
    pub protocol: String,
    /// Whether this endpoint supports TLS
    pub tls: bool,
    /// Endpoint weight for load balancing (higher = more traffic)
    pub weight: u32,
}

impl ServiceEndpoint {
    /// Create a new service endpoint
    pub fn new(address: SocketAddr, protocol: impl Into<String>) -> Self {
        Self {
            address,
            protocol: protocol.into(),
            tls: false,
            weight: 100,
        }
    }

    /// Enable TLS for this endpoint
    pub fn with_tls(mut self) -> Self {
        self.tls = true;
        self
    }

    /// Set endpoint weight
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }
}

/// Service metadata and registration information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    /// Unique service ID
    pub id: String,
    /// Service name (e.g., "user-service", "payment-api")
    pub name: String,
    /// Service version (semver)
    pub version: String,
    /// Node hosting this service
    pub node_id: NodeId,
    /// Service endpoints
    pub endpoints: Vec<ServiceEndpoint>,
    /// Service tags for filtering
    pub tags: HashSet<String>,
    /// Service metadata (key-value pairs)
    pub metadata: HashMap<String, String>,
    /// Current health status
    pub health: ServiceHealth,
    /// Registration timestamp
    pub registered_at: SystemTime,
    /// Last health check timestamp
    pub last_health_check: SystemTime,
    /// Time-to-live for this registration (auto-deregister after)
    pub ttl: Duration,
}

impl ServiceRegistration {
    /// Create a new service registration
    pub fn new(name: impl Into<String>, version: impl Into<String>, node_id: NodeId) -> Self {
        let name = name.into();
        let id = format!("{}:{}", name, uuid::Uuid::new_v4());

        Self {
            id,
            name,
            version: version.into(),
            node_id,
            endpoints: Vec::new(),
            tags: HashSet::new(),
            metadata: HashMap::new(),
            health: ServiceHealth::Healthy,
            registered_at: SystemTime::now(),
            last_health_check: SystemTime::now(),
            ttl: Duration::from_secs(60), // 1 minute default TTL
        }
    }

    /// Add an endpoint to this service
    pub fn add_endpoint(mut self, endpoint: ServiceEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// Add a tag to this service
    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Add metadata to this service
    pub fn add_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set health status
    pub fn with_health(mut self, health: ServiceHealth) -> Self {
        self.health = health;
        self
    }

    /// Set TTL
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Check if service registration has expired
    pub fn is_expired(&self) -> bool {
        self.last_health_check
            .elapsed()
            .map(|elapsed| elapsed > self.ttl)
            .unwrap_or(true)
    }

    /// Update last health check timestamp
    pub fn update_health_check(&mut self) {
        self.last_health_check = SystemTime::now();
    }
}

/// Service query criteria
#[derive(Debug, Clone, Default)]
pub struct ServiceQuery {
    /// Filter by service name (exact match)
    pub name: Option<String>,
    /// Filter by tags (service must have all specified tags)
    pub tags: HashSet<String>,
    /// Filter by health status
    pub health: Option<ServiceHealth>,
    /// Filter by node ID
    pub node_id: Option<NodeId>,
    /// Filter by metadata (service must have all specified key-value pairs)
    pub metadata: HashMap<String, String>,
}

impl ServiceQuery {
    /// Create a new service query
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by service name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Filter by tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Filter by health status
    pub fn with_health(mut self, health: ServiceHealth) -> Self {
        self.health = Some(health);
        self
    }

    /// Filter by node ID
    pub fn with_node(mut self, node_id: NodeId) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Check if a service matches this query
    pub fn matches(&self, service: &ServiceRegistration) -> bool {
        // Check name
        if let Some(ref name) = self.name {
            if &service.name != name {
                return false;
            }
        }

        // Check tags (service must have all query tags)
        if !self.tags.is_subset(&service.tags) {
            return false;
        }

        // Check health
        if let Some(health) = self.health {
            if service.health != health {
                return false;
            }
        }

        // Check node ID
        if let Some(node_id) = self.node_id {
            if service.node_id != node_id {
                return false;
            }
        }

        // Check metadata (service must have all query metadata)
        for (key, value) in &self.metadata {
            if service.metadata.get(key) != Some(value) {
                return false;
            }
        }

        true
    }
}

/// Service change event
#[derive(Debug, Clone)]
pub enum ServiceEvent {
    /// Service was registered
    Registered(Box<ServiceRegistration>),
    /// Service was deregistered
    Deregistered(String), // service ID
    /// Service health changed
    HealthChanged {
        service_id: String,
        old_health: ServiceHealth,
        new_health: ServiceHealth,
    },
    /// Service endpoints changed
    EndpointsChanged {
        service_id: String,
        endpoints: Vec<ServiceEndpoint>,
    },
}

/// Service discovery registry
pub struct ServiceDiscovery {
    /// Registered services (service_id -> registration)
    services: Arc<RwLock<HashMap<String, ServiceRegistration>>>,
    /// Service name index (name -> set of service IDs)
    name_index: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Tag index (tag -> set of service IDs)
    tag_index: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Node index (node_id -> set of service IDs)
    node_index: Arc<RwLock<HashMap<NodeId, HashSet<String>>>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<ServiceEvent>,
}

impl ServiceDiscovery {
    /// Create a new service discovery registry
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
            node_index: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Register a service
    pub async fn register(
        &self,
        service: ServiceRegistration,
    ) -> Result<(), ServiceDiscoveryError> {
        let service_id = service.id.clone();
        let service_name = service.name.clone();
        let service_tags = service.tags.clone();
        let node_id = service.node_id;

        // Validate service
        if service.endpoints.is_empty() {
            return Err(ServiceDiscoveryError::InvalidRegistration(
                "Service must have at least one endpoint".to_string(),
            ));
        }

        info!("Registering service: {} ({})", service_name, service_id);

        // Add to services map
        let mut services = self.services.write().await;
        if services.contains_key(&service_id) {
            return Err(ServiceDiscoveryError::ServiceAlreadyRegistered(service_id));
        }
        services.insert(service_id.clone(), service.clone());
        drop(services);

        // Update name index
        let mut name_index = self.name_index.write().await;
        name_index
            .entry(service_name.clone())
            .or_insert_with(HashSet::new)
            .insert(service_id.clone());
        drop(name_index);

        // Update tag index
        let mut tag_index = self.tag_index.write().await;
        for tag in &service_tags {
            tag_index
                .entry(tag.clone())
                .or_insert_with(HashSet::new)
                .insert(service_id.clone());
        }
        drop(tag_index);

        // Update node index
        let mut node_index = self.node_index.write().await;
        node_index
            .entry(node_id)
            .or_insert_with(HashSet::new)
            .insert(service_id.clone());
        drop(node_index);

        // Broadcast event
        let _ = self
            .event_tx
            .send(ServiceEvent::Registered(Box::new(service)));

        Ok(())
    }

    /// Deregister a service
    pub async fn deregister(&self, service_id: &str) -> Result<(), ServiceDiscoveryError> {
        info!("Deregistering service: {}", service_id);

        let mut services = self.services.write().await;
        let service = services
            .remove(service_id)
            .ok_or_else(|| ServiceDiscoveryError::ServiceNotFound(service_id.to_string()))?;
        drop(services);

        // Remove from name index
        let mut name_index = self.name_index.write().await;
        if let Some(ids) = name_index.get_mut(&service.name) {
            ids.remove(service_id);
            if ids.is_empty() {
                name_index.remove(&service.name);
            }
        }
        drop(name_index);

        // Remove from tag index
        let mut tag_index = self.tag_index.write().await;
        for tag in &service.tags {
            if let Some(ids) = tag_index.get_mut(tag) {
                ids.remove(service_id);
                if ids.is_empty() {
                    tag_index.remove(tag);
                }
            }
        }
        drop(tag_index);

        // Remove from node index
        let mut node_index = self.node_index.write().await;
        if let Some(ids) = node_index.get_mut(&service.node_id) {
            ids.remove(service_id);
            if ids.is_empty() {
                node_index.remove(&service.node_id);
            }
        }
        drop(node_index);

        // Broadcast event
        let _ = self
            .event_tx
            .send(ServiceEvent::Deregistered(service_id.to_string()));

        Ok(())
    }

    /// Query services
    pub async fn query(&self, query: &ServiceQuery) -> Vec<ServiceRegistration> {
        let services = self.services.read().await;

        services
            .values()
            .filter(|service| query.matches(service))
            .cloned()
            .collect()
    }

    /// Get a service by ID
    pub async fn get(&self, service_id: &str) -> Option<ServiceRegistration> {
        let services = self.services.read().await;
        services.get(service_id).cloned()
    }

    /// Get all services with a given name
    pub async fn get_by_name(&self, name: &str) -> Vec<ServiceRegistration> {
        let name_index = self.name_index.read().await;
        let service_ids = match name_index.get(name) {
            Some(ids) => ids.clone(),
            None => return Vec::new(),
        };
        drop(name_index);

        let services = self.services.read().await;
        service_ids
            .iter()
            .filter_map(|id| services.get(id).cloned())
            .collect()
    }

    /// Update service health
    pub async fn update_health(
        &self,
        service_id: &str,
        health: ServiceHealth,
    ) -> Result<(), ServiceDiscoveryError> {
        let mut services = self.services.write().await;
        let service = services
            .get_mut(service_id)
            .ok_or_else(|| ServiceDiscoveryError::ServiceNotFound(service_id.to_string()))?;

        let old_health = service.health;
        service.health = health;
        service.update_health_check();

        debug!(
            "Updated service {} health: {:?} -> {:?}",
            service_id, old_health, health
        );

        // Broadcast event if health changed
        if old_health != health {
            let _ = self.event_tx.send(ServiceEvent::HealthChanged {
                service_id: service_id.to_string(),
                old_health,
                new_health: health,
            });
        }

        Ok(())
    }

    /// Subscribe to service events
    pub fn subscribe(&self) -> broadcast::Receiver<ServiceEvent> {
        self.event_tx.subscribe()
    }

    /// Clean up expired services
    pub async fn cleanup_expired(&self) -> usize {
        let services = self.services.read().await;
        let expired: Vec<String> = services
            .values()
            .filter(|s| s.is_expired())
            .map(|s| s.id.clone())
            .collect();
        drop(services);

        let count = expired.len();
        for service_id in expired {
            warn!("Service {} expired, deregistering", service_id);
            let _ = self.deregister(&service_id).await;
        }

        count
    }

    /// Get service count
    pub async fn count(&self) -> usize {
        self.services.read().await.len()
    }

    /// Get all services
    pub async fn list_all(&self) -> Vec<ServiceRegistration> {
        self.services.read().await.values().cloned().collect()
    }
}

impl Default for ServiceDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_service_registration() {
        let discovery = ServiceDiscovery::new();
        let node_id = NodeId::new_v4();

        let service = ServiceRegistration::new("test-service", "1.0.0", node_id)
            .add_endpoint(ServiceEndpoint::new(
                "127.0.0.1:8080".parse().expect("parse failed"),
                "http",
            ))
            .add_tag("test")
            .add_metadata("region", "us-west");

        let result = discovery.register(service).await;
        assert!(result.is_ok());
        assert_eq!(discovery.count().await, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_service_deregistration() {
        let discovery = ServiceDiscovery::new();
        let node_id = NodeId::new_v4();

        let service = ServiceRegistration::new("test-service", "1.0.0", node_id).add_endpoint(
            ServiceEndpoint::new("127.0.0.1:8080".parse().expect("parse failed"), "http"),
        );

        let service_id = service.id.clone();
        discovery.register(service).await.expect("register failed");

        let result = discovery.deregister(&service_id).await;
        assert!(result.is_ok());
        assert_eq!(discovery.count().await, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_service_query_by_name() {
        let discovery = ServiceDiscovery::new();
        let node_id = NodeId::new_v4();

        let service = ServiceRegistration::new("test-service", "1.0.0", node_id).add_endpoint(
            ServiceEndpoint::new("127.0.0.1:8080".parse().expect("parse failed"), "http"),
        );

        discovery.register(service).await.expect("register failed");

        let query = ServiceQuery::new().with_name("test-service");
        let results = discovery.query(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test-service");
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_service_query_by_tags() {
        let discovery = ServiceDiscovery::new();
        let node_id = NodeId::new_v4();

        let service = ServiceRegistration::new("test-service", "1.0.0", node_id)
            .add_endpoint(ServiceEndpoint::new(
                "127.0.0.1:8080".parse().expect("parse failed"),
                "http",
            ))
            .add_tag("production")
            .add_tag("web");

        discovery.register(service).await.expect("register failed");

        let query = ServiceQuery::new().with_tag("production");
        let results = discovery.query(&query).await;
        assert_eq!(results.len(), 1);

        let query = ServiceQuery::new().with_tag("staging");
        let results = discovery.query(&query).await;
        assert_eq!(results.len(), 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_service_health_update() {
        let discovery = ServiceDiscovery::new();
        let node_id = NodeId::new_v4();

        let service = ServiceRegistration::new("test-service", "1.0.0", node_id).add_endpoint(
            ServiceEndpoint::new("127.0.0.1:8080".parse().expect("parse failed"), "http"),
        );

        let service_id = service.id.clone();
        discovery.register(service).await.expect("register failed");

        let result = discovery
            .update_health(&service_id, ServiceHealth::Unhealthy)
            .await;
        assert!(result.is_ok());

        let service = discovery.get(&service_id).await.expect("get failed");
        assert_eq!(service.health, ServiceHealth::Unhealthy);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_service_events() {
        let discovery = ServiceDiscovery::new();
        let mut rx = discovery.subscribe();
        let node_id = NodeId::new_v4();

        let service = ServiceRegistration::new("test-service", "1.0.0", node_id).add_endpoint(
            ServiceEndpoint::new("127.0.0.1:8080".parse().expect("parse failed"), "http"),
        );

        discovery.register(service).await.expect("register failed");

        let event = rx.recv().await.expect("recv failed");
        assert!(matches!(event, ServiceEvent::Registered(_)));
    }
}
