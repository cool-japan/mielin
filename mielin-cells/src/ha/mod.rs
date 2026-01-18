//! High Availability Module
//!
//! Provides high availability features for agent management including:
//! - Leader election for coordination
//! - Automatic failover mechanisms
//! - State replication across nodes
//! - Quorum-based decision making
//! - Health monitoring for clusters

pub mod failover;
pub mod health;
pub mod leader;
pub mod quorum;
pub mod replication;

pub use failover::{
    FailoverConfig, FailoverCoordinator, FailoverDecision, FailoverError, FailoverEvent,
    FailoverPolicy, FailoverResult, FailoverState, FailoverStrategy,
};
pub use health::{
    ClusterHealth, ClusterHealthMonitor, HealthConfig, HealthMetric, HealthStatus, HealthThreshold,
    NodeHealth,
};
pub use leader::{
    LeaderElection, LeaderElector, LeaderError, LeaderState, LeaderTerm, NodeId, VoteRequest,
    VoteResponse,
};
pub use quorum::{
    QuorumConfig, QuorumDecision, QuorumError, QuorumPolicy, QuorumResult, QuorumVote, QuorumVoter,
};
pub use replication::{
    ReplicationConfig, ReplicationError, ReplicationLog, ReplicationManager, ReplicationState,
    ReplicationStrategy, StateReplica,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HAError {
    #[error("Leader election failed: {0}")]
    LeaderElectionFailed(String),
    #[error("Failover failed: {0}")]
    FailoverFailed(String),
    #[error("Replication failed: {0}")]
    ReplicationFailed(String),
    #[error("Quorum not reached: {0}")]
    QuorumNotReached(String),
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
}
