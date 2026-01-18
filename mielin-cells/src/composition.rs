//! Agent Composition and Behaviors
//!
//! Provides a composition-based system for extending agent capabilities
//! through reusable behaviors, avoiding deep inheritance hierarchies.

use crate::{Agent, AgentId, AgentState};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for behaviors
pub type BehaviorId = Uuid;

/// Behavior execution context
#[derive(Debug, Clone)]
pub struct BehaviorContext {
    /// Agent ID this behavior is executing for
    pub agent_id: AgentId,
    /// Agent current state
    pub agent_state: AgentState,
    /// Execution timestamp
    pub timestamp: u64,
    /// Custom context data
    pub data: HashMap<String, String>,
}

impl BehaviorContext {
    /// Create a new behavior context for an agent
    pub fn new(agent: &Agent) -> Self {
        Self {
            agent_id: agent.id(),
            agent_state: agent.state().clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            data: HashMap::new(),
        }
    }

    /// Add context data
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    /// Get context data
    pub fn get_data(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }
}

/// Result of behavior execution
#[derive(Debug, Clone)]
pub enum BehaviorResult {
    /// Behavior executed successfully
    Success,
    /// Behavior execution failed
    Failed(String),
    /// Behavior was skipped (e.g., preconditions not met)
    Skipped(String),
}

impl BehaviorResult {
    pub fn is_success(&self) -> bool {
        matches!(self, BehaviorResult::Success)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, BehaviorResult::Failed(_))
    }
}

/// Behavior trait for composable agent capabilities
pub trait Behavior: Send + Sync {
    /// Get behavior name
    fn name(&self) -> &str;

    /// Get behavior description
    fn description(&self) -> &str {
        ""
    }

    /// Check if behavior can execute in current context
    fn can_execute(&self, _context: &BehaviorContext) -> bool {
        true
    }

    /// Execute the behavior
    fn execute(&self, context: &BehaviorContext) -> BehaviorResult;

    /// Called when behavior is attached to an agent
    fn on_attach(&self, _agent_id: AgentId) -> BehaviorResult {
        BehaviorResult::Success
    }

    /// Called when behavior is detached from an agent
    fn on_detach(&self, _agent_id: AgentId) -> BehaviorResult {
        BehaviorResult::Success
    }
}

/// A simple behavior defined by a closure
pub struct ClosureBehavior {
    name: String,
    description: String,
    executor: Arc<dyn Fn(&BehaviorContext) -> BehaviorResult + Send + Sync>,
}

impl ClosureBehavior {
    /// Create a new closure-based behavior
    pub fn new<F>(name: impl Into<String>, executor: F) -> Self
    where
        F: Fn(&BehaviorContext) -> BehaviorResult + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            description: String::new(),
            executor: Arc::new(executor),
        }
    }

    /// Set behavior description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

impl Behavior for ClosureBehavior {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn execute(&self, context: &BehaviorContext) -> BehaviorResult {
        (self.executor)(context)
    }
}

/// Logging behavior - logs agent state changes
pub struct LoggingBehavior {
    name: String,
}

impl LoggingBehavior {
    pub fn new() -> Self {
        Self {
            name: "logging".to_string(),
        }
    }
}

impl Default for LoggingBehavior {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for LoggingBehavior {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Logs agent state and activity"
    }

    fn execute(&self, context: &BehaviorContext) -> BehaviorResult {
        println!(
            "[Log] Agent {} in state {:?} at {}",
            context.agent_id, context.agent_state, context.timestamp
        );
        BehaviorResult::Success
    }
}

/// Metrics collection behavior
pub struct MetricsBehavior {
    name: String,
    metrics: Arc<RwLock<HashMap<String, u64>>>,
}

impl MetricsBehavior {
    pub fn new() -> Self {
        Self {
            name: "metrics".to_string(),
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get metric value
    pub fn get_metric(&self, key: &str) -> Option<u64> {
        self.metrics
            .read()
            .expect("Lock poisoned: metrics")
            .get(key)
            .copied()
    }

    /// Get all metrics
    pub fn all_metrics(&self) -> HashMap<String, u64> {
        self.metrics.read().expect("Lock poisoned: metrics").clone()
    }
}

impl Default for MetricsBehavior {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for MetricsBehavior {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Collects agent execution metrics"
    }

    fn execute(&self, context: &BehaviorContext) -> BehaviorResult {
        let mut metrics = self.metrics.write().expect("Lock poisoned: metrics");
        let key = format!("state_{:?}", context.agent_state);
        *metrics.entry(key).or_insert(0) += 1;
        *metrics.entry("total_executions".to_string()).or_insert(0) += 1;
        BehaviorResult::Success
    }
}

/// Health check behavior
pub struct HealthCheckBehavior {
    name: String,
    healthy_states: Vec<AgentState>,
}

impl HealthCheckBehavior {
    pub fn new() -> Self {
        Self {
            name: "health_check".to_string(),
            healthy_states: vec![AgentState::Running, AgentState::Created],
        }
    }

    pub fn with_healthy_states(mut self, states: Vec<AgentState>) -> Self {
        self.healthy_states = states;
        self
    }
}

impl Default for HealthCheckBehavior {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for HealthCheckBehavior {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Checks agent health based on state"
    }

    fn execute(&self, context: &BehaviorContext) -> BehaviorResult {
        if self.healthy_states.contains(&context.agent_state) {
            BehaviorResult::Success
        } else {
            BehaviorResult::Failed(format!("Unhealthy state: {:?}", context.agent_state))
        }
    }
}

/// Behavior composition - manages multiple behaviors for an agent
pub struct BehaviorComposite {
    agent_id: AgentId,
    behaviors: RwLock<HashMap<BehaviorId, Arc<dyn Behavior>>>,
    execution_order: RwLock<Vec<BehaviorId>>,
}

impl BehaviorComposite {
    /// Create a new behavior composite for an agent
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            behaviors: RwLock::new(HashMap::new()),
            execution_order: RwLock::new(Vec::new()),
        }
    }

    /// Attach a behavior
    pub fn attach(&self, behavior: Arc<dyn Behavior>) -> BehaviorId {
        let id = Uuid::new_v4();

        // Call on_attach hook
        behavior.on_attach(self.agent_id);

        // Store behavior
        self.behaviors
            .write()
            .expect("Lock poisoned: behaviors")
            .insert(id, behavior);
        self.execution_order
            .write()
            .expect("Lock poisoned: execution_order")
            .push(id);

        id
    }

    /// Detach a behavior
    pub fn detach(&self, behavior_id: &BehaviorId) -> Option<Arc<dyn Behavior>> {
        let behavior = self
            .behaviors
            .write()
            .expect("Lock poisoned: behaviors")
            .remove(behavior_id)?;

        // Call on_detach hook
        behavior.on_detach(self.agent_id);

        // Remove from execution order
        self.execution_order
            .write()
            .expect("Lock poisoned: execution_order")
            .retain(|id| id != behavior_id);

        Some(behavior)
    }

    /// Get a behavior by ID
    pub fn get(&self, behavior_id: &BehaviorId) -> Option<Arc<dyn Behavior>> {
        self.behaviors
            .read()
            .expect("Lock poisoned: behaviors")
            .get(behavior_id)
            .cloned()
    }

    /// Execute all behaviors in order
    pub fn execute_all(&self, context: &BehaviorContext) -> Vec<(BehaviorId, BehaviorResult)> {
        let order = self
            .execution_order
            .read()
            .expect("Lock poisoned: execution_order");
        let behaviors = self.behaviors.read().expect("Lock poisoned: behaviors");

        order
            .iter()
            .filter_map(|id| {
                let behavior = behaviors.get(id)?;
                if behavior.can_execute(context) {
                    let result = behavior.execute(context);
                    Some((*id, result))
                } else {
                    Some((
                        *id,
                        BehaviorResult::Skipped("Preconditions not met".to_string()),
                    ))
                }
            })
            .collect()
    }

    /// Execute a specific behavior
    pub fn execute(
        &self,
        behavior_id: &BehaviorId,
        context: &BehaviorContext,
    ) -> Option<BehaviorResult> {
        let behaviors = self.behaviors.read().expect("Lock poisoned: behaviors");
        let behavior = behaviors.get(behavior_id)?;

        if behavior.can_execute(context) {
            Some(behavior.execute(context))
        } else {
            Some(BehaviorResult::Skipped("Preconditions not met".to_string()))
        }
    }

    /// Get number of attached behaviors
    pub fn count(&self) -> usize {
        self.behaviors
            .read()
            .expect("Lock poisoned: behaviors")
            .len()
    }

    /// Get all behavior IDs
    pub fn behavior_ids(&self) -> Vec<BehaviorId> {
        self.execution_order
            .read()
            .expect("Lock poisoned: execution_order")
            .clone()
    }

    /// Get behavior names
    pub fn behavior_names(&self) -> Vec<String> {
        let order = self
            .execution_order
            .read()
            .expect("Lock poisoned: execution_order");
        let behaviors = self.behaviors.read().expect("Lock poisoned: behaviors");

        order
            .iter()
            .filter_map(|id| behaviors.get(id).map(|b| b.name().to_string()))
            .collect()
    }
}

/// Registry for managing agent behaviors
pub struct BehaviorRegistry {
    /// Agent ID -> BehaviorComposite
    composites: RwLock<HashMap<AgentId, Arc<BehaviorComposite>>>,
}

impl BehaviorRegistry {
    /// Create a new behavior registry
    pub fn new() -> Self {
        Self {
            composites: RwLock::new(HashMap::new()),
        }
    }

    /// Register an agent (creates behavior composite)
    pub fn register_agent(&self, agent_id: AgentId) -> Arc<BehaviorComposite> {
        let composite = Arc::new(BehaviorComposite::new(agent_id));
        self.composites
            .write()
            .expect("Lock poisoned: composites")
            .insert(agent_id, Arc::clone(&composite));
        composite
    }

    /// Get composite for an agent
    pub fn get_composite(&self, agent_id: &AgentId) -> Option<Arc<BehaviorComposite>> {
        self.composites
            .read()
            .expect("Lock poisoned: composites")
            .get(agent_id)
            .cloned()
    }

    /// Unregister an agent
    pub fn unregister_agent(&self, agent_id: &AgentId) -> Option<Arc<BehaviorComposite>> {
        self.composites
            .write()
            .expect("Lock poisoned: composites")
            .remove(agent_id)
    }

    /// Execute all behaviors for an agent
    pub fn execute_for_agent(
        &self,
        agent_id: &AgentId,
        context: &BehaviorContext,
    ) -> Option<Vec<(BehaviorId, BehaviorResult)>> {
        let composite = self.get_composite(agent_id)?;
        Some(composite.execute_all(context))
    }

    /// Get number of registered agents
    pub fn agent_count(&self) -> usize {
        self.composites
            .read()
            .expect("Lock poisoned: composites")
            .len()
    }
}

impl Default for BehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behavior_context() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let context = BehaviorContext::new(&agent);

        assert_eq!(context.agent_id, agent.id());
        assert_eq!(context.agent_state, AgentState::Created);
    }

    #[test]
    fn test_closure_behavior() {
        let behavior = ClosureBehavior::new("test", |_| BehaviorResult::Success);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let context = BehaviorContext::new(&agent);

        let result = behavior.execute(&context);
        assert!(result.is_success());
    }

    #[test]
    fn test_logging_behavior() {
        let behavior = LoggingBehavior::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let context = BehaviorContext::new(&agent);

        let result = behavior.execute(&context);
        assert!(result.is_success());
    }

    #[test]
    fn test_metrics_behavior() {
        let behavior = MetricsBehavior::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let context = BehaviorContext::new(&agent);

        behavior.execute(&context);
        behavior.execute(&context);

        assert_eq!(behavior.get_metric("total_executions"), Some(2));
    }

    #[test]
    fn test_health_check_behavior() {
        let behavior = HealthCheckBehavior::new();
        let mut agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        // Created is healthy
        let context = BehaviorContext::new(&agent);
        let result = behavior.execute(&context);
        assert!(result.is_success());

        // Error is unhealthy
        agent.set_state(AgentState::Error);
        let context = BehaviorContext::new(&agent);
        let result = behavior.execute(&context);
        assert!(result.is_failed());
    }

    #[test]
    fn test_behavior_composite() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let composite = BehaviorComposite::new(agent.id());

        // Attach behaviors
        let behavior1 = Arc::new(LoggingBehavior::new());
        let behavior2 = Arc::new(MetricsBehavior::new());

        let id1 = composite.attach(behavior1);
        let _id2 = composite.attach(behavior2);

        assert_eq!(composite.count(), 2);

        // Execute all
        let context = BehaviorContext::new(&agent);
        let results = composite.execute_all(&context);
        assert_eq!(results.len(), 2);

        // Detach
        composite.detach(&id1);
        assert_eq!(composite.count(), 1);
    }

    #[test]
    fn test_behavior_registry() {
        let registry = BehaviorRegistry::new();
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        let composite = registry.register_agent(agent.id());
        assert_eq!(registry.agent_count(), 1);

        // Attach behavior
        let behavior = Arc::new(LoggingBehavior::new());
        composite.attach(behavior);

        // Execute
        let context = BehaviorContext::new(&agent);
        let results = registry.execute_for_agent(&agent.id(), &context);
        assert!(results.is_some());
        assert_eq!(results.unwrap().len(), 1);

        // Unregister
        registry.unregister_agent(&agent.id());
        assert_eq!(registry.agent_count(), 0);
    }

    #[test]
    fn test_behavior_names() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let composite = BehaviorComposite::new(agent.id());

        composite.attach(Arc::new(LoggingBehavior::new()));
        composite.attach(Arc::new(MetricsBehavior::new()));
        composite.attach(Arc::new(HealthCheckBehavior::new()));

        let names = composite.behavior_names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"logging".to_string()));
        assert!(names.contains(&"metrics".to_string()));
        assert!(names.contains(&"health_check".to_string()));
    }
}
