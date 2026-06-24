//! Agent Orchestration System
//!
//! Provides declarative deployment specs, auto-scaling, placement constraints,
//! and affinity/anti-affinity rules for managing agent deployments.

use crate::discovery::Location;
use crate::{AgentId, Capability, ServiceRegistry, Version};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Orchestration-related errors
#[derive(Debug, Error)]
pub enum OrchestrationError {
    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),
    #[error("No suitable node found: {0}")]
    NoSuitableNode(String),
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
    #[error("Scaling failed: {0}")]
    ScalingFailed(String),
    #[error("Deployment not found: {0}")]
    DeploymentNotFound(String),
}

/// Resource requirements for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu_cores: f32,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub network_mbps: Option<u32>,
}

impl ResourceRequirements {
    pub fn new(cpu_cores: f32, memory_mb: u64, disk_mb: u64) -> Self {
        Self {
            cpu_cores,
            memory_mb,
            disk_mb,
            network_mbps: None,
        }
    }

    pub fn with_network(mut self, network_mbps: u32) -> Self {
        self.network_mbps = Some(network_mbps);
        self
    }
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self::new(1.0, 512, 1024)
    }
}

/// Placement constraints for agent deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlacementConstraint {
    /// Must run on specific node
    NodeAffinity(String),
    /// Must not run on specific node
    NodeAntiAffinity(String),
    /// Must run on same node as agents with label
    PodAffinity(String),
    /// Must not run on same node as agents with label
    PodAntiAffinity(String),
    /// Must run in specific region
    RegionAffinity(String),
    /// Must not run in specific region
    RegionAntiAffinity(String),
    /// Must run within distance of location
    LocationConstraint {
        location: Location,
        max_distance_km: f64,
    },
    /// Required node labels
    NodeSelector(HashMap<String, String>),
}

/// Scaling policy for auto-scaling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingPolicy {
    pub min_replicas: u32,
    pub max_replicas: u32,
    pub target_cpu_percent: Option<u8>,
    pub target_memory_percent: Option<u8>,
    pub target_request_rate: Option<f64>,
    pub scale_up_threshold: f64,
    pub scale_down_threshold: f64,
    pub cooldown_seconds: u64,
}

impl ScalingPolicy {
    pub fn new(min_replicas: u32, max_replicas: u32) -> Self {
        Self {
            min_replicas,
            max_replicas,
            target_cpu_percent: Some(70),
            target_memory_percent: Some(80),
            target_request_rate: None,
            scale_up_threshold: 0.8,
            scale_down_threshold: 0.3,
            cooldown_seconds: 300, // 5 minutes
        }
    }

    pub fn with_cpu_target(mut self, percent: u8) -> Self {
        self.target_cpu_percent = Some(percent);
        self
    }

    pub fn with_memory_target(mut self, percent: u8) -> Self {
        self.target_memory_percent = Some(percent);
        self
    }

    pub fn with_request_rate_target(mut self, rate: f64) -> Self {
        self.target_request_rate = Some(rate);
        self
    }

    pub fn with_thresholds(mut self, scale_up: f64, scale_down: f64) -> Self {
        self.scale_up_threshold = scale_up;
        self.scale_down_threshold = scale_down;
        self
    }

    pub fn with_cooldown(mut self, seconds: u64) -> Self {
        self.cooldown_seconds = seconds;
        self
    }
}

/// Deployment strategy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentStrategy {
    /// Replace all instances at once
    Recreate,
    /// Rolling update with configurable batch size
    RollingUpdate,
    /// Blue-green deployment
    BlueGreen,
    /// Canary deployment
    Canary,
}

/// Deployment specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSpec {
    pub name: String,
    pub version: Version,
    pub replicas: u32,
    pub service_name: String,
    pub capabilities: Vec<Capability>,
    pub resources: ResourceRequirements,
    pub placement_constraints: Vec<PlacementConstraint>,
    pub labels: HashMap<String, String>,
    pub strategy: DeploymentStrategy,
    pub scaling_policy: Option<ScalingPolicy>,
}

impl DeploymentSpec {
    pub fn new(
        name: impl Into<String>,
        service_name: impl Into<String>,
        version: Version,
        replicas: u32,
    ) -> Self {
        Self {
            name: name.into(),
            version,
            replicas,
            service_name: service_name.into(),
            capabilities: Vec::new(),
            resources: ResourceRequirements::default(),
            placement_constraints: Vec::new(),
            labels: HashMap::new(),
            strategy: DeploymentStrategy::RollingUpdate,
            scaling_policy: None,
        }
    }

    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.capabilities.push(capability);
        self
    }

    pub fn with_resources(mut self, resources: ResourceRequirements) -> Self {
        self.resources = resources;
        self
    }

    pub fn with_constraint(mut self, constraint: PlacementConstraint) -> Self {
        self.placement_constraints.push(constraint);
        self
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    pub fn with_strategy(mut self, strategy: DeploymentStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_auto_scaling(mut self, policy: ScalingPolicy) -> Self {
        self.scaling_policy = Some(policy);
        self
    }
}

/// Deployment status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentStatus {
    Pending,
    Deploying,
    Running,
    Scaling,
    Failed,
    Terminating,
    Terminated,
}

/// Running deployment state
#[derive(Debug, Clone)]
pub struct Deployment {
    pub spec: DeploymentSpec,
    pub status: DeploymentStatus,
    pub agents: HashSet<AgentId>,
    pub desired_replicas: u32,
    pub ready_replicas: u32,
    pub created_at: Instant,
    pub last_scale_time: Option<Instant>,
}

impl Deployment {
    pub fn new(spec: DeploymentSpec) -> Self {
        Self {
            desired_replicas: spec.replicas,
            status: DeploymentStatus::Pending,
            agents: HashSet::new(),
            ready_replicas: 0,
            created_at: Instant::now(),
            last_scale_time: None,
            spec,
        }
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self.status, DeploymentStatus::Running)
            && self.ready_replicas >= self.desired_replicas
    }

    pub fn can_scale(&self, cooldown: Duration) -> bool {
        if let Some(last_scale) = self.last_scale_time {
            last_scale.elapsed() >= cooldown
        } else {
            true
        }
    }
}

/// Node for scheduling agents
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub labels: HashMap<String, String>,
    pub location: Option<Location>,
    pub available_cpu: f32,
    pub available_memory_mb: u64,
    pub available_disk_mb: u64,
    pub agent_count: usize,
}

impl Node {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            labels: HashMap::new(),
            location: None,
            available_cpu: 8.0,
            available_memory_mb: 16384,
            available_disk_mb: 102400,
            agent_count: 0,
        }
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    pub fn with_resources(mut self, cpu: f32, memory_mb: u64, disk_mb: u64) -> Self {
        self.available_cpu = cpu;
        self.available_memory_mb = memory_mb;
        self.available_disk_mb = disk_mb;
        self
    }

    pub fn can_fit(&self, resources: &ResourceRequirements) -> bool {
        self.available_cpu >= resources.cpu_cores
            && self.available_memory_mb >= resources.memory_mb
            && self.available_disk_mb >= resources.disk_mb
    }

    pub fn allocate(&mut self, resources: &ResourceRequirements) {
        self.available_cpu -= resources.cpu_cores;
        self.available_memory_mb -= resources.memory_mb;
        self.available_disk_mb -= resources.disk_mb;
        self.agent_count += 1;
    }

    pub fn deallocate(&mut self, resources: &ResourceRequirements) {
        self.available_cpu += resources.cpu_cores;
        self.available_memory_mb += resources.memory_mb;
        self.available_disk_mb += resources.disk_mb;
        self.agent_count = self.agent_count.saturating_sub(1);
    }
}

/// Scheduler for placing agents on nodes
pub struct Scheduler {
    nodes: Arc<RwLock<HashMap<String, Node>>>,
    agent_placements: Arc<RwLock<HashMap<AgentId, String>>>, // agent_id -> node_id
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            agent_placements: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_node(&self, node: Node) {
        let mut nodes = self.nodes.write().expect("Lock poisoned: nodes");
        nodes.insert(node.id.clone(), node);
    }

    pub fn deregister_node(&self, node_id: &str) {
        let mut nodes = self.nodes.write().expect("Lock poisoned: nodes");
        nodes.remove(node_id);
    }

    pub fn select_node(&self, spec: &DeploymentSpec) -> Result<String, OrchestrationError> {
        let nodes = self.nodes.read().expect("Lock poisoned: nodes");
        let agent_placements = self
            .agent_placements
            .read()
            .expect("Lock poisoned: agent_placements");

        let mut candidates: Vec<&Node> = nodes.values().collect();

        // Filter by resource requirements
        candidates.retain(|node| node.can_fit(&spec.resources));

        if candidates.is_empty() {
            return Err(OrchestrationError::NoSuitableNode(
                "No nodes with sufficient resources".to_string(),
            ));
        }

        // Apply placement constraints
        for constraint in &spec.placement_constraints {
            match constraint {
                PlacementConstraint::NodeAffinity(node_id) => {
                    candidates.retain(|node| node.id == *node_id);
                }
                PlacementConstraint::NodeAntiAffinity(node_id) => {
                    candidates.retain(|node| node.id != *node_id);
                }
                PlacementConstraint::NodeSelector(required_labels) => {
                    candidates.retain(|node| {
                        required_labels
                            .iter()
                            .all(|(k, v)| node.labels.get(k) == Some(v))
                    });
                }
                PlacementConstraint::LocationConstraint {
                    location,
                    max_distance_km,
                } => {
                    candidates.retain(|node| {
                        if let Some(node_loc) = &node.location {
                            location.distance_to(node_loc) <= *max_distance_km
                        } else {
                            false
                        }
                    });
                }
                PlacementConstraint::PodAffinity(_label) => {
                    // Find nodes that have agents with this label
                    let mut affinity_nodes = HashSet::new();
                    for node_id in agent_placements.values() {
                        // This would need access to agent labels
                        // Simplified for now
                        affinity_nodes.insert(node_id.clone());
                    }
                    candidates.retain(|node| affinity_nodes.contains(&node.id));
                }
                PlacementConstraint::PodAntiAffinity(_label) => {
                    // Spread agents across different nodes
                    // Simplified: prefer nodes with fewer agents
                }
                PlacementConstraint::RegionAffinity(region) => {
                    candidates.retain(|node| node.labels.get("region") == Some(region));
                }
                PlacementConstraint::RegionAntiAffinity(region) => {
                    candidates.retain(|node| node.labels.get("region") != Some(region));
                }
            }
        }

        if candidates.is_empty() {
            return Err(OrchestrationError::NoSuitableNode(
                "No nodes matching placement constraints".to_string(),
            ));
        }

        // Select node with most available resources (bin packing)
        let selected = candidates
            .iter()
            .max_by(|a, b| {
                (a.available_cpu, a.available_memory_mb)
                    .partial_cmp(&(b.available_cpu, b.available_memory_mb))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("Candidates is not empty");

        Ok(selected.id.clone())
    }

    pub fn schedule(
        &self,
        agent_id: AgentId,
        spec: &DeploymentSpec,
    ) -> Result<String, OrchestrationError> {
        let node_id = self.select_node(spec)?;

        // Allocate resources
        {
            let mut nodes = self.nodes.write().expect("Lock poisoned: nodes");
            if let Some(node) = nodes.get_mut(&node_id) {
                node.allocate(&spec.resources);
            }
        }

        // Record placement
        {
            let mut agent_placements = self
                .agent_placements
                .write()
                .expect("Lock poisoned: agent_placements");
            agent_placements.insert(agent_id, node_id.clone());
        }

        Ok(node_id)
    }

    pub fn deschedule(&self, agent_id: &AgentId, spec: &DeploymentSpec) {
        let node_id = {
            let mut agent_placements = self
                .agent_placements
                .write()
                .expect("Lock poisoned: agent_placements");
            agent_placements.remove(agent_id)
        };

        if let Some(node_id) = node_id {
            let mut nodes = self.nodes.write().expect("Lock poisoned: nodes");
            if let Some(node) = nodes.get_mut(&node_id) {
                node.deallocate(&spec.resources);
            }
        }
    }

    pub fn get_node_agent_count(&self, node_id: &str) -> usize {
        let nodes = self.nodes.read().expect("Lock poisoned: nodes");
        nodes.get(node_id).map(|n| n.agent_count).unwrap_or(0)
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Orchestrator manages deployments and auto-scaling
pub struct Orchestrator {
    deployments: Arc<RwLock<HashMap<String, Deployment>>>,
    scheduler: Arc<Scheduler>,
    #[allow(dead_code)]
    registry: Arc<ServiceRegistry>,
}

impl Orchestrator {
    pub fn new(scheduler: Arc<Scheduler>, registry: Arc<ServiceRegistry>) -> Self {
        Self {
            deployments: Arc::new(RwLock::new(HashMap::new())),
            scheduler,
            registry,
        }
    }

    pub fn create_deployment(&self, spec: DeploymentSpec) -> Result<(), OrchestrationError> {
        let deployment = Deployment::new(spec.clone());
        let mut deployments = self
            .deployments
            .write()
            .expect("Lock poisoned: deployments");
        deployments.insert(spec.name.clone(), deployment);
        Ok(())
    }

    pub fn delete_deployment(&self, name: &str) -> Result<(), OrchestrationError> {
        let mut deployments = self
            .deployments
            .write()
            .expect("Lock poisoned: deployments");
        let deployment = deployments
            .remove(name)
            .ok_or_else(|| OrchestrationError::DeploymentNotFound(name.to_string()))?;

        // Deschedule all agents
        for agent_id in &deployment.agents {
            self.scheduler.deschedule(agent_id, &deployment.spec);
        }

        Ok(())
    }

    pub fn scale_deployment(&self, name: &str, replicas: u32) -> Result<(), OrchestrationError> {
        let mut deployments = self
            .deployments
            .write()
            .expect("Lock poisoned: deployments");
        let deployment = deployments
            .get_mut(name)
            .ok_or_else(|| OrchestrationError::DeploymentNotFound(name.to_string()))?;

        if let Some(policy) = &deployment.spec.scaling_policy {
            if replicas < policy.min_replicas || replicas > policy.max_replicas {
                return Err(OrchestrationError::ScalingFailed(format!(
                    "Replicas {} out of range [{}, {}]",
                    replicas, policy.min_replicas, policy.max_replicas
                )));
            }
        }

        deployment.desired_replicas = replicas;
        deployment.last_scale_time = Some(Instant::now());
        deployment.status = DeploymentStatus::Scaling;

        Ok(())
    }

    pub fn get_deployment(&self, name: &str) -> Option<Deployment> {
        let deployments = self.deployments.read().expect("Lock poisoned: deployments");
        deployments.get(name).cloned()
    }

    pub fn list_deployments(&self) -> Vec<String> {
        let deployments = self.deployments.read().expect("Lock poisoned: deployments");
        deployments.keys().cloned().collect()
    }

    pub fn reconcile(&self) -> Result<(), OrchestrationError> {
        let mut deployments = self
            .deployments
            .write()
            .expect("Lock poisoned: deployments");

        for (name, deployment) in deployments.iter_mut() {
            let current_replicas = deployment.agents.len() as u32;
            let desired = deployment.desired_replicas;

            if current_replicas < desired {
                // Scale up
                let to_add = desired - current_replicas;
                for _ in 0..to_add {
                    let agent_id = AgentId::new_v4();
                    match self.scheduler.schedule(agent_id, &deployment.spec) {
                        Ok(_) => {
                            deployment.agents.insert(agent_id);
                        }
                        Err(e) => {
                            eprintln!("Failed to schedule agent for {}: {}", name, e);
                        }
                    }
                }
            } else if current_replicas > desired {
                // Scale down
                let to_remove = current_replicas - desired;
                let agents_to_remove: Vec<AgentId> = deployment
                    .agents
                    .iter()
                    .take(to_remove as usize)
                    .copied()
                    .collect();

                for agent_id in agents_to_remove {
                    self.scheduler.deschedule(&agent_id, &deployment.spec);
                    deployment.agents.remove(&agent_id);
                }
            }

            // Update status
            deployment.ready_replicas = deployment.agents.len() as u32;
            if deployment.ready_replicas == deployment.desired_replicas {
                deployment.status = DeploymentStatus::Running;
            }
        }

        Ok(())
    }

    pub fn auto_scale(&self) -> Result<(), OrchestrationError> {
        let deployments = self.deployments.read().expect("Lock poisoned: deployments");

        for (name, deployment) in deployments.iter() {
            if let Some(policy) = &deployment.spec.scaling_policy {
                let cooldown = Duration::from_secs(policy.cooldown_seconds);
                if !deployment.can_scale(cooldown) {
                    continue;
                }

                // Simplified auto-scaling logic
                // In practice, this would query metrics from monitoring system
                let current_load = 0.75; // Example: 75% CPU utilization

                let current_replicas = deployment.ready_replicas;
                let mut new_replicas = current_replicas;

                if current_load > policy.scale_up_threshold {
                    new_replicas = (current_replicas + 1).min(policy.max_replicas);
                } else if current_load < policy.scale_down_threshold {
                    new_replicas = (current_replicas.saturating_sub(1)).max(policy.min_replicas);
                }

                if new_replicas != current_replicas {
                    let name_clone = name.clone();
                    drop(deployments); // Release read lock
                    self.scale_deployment(&name_clone, new_replicas)?;
                    break; // Only scale one deployment per iteration
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_requirements() {
        let resources = ResourceRequirements::new(2.0, 2048, 10240).with_network(1000);

        assert_eq!(resources.cpu_cores, 2.0);
        assert_eq!(resources.memory_mb, 2048);
        assert_eq!(resources.disk_mb, 10240);
        assert_eq!(resources.network_mbps, Some(1000));
    }

    #[test]
    fn test_node_resource_allocation() {
        let mut node = Node::new("node-1").with_resources(4.0, 8192, 51200);

        let resources = ResourceRequirements::new(1.0, 1024, 10240);

        assert!(node.can_fit(&resources));

        node.allocate(&resources);
        assert_eq!(node.available_cpu, 3.0);
        assert_eq!(node.available_memory_mb, 7168);
        assert_eq!(node.agent_count, 1);

        node.deallocate(&resources);
        assert_eq!(node.available_cpu, 4.0);
        assert_eq!(node.available_memory_mb, 8192);
        assert_eq!(node.agent_count, 0);
    }

    #[test]
    fn test_deployment_spec_builder() {
        let spec = DeploymentSpec::new("test-deployment", "test-service", Version::new(1, 0, 0), 3)
            .with_capability(Capability::new("processing", Version::new(1, 0, 0)))
            .with_label("env", "production")
            .with_strategy(DeploymentStrategy::RollingUpdate);

        assert_eq!(spec.name, "test-deployment");
        assert_eq!(spec.replicas, 3);
        assert_eq!(spec.capabilities.len(), 1);
        assert_eq!(spec.labels.get("env"), Some(&"production".to_string()));
    }

    #[test]
    fn test_scheduler_node_selection() {
        let scheduler = Scheduler::new();

        scheduler.register_node(
            Node::new("node-1")
                .with_resources(4.0, 8192, 51200)
                .with_label("zone", "east"),
        );
        scheduler.register_node(
            Node::new("node-2")
                .with_resources(8.0, 16384, 102400)
                .with_label("zone", "west"),
        );

        let spec = DeploymentSpec::new("test", "service", Version::new(1, 0, 0), 1)
            .with_resources(ResourceRequirements::new(2.0, 2048, 10240));

        let node_id = scheduler.select_node(&spec).unwrap();
        // Should select node-2 (more resources available)
        assert_eq!(node_id, "node-2");
    }

    #[test]
    fn test_placement_constraints() {
        let scheduler = Scheduler::new();

        scheduler.register_node(
            Node::new("node-1")
                .with_resources(4.0, 8192, 51200)
                .with_label("zone", "east"),
        );
        scheduler.register_node(
            Node::new("node-2")
                .with_resources(8.0, 16384, 102400)
                .with_label("zone", "west"),
        );

        let mut labels = HashMap::new();
        labels.insert("zone".to_string(), "east".to_string());

        let spec = DeploymentSpec::new("test", "service", Version::new(1, 0, 0), 1)
            .with_resources(ResourceRequirements::new(2.0, 2048, 10240))
            .with_constraint(PlacementConstraint::NodeSelector(labels));

        let node_id = scheduler.select_node(&spec).unwrap();
        assert_eq!(node_id, "node-1");
    }

    #[test]
    fn test_deployment_lifecycle() {
        let scheduler = Arc::new(Scheduler::new());
        let registry = Arc::new(ServiceRegistry::new());
        let orchestrator = Orchestrator::new(scheduler.clone(), registry);

        // Register nodes
        scheduler.register_node(Node::new("node-1").with_resources(8.0, 16384, 102400));
        scheduler.register_node(Node::new("node-2").with_resources(8.0, 16384, 102400));

        let spec = DeploymentSpec::new("test-deployment", "test-service", Version::new(1, 0, 0), 2);

        orchestrator.create_deployment(spec).unwrap();

        let deployment = orchestrator.get_deployment("test-deployment").unwrap();
        assert_eq!(deployment.desired_replicas, 2);
        assert_eq!(deployment.status, DeploymentStatus::Pending);
    }

    #[test]
    fn test_deployment_scaling() {
        let scheduler = Arc::new(Scheduler::new());
        let registry = Arc::new(ServiceRegistry::new());
        let orchestrator = Orchestrator::new(scheduler.clone(), registry);

        scheduler.register_node(Node::new("node-1").with_resources(8.0, 16384, 102400));

        let spec = DeploymentSpec::new("scalable", "service", Version::new(1, 0, 0), 1)
            .with_auto_scaling(ScalingPolicy::new(1, 5));

        orchestrator.create_deployment(spec).unwrap();
        orchestrator.scale_deployment("scalable", 3).unwrap();

        let deployment = orchestrator.get_deployment("scalable").unwrap();
        assert_eq!(deployment.desired_replicas, 3);
    }

    #[test]
    fn test_scaling_policy_limits() {
        let scheduler = Arc::new(Scheduler::new());
        let registry = Arc::new(ServiceRegistry::new());
        let orchestrator = Orchestrator::new(scheduler.clone(), registry);

        scheduler.register_node(Node::new("node-1").with_resources(8.0, 16384, 102400));

        let spec = DeploymentSpec::new("limited", "service", Version::new(1, 0, 0), 2)
            .with_auto_scaling(ScalingPolicy::new(1, 3));

        orchestrator.create_deployment(spec).unwrap();

        // Try to scale beyond max
        let result = orchestrator.scale_deployment("limited", 5);
        assert!(result.is_err());

        // Try to scale below min
        let result = orchestrator.scale_deployment("limited", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_reconciliation() {
        let scheduler = Arc::new(Scheduler::new());
        let registry = Arc::new(ServiceRegistry::new());
        let orchestrator = Orchestrator::new(scheduler.clone(), registry);

        scheduler.register_node(Node::new("node-1").with_resources(8.0, 16384, 102400));

        let spec = DeploymentSpec::new("reconcile-test", "service", Version::new(1, 0, 0), 3);

        orchestrator.create_deployment(spec).unwrap();
        orchestrator.reconcile().unwrap();

        let deployment = orchestrator.get_deployment("reconcile-test").unwrap();
        assert_eq!(deployment.ready_replicas, 3);
        assert_eq!(deployment.status, DeploymentStatus::Running);
    }
}
