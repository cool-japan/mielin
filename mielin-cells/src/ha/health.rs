//! Health Monitoring Module
//!
//! Implements health monitoring for high availability clusters.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

pub type NodeId = String;

#[derive(Debug, Error)]
pub enum HealthError {
    #[error("Health check failed: {0}")]
    CheckFailed(String),
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Unhealthy node: {0}")]
    UnhealthyNode(String),
}

pub type HealthResult<T> = Result<T, HealthError>;

/// Health status of a node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Node is healthy
    Healthy,
    /// Node is degraded (partial functionality)
    Degraded,
    /// Node is unhealthy
    Unhealthy,
    /// Node status is unknown
    Unknown,
}

/// Health metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetric {
    /// Metric name
    pub name: String,
    /// Metric value
    pub value: f64,
    /// Threshold
    pub threshold: f64,
    /// Whether metric is within threshold
    pub healthy: bool,
}

/// Health threshold configuration
#[derive(Debug, Clone)]
pub struct HealthThreshold {
    /// CPU threshold (0.0 to 1.0)
    pub cpu_threshold: f64,
    /// Memory threshold (0.0 to 1.0)
    pub memory_threshold: f64,
    /// Disk threshold (0.0 to 1.0)
    pub disk_threshold: f64,
    /// Network latency threshold (ms)
    pub latency_threshold: Duration,
    /// Error rate threshold (0.0 to 1.0)
    pub error_rate_threshold: f64,
}

impl Default for HealthThreshold {
    fn default() -> Self {
        Self {
            cpu_threshold: 0.8,
            memory_threshold: 0.85,
            disk_threshold: 0.9,
            latency_threshold: Duration::from_millis(100),
            error_rate_threshold: 0.05,
        }
    }
}

/// Health configuration
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Health check interval
    pub check_interval: Duration,
    /// Health thresholds
    pub thresholds: HealthThreshold,
    /// Consecutive failures before marking unhealthy
    pub failure_threshold: usize,
    /// Consecutive successes before marking healthy
    pub success_threshold: usize,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(10),
            thresholds: HealthThreshold::default(),
            failure_threshold: 3,
            success_threshold: 2,
        }
    }
}

/// Node health information
#[derive(Debug, Clone)]
pub struct NodeHealth {
    /// Node ID
    pub node_id: NodeId,
    /// Health status
    pub status: HealthStatus,
    /// Health metrics
    pub metrics: Vec<HealthMetric>,
    /// Last check time
    pub last_check: Instant,
    /// Consecutive failures
    pub consecutive_failures: usize,
    /// Consecutive successes
    pub consecutive_successes: usize,
}

/// Cluster health information
#[derive(Debug, Clone)]
pub struct ClusterHealth {
    /// Total nodes
    pub total_nodes: usize,
    /// Healthy nodes
    pub healthy_nodes: usize,
    /// Degraded nodes
    pub degraded_nodes: usize,
    /// Unhealthy nodes
    pub unhealthy_nodes: usize,
    /// Unknown nodes
    pub unknown_nodes: usize,
    /// Overall cluster status
    pub status: HealthStatus,
    /// Last updated
    pub last_updated: Instant,
}

/// Cluster health monitor
pub struct ClusterHealthMonitor {
    /// Configuration
    config: HealthConfig,
    /// Node health status
    nodes: Arc<RwLock<HashMap<NodeId, NodeHealth>>>,
    /// Last cluster health
    cluster_health: Arc<RwLock<ClusterHealth>>,
}

impl ClusterHealthMonitor {
    /// Create a new cluster health monitor
    pub fn new(config: HealthConfig) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            cluster_health: Arc::new(RwLock::new(ClusterHealth {
                total_nodes: 0,
                healthy_nodes: 0,
                degraded_nodes: 0,
                unhealthy_nodes: 0,
                unknown_nodes: 0,
                status: HealthStatus::Unknown,
                last_updated: Instant::now(),
            })),
        }
    }

    /// Register a node for health monitoring
    pub fn register_node(&self, node_id: NodeId) -> HealthResult<()> {
        let mut nodes = self
            .nodes
            .write()
            .map_err(|_| HealthError::NodeNotFound("Failed to acquire lock".to_string()))?;

        nodes.insert(
            node_id.clone(),
            NodeHealth {
                node_id,
                status: HealthStatus::Unknown,
                metrics: Vec::new(),
                last_check: Instant::now(),
                consecutive_failures: 0,
                consecutive_successes: 0,
            },
        );

        drop(nodes); // Release write lock before calling update_cluster_health

        self.update_cluster_health()?;
        Ok(())
    }

    /// Unregister a node
    pub fn unregister_node(&self, node_id: &str) -> HealthResult<()> {
        let mut nodes = self
            .nodes
            .write()
            .map_err(|_| HealthError::NodeNotFound("Failed to acquire lock".to_string()))?;

        nodes.remove(node_id);
        drop(nodes); // Release write lock before calling update_cluster_health

        self.update_cluster_health()?;
        Ok(())
    }

    /// Perform health check on a node
    pub fn check_node_health(
        &self,
        node_id: &str,
        metrics: Vec<HealthMetric>,
    ) -> HealthResult<NodeHealth> {
        let mut nodes = self
            .nodes
            .write()
            .map_err(|_| HealthError::NodeNotFound("Failed to acquire lock".to_string()))?;

        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| HealthError::NodeNotFound(node_id.to_string()))?;

        // Determine health status based on metrics
        let all_healthy = metrics.iter().all(|m| m.healthy);
        let any_unhealthy = metrics.iter().any(|m| !m.healthy);

        if all_healthy {
            node.consecutive_successes += 1;
            node.consecutive_failures = 0;

            if node.consecutive_successes >= self.config.success_threshold {
                node.status = HealthStatus::Healthy;
            }
        } else if any_unhealthy {
            node.consecutive_failures += 1;
            node.consecutive_successes = 0;

            if node.consecutive_failures >= self.config.failure_threshold {
                node.status = HealthStatus::Unhealthy;
            } else {
                node.status = HealthStatus::Degraded;
            }
        } else {
            node.status = HealthStatus::Unknown;
        }

        node.metrics = metrics;
        node.last_check = Instant::now();

        let result = node.clone();
        drop(nodes);

        self.update_cluster_health()?;
        Ok(result)
    }

    /// Get node health
    pub fn get_node_health(&self, node_id: &str) -> HealthResult<NodeHealth> {
        Ok(self
            .nodes
            .read()
            .map_err(|_| HealthError::NodeNotFound("Failed to acquire lock".to_string()))?
            .get(node_id)
            .ok_or_else(|| HealthError::NodeNotFound(node_id.to_string()))?
            .clone())
    }

    /// Get cluster health
    pub fn get_cluster_health(&self) -> HealthResult<ClusterHealth> {
        Ok(self
            .cluster_health
            .read()
            .map_err(|_| HealthError::CheckFailed("Failed to acquire lock".to_string()))?
            .clone())
    }

    /// Update cluster health based on node health
    fn update_cluster_health(&self) -> HealthResult<()> {
        let nodes = self
            .nodes
            .read()
            .map_err(|_| HealthError::CheckFailed("Failed to acquire lock".to_string()))?;

        let mut healthy = 0;
        let mut degraded = 0;
        let mut unhealthy = 0;
        let mut unknown = 0;

        for node in nodes.values() {
            match node.status {
                HealthStatus::Healthy => healthy += 1,
                HealthStatus::Degraded => degraded += 1,
                HealthStatus::Unhealthy => unhealthy += 1,
                HealthStatus::Unknown => unknown += 1,
            }
        }

        let total = nodes.len();

        // Determine overall cluster status
        let status = if unhealthy > total / 2 {
            HealthStatus::Unhealthy
        } else if unhealthy > 0 || degraded > total / 2 {
            HealthStatus::Degraded
        } else if healthy == total && total > 0 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unknown
        };

        *self
            .cluster_health
            .write()
            .map_err(|_| HealthError::CheckFailed("Failed to acquire lock".to_string()))? =
            ClusterHealth {
                total_nodes: total,
                healthy_nodes: healthy,
                degraded_nodes: degraded,
                unhealthy_nodes: unhealthy,
                unknown_nodes: unknown,
                status,
                last_updated: Instant::now(),
            };

        Ok(())
    }

    /// Get all unhealthy nodes
    pub fn get_unhealthy_nodes(&self) -> HealthResult<Vec<NodeHealth>> {
        Ok(self
            .nodes
            .read()
            .map_err(|_| HealthError::CheckFailed("Failed to acquire lock".to_string()))?
            .values()
            .filter(|n| n.status == HealthStatus::Unhealthy)
            .cloned()
            .collect())
    }

    /// Get all healthy nodes
    pub fn get_healthy_nodes(&self) -> HealthResult<Vec<NodeHealth>> {
        Ok(self
            .nodes
            .read()
            .map_err(|_| HealthError::CheckFailed("Failed to acquire lock".to_string()))?
            .values()
            .filter(|n| n.status == HealthStatus::Healthy)
            .cloned()
            .collect())
    }

    /// Check if cluster is healthy
    pub fn is_cluster_healthy(&self) -> HealthResult<bool> {
        Ok(self.get_cluster_health()?.status == HealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_health_monitor_creation() {
        let config = HealthConfig::default();
        let _monitor = ClusterHealthMonitor::new(config);
    }

    #[test]
    fn test_register_node() {
        let config = HealthConfig::default();
        let monitor = ClusterHealthMonitor::new(config);

        monitor
            .register_node("node1".to_string())
            .expect("register node");

        let health = monitor.get_node_health("node1").expect("get node health");
        assert_eq!(health.status, HealthStatus::Unknown);
    }

    #[test]
    fn test_healthy_node() {
        let config = HealthConfig {
            success_threshold: 2,
            ..Default::default()
        };
        let monitor = ClusterHealthMonitor::new(config);

        monitor
            .register_node("node1".to_string())
            .expect("register node");

        let metrics = vec![
            HealthMetric {
                name: "cpu".to_string(),
                value: 0.5,
                threshold: 0.8,
                healthy: true,
            },
            HealthMetric {
                name: "memory".to_string(),
                value: 0.6,
                threshold: 0.85,
                healthy: true,
            },
        ];

        // First check
        monitor
            .check_node_health("node1", metrics.clone())
            .expect("check node health");

        // Second check (should mark as healthy)
        let health = monitor
            .check_node_health("node1", metrics)
            .expect("check node health");

        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_unhealthy_node() {
        let config = HealthConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let monitor = ClusterHealthMonitor::new(config);

        monitor
            .register_node("node1".to_string())
            .expect("register node");

        let metrics = vec![HealthMetric {
            name: "cpu".to_string(),
            value: 0.95,
            threshold: 0.8,
            healthy: false,
        }];

        // First check (should mark as degraded)
        monitor
            .check_node_health("node1", metrics.clone())
            .expect("check node health");

        // Second check (should mark as unhealthy)
        let health = monitor
            .check_node_health("node1", metrics)
            .expect("check node health");

        assert_eq!(health.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_cluster_health() {
        let config = HealthConfig::default();
        let monitor = ClusterHealthMonitor::new(config);

        monitor
            .register_node("node1".to_string())
            .expect("register node");
        monitor
            .register_node("node2".to_string())
            .expect("register node");

        let cluster_health = monitor.get_cluster_health().expect("get cluster health");
        assert_eq!(cluster_health.total_nodes, 2);
    }

    #[test]
    fn test_get_unhealthy_nodes() {
        let config = HealthConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let monitor = ClusterHealthMonitor::new(config);

        monitor
            .register_node("node1".to_string())
            .expect("register node");
        monitor
            .register_node("node2".to_string())
            .expect("register node");

        let bad_metrics = vec![HealthMetric {
            name: "cpu".to_string(),
            value: 0.95,
            threshold: 0.8,
            healthy: false,
        }];

        monitor
            .check_node_health("node1", bad_metrics)
            .expect("check node health");

        let unhealthy = monitor.get_unhealthy_nodes().expect("get unhealthy nodes");
        assert_eq!(unhealthy.len(), 1);
        assert_eq!(unhealthy[0].node_id, "node1");
    }

    #[test]
    fn test_unregister_node() {
        let config = HealthConfig::default();
        let monitor = ClusterHealthMonitor::new(config);

        monitor
            .register_node("node1".to_string())
            .expect("register node");
        monitor.unregister_node("node1").expect("unregister node");

        assert!(monitor.get_node_health("node1").is_err());
    }
}
