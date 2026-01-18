//! Failover Module
//!
//! Implements automatic failover mechanisms for high availability.

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FailoverError {
    #[error("No backup available")]
    NoBackupAvailable,
    #[error("Failover already in progress")]
    FailoverInProgress,
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        from: FailoverState,
        to: FailoverState,
    },
    #[error("Failover timeout")]
    Timeout,
}

pub type FailoverResult<T> = Result<T, FailoverError>;

/// Failover state for an agent
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailoverState {
    /// Agent is active
    Active,
    /// Agent is degraded (partial functionality)
    Degraded,
    /// Failover is in progress
    Failing,
    /// Agent has failed over
    FailedOver,
    /// Agent is recovering
    Recovering,
}

/// Failover strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailoverStrategy {
    /// Fail over immediately on first failure
    Immediate,
    /// Wait for N consecutive failures
    ConsecutiveFailures { count: usize },
    /// Wait for N failures in time window
    FailureRate { count: usize, window: Duration },
    /// Custom threshold-based strategy
    Threshold { threshold: f64 },
}

/// Failover policy
#[derive(Debug, Clone)]
pub struct FailoverPolicy {
    /// Strategy to use
    pub strategy: FailoverStrategy,
    /// Maximum failover time
    pub max_failover_time: Duration,
    /// Minimum time between failovers
    pub min_failover_interval: Duration,
    /// Whether to auto-recover
    pub auto_recover: bool,
}

impl Default for FailoverPolicy {
    fn default() -> Self {
        Self {
            strategy: FailoverStrategy::ConsecutiveFailures { count: 3 },
            max_failover_time: Duration::from_secs(30),
            min_failover_interval: Duration::from_secs(60),
            auto_recover: true,
        }
    }
}

/// Failover configuration
#[derive(Debug, Clone)]
pub struct FailoverConfig {
    /// Failover policy
    pub policy: FailoverPolicy,
    /// Maximum backup agents
    pub max_backups: usize,
    /// Health check interval
    pub health_check_interval: Duration,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            policy: FailoverPolicy::default(),
            max_backups: 3,
            health_check_interval: Duration::from_secs(5),
        }
    }
}

/// Failover event
#[derive(Debug, Clone)]
pub struct FailoverEvent {
    /// Agent that failed over
    pub agent_id: AgentId,
    /// Backup agent (if any)
    pub backup_id: Option<AgentId>,
    /// Time of failover
    pub timestamp: Instant,
    /// Reason for failover
    pub reason: String,
    /// Whether failover succeeded
    pub success: bool,
}

/// Failover decision
#[derive(Debug, Clone)]
pub struct FailoverDecision {
    /// Whether to fail over
    pub should_failover: bool,
    /// Target backup agent
    pub backup_id: Option<AgentId>,
    /// Reason for decision
    pub reason: String,
    /// Confidence (0.0 to 1.0)
    pub confidence: f64,
}

/// Agent failover status
#[derive(Debug, Clone)]
struct AgentFailoverStatus {
    state: FailoverState,
    failure_count: usize,
    last_failure: Option<Instant>,
    last_failover: Option<Instant>,
    backup_agents: Vec<AgentId>,
}

/// Failover coordinator manages failover for agents
pub struct FailoverCoordinator {
    /// Configuration
    config: FailoverConfig,
    /// Agent failover status
    agents: Arc<RwLock<HashMap<AgentId, AgentFailoverStatus>>>,
    /// Failover history
    events: Arc<RwLock<Vec<FailoverEvent>>>,
}

impl FailoverCoordinator {
    /// Create a new failover coordinator
    pub fn new(config: FailoverConfig) -> Self {
        Self {
            config,
            agents: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register an agent for failover management
    pub fn register_agent(&self, agent_id: AgentId, backups: Vec<AgentId>) -> FailoverResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        agents.insert(
            agent_id,
            AgentFailoverStatus {
                state: FailoverState::Active,
                failure_count: 0,
                last_failure: None,
                last_failover: None,
                backup_agents: backups,
            },
        );

        Ok(())
    }

    /// Unregister an agent
    pub fn unregister_agent(&self, agent_id: &AgentId) -> FailoverResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        agents.remove(agent_id);
        Ok(())
    }

    /// Report agent failure
    pub fn report_failure(
        &self,
        agent_id: &AgentId,
        reason: &str,
    ) -> FailoverResult<FailoverDecision> {
        let mut agents = self
            .agents
            .write()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        let status = agents
            .get_mut(agent_id)
            .ok_or_else(|| FailoverError::AgentNotFound(agent_id.to_string()))?;

        status.failure_count += 1;
        status.last_failure = Some(Instant::now());

        // Evaluate failover decision based on strategy
        let decision = self.evaluate_failover(agent_id, status, reason)?;

        if decision.should_failover {
            status.state = FailoverState::Failing;
        }

        Ok(decision)
    }

    /// Evaluate whether to failover
    fn evaluate_failover(
        &self,
        _agent_id: &AgentId,
        status: &AgentFailoverStatus,
        reason: &str,
    ) -> FailoverResult<FailoverDecision> {
        // Check if failover is already in progress
        if status.state == FailoverState::Failing {
            return Err(FailoverError::FailoverInProgress);
        }

        // Check minimum failover interval
        if let Some(last_failover) = status.last_failover {
            if last_failover.elapsed() < self.config.policy.min_failover_interval {
                return Ok(FailoverDecision {
                    should_failover: false,
                    backup_id: None,
                    reason: "Too soon since last failover".to_string(),
                    confidence: 0.0,
                });
            }
        }

        // Evaluate based on strategy
        let should_failover = match &self.config.policy.strategy {
            FailoverStrategy::Immediate => true,
            FailoverStrategy::ConsecutiveFailures { count } => status.failure_count >= *count,
            FailoverStrategy::FailureRate { count, window } => {
                if let Some(last_failure) = status.last_failure {
                    last_failure.elapsed() <= *window && status.failure_count >= *count
                } else {
                    false
                }
            }
            FailoverStrategy::Threshold { threshold } => {
                let failure_rate = status.failure_count as f64
                    / self.config.health_check_interval.as_secs() as f64;
                failure_rate >= *threshold
            }
        };

        let backup_id = if should_failover {
            status.backup_agents.first().cloned()
        } else {
            None
        };

        Ok(FailoverDecision {
            should_failover,
            backup_id,
            reason: reason.to_string(),
            confidence: if should_failover { 0.9 } else { 0.1 },
        })
    }

    /// Execute failover
    pub fn execute_failover(
        &self,
        agent_id: &AgentId,
        backup_id: &AgentId,
    ) -> FailoverResult<FailoverEvent> {
        let mut agents = self
            .agents
            .write()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        let status = agents
            .get_mut(agent_id)
            .ok_or_else(|| FailoverError::AgentNotFound(agent_id.to_string()))?;

        // Validate state transition
        if status.state != FailoverState::Failing {
            return Err(FailoverError::InvalidStateTransition {
                from: status.state.clone(),
                to: FailoverState::FailedOver,
            });
        }

        // Update status
        status.state = FailoverState::FailedOver;
        status.last_failover = Some(Instant::now());
        status.failure_count = 0;

        let event = FailoverEvent {
            agent_id: *agent_id,
            backup_id: Some(*backup_id),
            timestamp: Instant::now(),
            reason: "Automatic failover".to_string(),
            success: true,
        };

        // Record event
        self.events
            .write()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?
            .push(event.clone());

        Ok(event)
    }

    /// Report agent recovery
    pub fn report_recovery(&self, agent_id: &AgentId) -> FailoverResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        let status = agents
            .get_mut(agent_id)
            .ok_or_else(|| FailoverError::AgentNotFound(agent_id.to_string()))?;

        status.state = FailoverState::Active;
        status.failure_count = 0;
        status.last_failure = None;

        Ok(())
    }

    /// Get agent failover state
    pub fn get_state(&self, agent_id: &AgentId) -> FailoverResult<FailoverState> {
        let agents = self
            .agents
            .read()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        Ok(agents
            .get(agent_id)
            .ok_or_else(|| FailoverError::AgentNotFound(agent_id.to_string()))?
            .state
            .clone())
    }

    /// Get failover events
    pub fn get_events(&self) -> FailoverResult<Vec<FailoverEvent>> {
        Ok(self
            .events
            .read()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?
            .clone())
    }

    /// Get agent failure count
    pub fn get_failure_count(&self, agent_id: &AgentId) -> FailoverResult<usize> {
        let agents = self
            .agents
            .read()
            .map_err(|_| FailoverError::AgentNotFound("Failed to acquire lock".to_string()))?;

        Ok(agents
            .get(agent_id)
            .ok_or_else(|| FailoverError::AgentNotFound(agent_id.to_string()))?
            .failure_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn test_failover_coordinator_creation() {
        let config = FailoverConfig::default();
        let _coordinator = FailoverCoordinator::new(config);
    }

    #[test]
    fn test_register_agent() {
        let config = FailoverConfig::default();
        let coordinator = FailoverCoordinator::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let backup = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        coordinator
            .register_agent(agent.id(), vec![backup.id()])
            .expect("register agent");

        assert_eq!(
            coordinator.get_state(&agent.id()).expect("get state"),
            FailoverState::Active
        );
    }

    #[test]
    fn test_immediate_failover() {
        let config = FailoverConfig {
            policy: FailoverPolicy {
                strategy: FailoverStrategy::Immediate,
                ..Default::default()
            },
            ..Default::default()
        };
        let coordinator = FailoverCoordinator::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let backup = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        coordinator
            .register_agent(agent.id(), vec![backup.id()])
            .expect("register agent");

        let decision = coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");

        assert!(decision.should_failover);
        assert_eq!(decision.backup_id, Some(backup.id()));
    }

    #[test]
    fn test_consecutive_failures() {
        let config = FailoverConfig {
            policy: FailoverPolicy {
                strategy: FailoverStrategy::ConsecutiveFailures { count: 3 },
                ..Default::default()
            },
            ..Default::default()
        };
        let coordinator = FailoverCoordinator::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let backup = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        coordinator
            .register_agent(agent.id(), vec![backup.id()])
            .expect("register agent");

        // First failure - should not failover
        let decision = coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");
        assert!(!decision.should_failover);

        // Second failure - should not failover
        let decision = coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");
        assert!(!decision.should_failover);

        // Third failure - should failover
        let decision = coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");
        assert!(decision.should_failover);
    }

    #[test]
    fn test_execute_failover() {
        let config = FailoverConfig::default();
        let coordinator = FailoverCoordinator::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let backup = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        coordinator
            .register_agent(agent.id(), vec![backup.id()])
            .expect("register agent");

        // Report enough failures to trigger failover (default requires 3 consecutive failures)
        coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");
        coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");
        let decision = coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");

        assert!(decision.should_failover);

        let event = coordinator
            .execute_failover(&agent.id(), &backup.id())
            .expect("execute failover");

        assert!(event.success);
        assert_eq!(event.backup_id, Some(backup.id()));
        assert_eq!(
            coordinator.get_state(&agent.id()).expect("get state"),
            FailoverState::FailedOver
        );
    }

    #[test]
    fn test_recovery() {
        let config = FailoverConfig::default();
        let coordinator = FailoverCoordinator::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        coordinator
            .register_agent(agent.id(), vec![])
            .expect("register agent");

        coordinator
            .report_failure(&agent.id(), "test failure")
            .expect("report failure");

        coordinator
            .report_recovery(&agent.id())
            .expect("report recovery");

        assert_eq!(
            coordinator.get_state(&agent.id()).expect("get state"),
            FailoverState::Active
        );
        assert_eq!(
            coordinator
                .get_failure_count(&agent.id())
                .expect("failure count"),
            0
        );
    }

    #[test]
    fn test_unregister_agent() {
        let config = FailoverConfig::default();
        let coordinator = FailoverCoordinator::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        coordinator
            .register_agent(agent.id(), vec![])
            .expect("register agent");

        coordinator
            .unregister_agent(&agent.id())
            .expect("unregister agent");

        assert!(coordinator.get_state(&agent.id()).is_err());
    }
}
