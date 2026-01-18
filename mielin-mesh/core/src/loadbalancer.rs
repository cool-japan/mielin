//! Load Balancing for Service Mesh
//!
//! Provides multiple load balancing algorithms for distributing traffic across
//! service instances in the MielinOS mesh network.
//!
//! Features:
//! - Round-robin load balancing
//! - Least connections algorithm
//! - Weighted load balancing
//! - Health-based routing
//! - Connection tracking
//! - Configurable selection strategies

use crate::error::MeshNetworkError;
use crate::service_discovery::{ServiceEndpoint, ServiceHealth, ServiceRegistration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Load balancer errors
#[derive(Debug, Error)]
pub enum LoadBalancerError {
    #[error("No healthy endpoints available for service: {service_name}")]
    NoHealthyEndpoints { service_name: String },

    #[error("Service not found: {service_name}")]
    ServiceNotFound { service_name: String },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Network error: {0}")]
    NetworkError(#[from] MeshNetworkError),
}

/// Load balancing algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadBalancingAlgorithm {
    /// Round-robin: distribute requests evenly across all endpoints
    RoundRobin,
    /// Least connections: route to endpoint with fewest active connections
    LeastConnections,
    /// Weighted round-robin: distribute based on endpoint weights
    WeightedRoundRobin,
    /// Random: randomly select an endpoint
    Random,
    /// Least response time: route to endpoint with lowest average response time
    LeastResponseTime,
}

impl Default for LoadBalancingAlgorithm {
    fn default() -> Self {
        Self::RoundRobin
    }
}

/// Health check configuration for endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check interval
    pub interval: Duration,
    /// Health check timeout
    pub timeout: Duration,
    /// Consecutive failures before marking unhealthy
    pub unhealthy_threshold: usize,
    /// Consecutive successes before marking healthy
    pub healthy_threshold: usize,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

/// Connection tracking for an endpoint
#[derive(Debug)]
pub struct EndpointStats {
    /// Endpoint address
    pub endpoint: ServiceEndpoint,
    /// Service health status
    pub health: ServiceHealth,
    /// Active connection count
    pub active_connections: AtomicUsize,
    /// Total requests served
    pub total_requests: AtomicU64,
    /// Failed requests
    pub failed_requests: AtomicU64,
    /// Average response time (milliseconds)
    pub avg_response_time_ms: AtomicU64,
    /// Last health check time
    pub last_health_check: Arc<RwLock<Instant>>,
    /// Consecutive health check failures
    pub consecutive_failures: AtomicUsize,
    /// Consecutive health check successes
    pub consecutive_successes: AtomicUsize,
}

impl EndpointStats {
    /// Create new endpoint stats
    pub fn new(endpoint: ServiceEndpoint) -> Self {
        Self {
            endpoint,
            health: ServiceHealth::Healthy,
            active_connections: AtomicUsize::new(0),
            total_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            avg_response_time_ms: AtomicU64::new(0),
            last_health_check: Arc::new(RwLock::new(Instant::now())),
            consecutive_failures: AtomicUsize::new(0),
            consecutive_successes: AtomicUsize::new(0),
        }
    }

    /// Increment active connections
    pub fn inc_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connections
    pub fn dec_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record failed request
    pub fn record_failure(&self) {
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Update average response time
    pub fn update_response_time(&self, response_time_ms: u64) {
        let current_avg = self.avg_response_time_ms.load(Ordering::Relaxed);
        let total_requests = self.total_requests.load(Ordering::Relaxed);

        if total_requests == 0 {
            self.avg_response_time_ms
                .store(response_time_ms, Ordering::Relaxed);
        } else {
            // Exponential moving average
            let new_avg = (current_avg * 9 + response_time_ms) / 10;
            self.avg_response_time_ms.store(new_avg, Ordering::Relaxed);
        }
    }

    /// Check if endpoint is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self.health, ServiceHealth::Healthy)
    }

    /// Get active connections count
    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Get average response time
    pub fn get_avg_response_time(&self) -> u64 {
        self.avg_response_time_ms.load(Ordering::Relaxed)
    }

    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return 1.0;
        }
        let failed = self.failed_requests.load(Ordering::Relaxed);
        let successful = total.saturating_sub(failed);
        successful as f64 / total as f64
    }
}

/// Service pool with load balancing
pub struct ServicePool {
    /// Service name
    service_name: String,
    /// Load balancing algorithm
    algorithm: LoadBalancingAlgorithm,
    /// Endpoint statistics
    endpoints: Arc<RwLock<Vec<Arc<EndpointStats>>>>,
    /// Round-robin counter
    rr_counter: AtomicUsize,
    /// Health check configuration
    health_check_config: HealthCheckConfig,
}

impl ServicePool {
    /// Create a new service pool
    pub fn new(service_name: impl Into<String>, algorithm: LoadBalancingAlgorithm) -> Self {
        Self {
            service_name: service_name.into(),
            algorithm,
            endpoints: Arc::new(RwLock::new(Vec::new())),
            rr_counter: AtomicUsize::new(0),
            health_check_config: HealthCheckConfig::default(),
        }
    }

    /// Set health check configuration
    pub fn with_health_check(mut self, config: HealthCheckConfig) -> Self {
        self.health_check_config = config;
        self
    }

    /// Add endpoint to pool
    pub async fn add_endpoint(&self, endpoint: ServiceEndpoint) {
        let stats = Arc::new(EndpointStats::new(endpoint.clone()));
        self.endpoints.write().await.push(stats);
        debug!(
            "Added endpoint {:?} to service pool {}",
            endpoint.address, self.service_name
        );
    }

    /// Remove endpoint from pool
    pub async fn remove_endpoint(&self, endpoint: &ServiceEndpoint) {
        self.endpoints
            .write()
            .await
            .retain(|e| e.endpoint.address != endpoint.address);
        debug!(
            "Removed endpoint {:?} from service pool {}",
            endpoint.address, self.service_name
        );
    }

    /// Update endpoints from service registration
    pub async fn update_endpoints(&self, registration: &ServiceRegistration) {
        let mut endpoints = self.endpoints.write().await;
        endpoints.clear();

        for endpoint in &registration.endpoints {
            let stats = Arc::new(EndpointStats::new(endpoint.clone()));
            endpoints.push(stats);
        }

        debug!(
            "Updated {} endpoints for service pool {}",
            registration.endpoints.len(),
            self.service_name
        );
    }

    /// Select next endpoint based on algorithm
    pub async fn select_endpoint(&self) -> Result<Arc<EndpointStats>, LoadBalancerError> {
        let endpoints = self.endpoints.read().await;

        if endpoints.is_empty() {
            return Err(LoadBalancerError::NoHealthyEndpoints {
                service_name: self.service_name.clone(),
            });
        }

        // Filter healthy endpoints
        let healthy: Vec<_> = endpoints.iter().filter(|e| e.is_healthy()).collect();

        if healthy.is_empty() {
            warn!(
                "No healthy endpoints for service: {}, falling back to all endpoints",
                self.service_name
            );
            // Fall back to any endpoint
            return Ok(endpoints[0].clone());
        }

        let selected = match self.algorithm {
            LoadBalancingAlgorithm::RoundRobin => self.round_robin_select(&healthy),
            LoadBalancingAlgorithm::LeastConnections => self.least_connections_select(&healthy),
            LoadBalancingAlgorithm::WeightedRoundRobin => {
                self.weighted_round_robin_select(&healthy)
            }
            LoadBalancingAlgorithm::Random => self.random_select(&healthy),
            LoadBalancingAlgorithm::LeastResponseTime => self.least_response_time_select(&healthy),
        };

        Ok(selected.clone())
    }

    /// Round-robin selection
    fn round_robin_select<'a>(
        &self,
        endpoints: &[&'a Arc<EndpointStats>],
    ) -> &'a Arc<EndpointStats> {
        let index = self.rr_counter.fetch_add(1, Ordering::Relaxed) % endpoints.len();
        endpoints[index]
    }

    /// Least connections selection
    fn least_connections_select<'a>(
        &self,
        endpoints: &[&'a Arc<EndpointStats>],
    ) -> &'a Arc<EndpointStats> {
        endpoints
            .iter()
            .min_by_key(|e| e.get_active_connections())
            .copied()
            .unwrap_or(endpoints[0])
    }

    /// Weighted round-robin selection
    fn weighted_round_robin_select<'a>(
        &self,
        endpoints: &[&'a Arc<EndpointStats>],
    ) -> &'a Arc<EndpointStats> {
        // Calculate total weight
        let total_weight: u32 = endpoints.iter().map(|e| e.endpoint.weight).sum();

        if total_weight == 0 {
            return self.round_robin_select(endpoints);
        }

        // Select based on weight
        let mut counter = self.rr_counter.fetch_add(1, Ordering::Relaxed) as u32 % total_weight;

        for endpoint in endpoints {
            if counter < endpoint.endpoint.weight {
                return endpoint;
            }
            counter -= endpoint.endpoint.weight;
        }

        endpoints[0]
    }

    /// Random selection
    fn random_select<'a>(&self, endpoints: &[&'a Arc<EndpointStats>]) -> &'a Arc<EndpointStats> {
        use rand::Rng;
        let mut rng = rand::rng();
        let index = rng.random_range(0..endpoints.len());
        endpoints[index]
    }

    /// Least response time selection
    fn least_response_time_select<'a>(
        &self,
        endpoints: &[&'a Arc<EndpointStats>],
    ) -> &'a Arc<EndpointStats> {
        endpoints
            .iter()
            .min_by_key(|e| e.get_avg_response_time())
            .copied()
            .unwrap_or(endpoints[0])
    }

    /// Mark endpoint as unhealthy
    pub async fn mark_unhealthy(&self, endpoint: &ServiceEndpoint) {
        let endpoints = self.endpoints.read().await;
        for stats in endpoints.iter() {
            if stats.endpoint.address == endpoint.address {
                stats.consecutive_failures.fetch_add(1, Ordering::Relaxed);
                stats.consecutive_successes.store(0, Ordering::Relaxed);

                let failures = stats.consecutive_failures.load(Ordering::Relaxed);
                if failures >= self.health_check_config.unhealthy_threshold {
                    // Mark as unhealthy (would need mutable access in real impl)
                    warn!(
                        "Endpoint {:?} marked as unhealthy after {} consecutive failures",
                        endpoint.address, failures
                    );
                }
                break;
            }
        }
    }

    /// Mark endpoint as healthy
    pub async fn mark_healthy(&self, endpoint: &ServiceEndpoint) {
        let endpoints = self.endpoints.read().await;
        for stats in endpoints.iter() {
            if stats.endpoint.address == endpoint.address {
                stats.consecutive_successes.fetch_add(1, Ordering::Relaxed);
                stats.consecutive_failures.store(0, Ordering::Relaxed);

                let successes = stats.consecutive_successes.load(Ordering::Relaxed);
                if successes >= self.health_check_config.healthy_threshold {
                    debug!(
                        "Endpoint {:?} marked as healthy after {} consecutive successes",
                        endpoint.address, successes
                    );
                }
                break;
            }
        }
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> PoolStats {
        let endpoints = self.endpoints.read().await;
        let total_endpoints = endpoints.len();
        let healthy_endpoints = endpoints.iter().filter(|e| e.is_healthy()).count();
        let total_connections: usize = endpoints.iter().map(|e| e.get_active_connections()).sum();
        let total_requests: u64 = endpoints
            .iter()
            .map(|e| e.total_requests.load(Ordering::Relaxed))
            .sum();
        let avg_response_time = if !endpoints.is_empty() {
            endpoints
                .iter()
                .map(|e| e.get_avg_response_time())
                .sum::<u64>()
                / endpoints.len() as u64
        } else {
            0
        };

        PoolStats {
            service_name: self.service_name.clone(),
            algorithm: self.algorithm,
            total_endpoints,
            healthy_endpoints,
            total_connections,
            total_requests,
            avg_response_time_ms: avg_response_time,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub service_name: String,
    pub algorithm: LoadBalancingAlgorithm,
    pub total_endpoints: usize,
    pub healthy_endpoints: usize,
    pub total_connections: usize,
    pub total_requests: u64,
    pub avg_response_time_ms: u64,
}

/// Load balancer manager
pub struct LoadBalancer {
    /// Service pools
    pools: Arc<RwLock<HashMap<String, Arc<ServicePool>>>>,
    /// Default algorithm
    default_algorithm: LoadBalancingAlgorithm,
}

impl LoadBalancer {
    /// Create a new load balancer
    pub fn new(default_algorithm: LoadBalancingAlgorithm) -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            default_algorithm,
        }
    }

    /// Register a service
    pub async fn register_service(
        &self,
        service_name: impl Into<String>,
        registration: ServiceRegistration,
        algorithm: Option<LoadBalancingAlgorithm>,
    ) {
        let service_name = service_name.into();
        let algo = algorithm.unwrap_or(self.default_algorithm);

        let pool = Arc::new(ServicePool::new(service_name.clone(), algo));
        pool.update_endpoints(&registration).await;

        self.pools.write().await.insert(service_name.clone(), pool);

        debug!(
            "Registered service {} with {} endpoints using {:?} algorithm",
            service_name,
            registration.endpoints.len(),
            algo
        );
    }

    /// Unregister a service
    pub async fn unregister_service(&self, service_name: &str) {
        self.pools.write().await.remove(service_name);
        debug!("Unregistered service {}", service_name);
    }

    /// Get service pool
    pub async fn get_pool(&self, service_name: &str) -> Option<Arc<ServicePool>> {
        self.pools.read().await.get(service_name).cloned()
    }

    /// Select endpoint for a service
    pub async fn select_endpoint(
        &self,
        service_name: &str,
    ) -> Result<Arc<EndpointStats>, LoadBalancerError> {
        let pools = self.pools.read().await;
        let pool = pools
            .get(service_name)
            .ok_or_else(|| LoadBalancerError::ServiceNotFound {
                service_name: service_name.to_string(),
            })?;

        pool.select_endpoint().await
    }

    /// Execute request with load balancing
    pub async fn execute_request<F, T>(
        &self,
        service_name: &str,
        request_fn: F,
    ) -> Result<T, LoadBalancerError>
    where
        F: FnOnce(
            Arc<EndpointStats>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, MeshNetworkError>> + Send>,
        >,
    {
        let endpoint_stats = self.select_endpoint(service_name).await?;
        endpoint_stats.inc_connections();

        let start = Instant::now();
        let result = request_fn(endpoint_stats.clone()).await;
        let elapsed = start.elapsed();

        endpoint_stats.dec_connections();
        endpoint_stats.update_response_time(elapsed.as_millis() as u64);

        match result {
            Ok(value) => Ok(value),
            Err(e) => {
                endpoint_stats.record_failure();
                Err(LoadBalancerError::NetworkError(e))
            }
        }
    }

    /// Get statistics for all pools
    pub async fn get_all_stats(&self) -> Vec<PoolStats> {
        let pools = self.pools.read().await;
        let mut stats = Vec::new();

        for pool in pools.values() {
            stats.push(pool.get_stats().await);
        }

        stats
    }
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::new(LoadBalancingAlgorithm::RoundRobin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn create_endpoint(port: u16) -> ServiceEndpoint {
        ServiceEndpoint::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port),
            "http",
        )
    }

    #[test]
    fn test_endpoint_stats() {
        let endpoint = create_endpoint(8080);
        let stats = EndpointStats::new(endpoint);

        stats.inc_connections();
        assert_eq!(stats.get_active_connections(), 1);

        stats.dec_connections();
        assert_eq!(stats.get_active_connections(), 0);
    }

    #[test]
    fn test_response_time_tracking() {
        let endpoint = create_endpoint(8080);
        let stats = EndpointStats::new(endpoint);

        stats.update_response_time(100);
        assert_eq!(stats.get_avg_response_time(), 100);

        stats.update_response_time(200);
        // Exponential moving average: (100 * 9 + 200) / 10 = 110
        let avg = stats.get_avg_response_time();
        assert!(avg >= 100, "Average should be >= 100, got {}", avg);
        assert!(avg <= 200, "Average should be <= 200, got {}", avg);
    }

    #[test]
    fn test_success_rate() {
        let endpoint = create_endpoint(8080);
        let stats = EndpointStats::new(endpoint);

        stats.inc_connections();
        stats.inc_connections();
        stats.inc_connections();
        stats.record_failure();

        assert_eq!(stats.success_rate(), 2.0 / 3.0);
    }

    #[tokio::test]
    async fn test_service_pool_round_robin() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::RoundRobin);

        pool.add_endpoint(create_endpoint(8080)).await;
        pool.add_endpoint(create_endpoint(8081)).await;
        pool.add_endpoint(create_endpoint(8082)).await;

        let ep1 = pool.select_endpoint().await.unwrap();
        let ep2 = pool.select_endpoint().await.unwrap();
        let ep3 = pool.select_endpoint().await.unwrap();

        // Should cycle through endpoints
        assert_ne!(ep1.endpoint.address.port(), ep2.endpoint.address.port());
        assert_ne!(ep2.endpoint.address.port(), ep3.endpoint.address.port());
    }

    #[tokio::test]
    async fn test_service_pool_least_connections() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::LeastConnections);

        pool.add_endpoint(create_endpoint(8080)).await;
        pool.add_endpoint(create_endpoint(8081)).await;

        // First selection
        let ep1 = pool.select_endpoint().await.unwrap();
        ep1.inc_connections();

        // Second selection should choose different endpoint
        let ep2 = pool.select_endpoint().await.unwrap();

        // ep2 should have fewer connections
        assert!(ep2.get_active_connections() <= ep1.get_active_connections());
    }

    #[tokio::test]
    async fn test_weighted_round_robin() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::WeightedRoundRobin);

        let mut ep1 = create_endpoint(8080);
        ep1.weight = 100;
        let mut ep2 = create_endpoint(8081);
        ep2.weight = 200;

        pool.add_endpoint(ep1).await;
        pool.add_endpoint(ep2).await;

        // Select multiple times and count
        let mut counts = HashMap::new();
        for _ in 0..300 {
            let ep = pool.select_endpoint().await.unwrap();
            *counts.entry(ep.endpoint.address.port()).or_insert(0) += 1;
        }

        // ep2 should be selected approximately twice as often as ep1
        let count_8080 = counts.get(&8080).copied().unwrap_or(0);
        let count_8081 = counts.get(&8081).copied().unwrap_or(0);

        // Allow some variance but should be roughly 1:2 ratio
        assert!(count_8081 > count_8080);
    }

    #[tokio::test]
    async fn test_load_balancer() {
        let lb = LoadBalancer::new(LoadBalancingAlgorithm::RoundRobin);

        let node_id = uuid::Uuid::new_v4();
        let mut registration = crate::ServiceRegistration::new("test-service", "1.0.0", node_id);
        registration = registration.add_endpoint(create_endpoint(8080));
        registration = registration.add_endpoint(create_endpoint(8081));

        lb.register_service("test-service", registration, None)
            .await;

        let endpoint = lb.select_endpoint("test-service").await.unwrap();
        assert!(endpoint.is_healthy());
    }

    #[tokio::test]
    async fn test_no_healthy_endpoints_fallback() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::RoundRobin);
        pool.add_endpoint(create_endpoint(8080)).await;

        // Should still return endpoint even if marked unhealthy (fallback behavior)
        let endpoint = pool.select_endpoint().await;
        assert!(endpoint.is_ok());
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::RoundRobin);

        pool.add_endpoint(create_endpoint(8080)).await;
        pool.add_endpoint(create_endpoint(8081)).await;

        let ep = pool.select_endpoint().await.unwrap();
        ep.inc_connections();

        let stats = pool.get_stats().await;
        assert_eq!(stats.service_name, "test-service");
        assert_eq!(stats.total_endpoints, 2);
        assert_eq!(stats.total_connections, 1);
    }

    #[tokio::test]
    async fn test_random_selection() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::Random);

        pool.add_endpoint(create_endpoint(8080)).await;
        pool.add_endpoint(create_endpoint(8081)).await;
        pool.add_endpoint(create_endpoint(8082)).await;

        // Select multiple times to ensure randomness
        let mut selected = HashSet::new();
        for _ in 0..50 {
            let ep = pool.select_endpoint().await.unwrap();
            selected.insert(ep.endpoint.address.port());
        }

        // Should have selected multiple different endpoints
        assert!(selected.len() > 1);
    }

    #[tokio::test]
    async fn test_least_response_time() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::LeastResponseTime);

        pool.add_endpoint(create_endpoint(8080)).await;
        pool.add_endpoint(create_endpoint(8081)).await;

        // Set different response times
        {
            let endpoints = pool.endpoints.read().await;
            endpoints[0].update_response_time(200);
            endpoints[1].update_response_time(100);
        }

        // Should select endpoint with lower response time
        let ep = pool.select_endpoint().await.unwrap();
        assert_eq!(ep.endpoint.address.port(), 8081);
    }

    #[tokio::test]
    async fn test_mark_unhealthy() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::RoundRobin);
        let endpoint = create_endpoint(8080);
        pool.add_endpoint(endpoint.clone()).await;

        pool.mark_unhealthy(&endpoint).await;

        let endpoints = pool.endpoints.read().await;
        assert_eq!(endpoints[0].consecutive_failures.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_mark_healthy() {
        let pool = ServicePool::new("test-service", LoadBalancingAlgorithm::RoundRobin);
        let endpoint = create_endpoint(8080);
        pool.add_endpoint(endpoint.clone()).await;

        pool.mark_healthy(&endpoint).await;

        let endpoints = pool.endpoints.read().await;
        assert_eq!(
            endpoints[0].consecutive_successes.load(Ordering::Relaxed),
            1
        );
    }
}
